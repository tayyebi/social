//! A small IMAP client: LOGIN, SELECT, UID SEARCH, UID FETCH, COPY, STORE, EXPUNGE.
//!
//! It understands exactly the responses those commands produce, including
//! literals (`{123}` followed by 123 raw bytes). Anything else is reported, not
//! guessed at.

use std::io::{BufRead, BufReader, Read, Write};

use crate::error::{MailError, MailResult};
use crate::net::{Conn, TlsMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    No,
    Bad,
    Unknown,
}

struct Response {
    status: Status,
    untagged: Vec<String>,
    literals: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub struct MailboxInfo {
    pub exists: u32,
    pub uid_validity: Option<u32>,
}

pub struct ImapClient {
    stream: BufReader<Conn>,
    tag: u32,
}

impl ImapClient {
    pub fn connect(host: &str, port: u16, mode: TlsMode) -> MailResult<Self> {
        let conn = Conn::connect(host, port, mode)?;
        let mut client = ImapClient {
            stream: BufReader::new(conn),
            tag: 0,
        };
        let greeting = client.read_line()?;
        let upper = greeting.to_ascii_uppercase();
        if !(upper.starts_with("* OK") || upper.starts_with("* PREAUTH")) {
            return Err(MailError::protocol(format!(
                "unexpected IMAP greeting {greeting:?}"
            )));
        }
        if mode.is_starttls() {
            let tag = client.next_tag();
            client.write_line(&format!("{tag} STARTTLS"))?;
            let response = client.read_until_tag(&tag)?;
            if response.status != Status::Ok {
                return Err(classify(&response, "STARTTLS"));
            }
            let upgraded = client.stream.into_inner().upgrade(host)?;
            client.stream = BufReader::new(upgraded);
        }
        Ok(client)
    }

    pub fn login(&mut self, username: &str, password: &str) -> MailResult<()> {
        let tag = self.next_tag();
        self.write_line(&format!(
            "{tag} LOGIN {} {}",
            quote(username),
            quote(password)
        ))?;
        let response = self.read_until_tag(&tag)?;
        if response.status == Status::Ok {
            Ok(())
        } else {
            Err(classify(&response, "sign-in"))
        }
    }

    pub fn select(&mut self, folder: &str) -> MailResult<MailboxInfo> {
        let tag = self.next_tag();
        self.write_line(&format!("{tag} SELECT {}", quote(folder)))?;
        let response = self.read_until_tag(&tag)?;
        if response.status != Status::Ok {
            return Err(classify(&response, &format!("opening {folder}")));
        }
        let mut info = MailboxInfo::default();
        for line in &response.untagged {
            let lowered = line.to_ascii_lowercase();
            if lowered.ends_with(" exists") {
                if let Some(number) = lowered.split_whitespace().nth(1) {
                    info.exists = number.parse().unwrap_or(0);
                }
            }
            if let Some(start) = lowered.find("[uidvalidity ") {
                let digits: String = lowered[start + 13..]
                    .chars()
                    .take_while(|ch| ch.is_ascii_digit())
                    .collect();
                if let Ok(value) = digits.parse() {
                    info.uid_validity = Some(value);
                }
            }
        }
        Ok(info)
    }

    /// Every message UID in the mailbox, oldest first.
    pub fn search_all(&mut self) -> MailResult<Vec<u32>> {
        let tag = self.next_tag();
        self.write_line(&format!("{tag} UID SEARCH ALL"))?;
        let response = self.read_until_tag(&tag)?;
        if response.status != Status::Ok {
            return Err(classify(&response, "searching the mailbox"));
        }
        let mut uids = Vec::new();
        for line in &response.untagged {
            let upper = line.to_ascii_uppercase();
            if let Some(rest) = upper.strip_prefix("* SEARCH") {
                for token in rest.split_whitespace() {
                    if let Ok(uid) = token.parse::<u32>() {
                        uids.push(uid);
                    }
                }
            }
        }
        uids.sort_unstable();
        Ok(uids)
    }

    /// Fetch one message body without setting the \Seen flag.
    pub fn fetch(&mut self, uid: u32) -> MailResult<Vec<u8>> {
        let tag = self.next_tag();
        self.write_line(&format!("{tag} UID FETCH {uid} (BODY.PEEK[])"))?;
        let response = self.read_until_tag(&tag)?;
        if response.status != Status::Ok {
            return Err(classify(&response, "fetching a message"));
        }
        response
            .literals
            .into_iter()
            .next()
            .ok_or_else(|| MailError::protocol(format!("no body returned for UID {uid}")))
    }

    pub fn mark_seen(&mut self, uid: u32) -> MailResult<()> {
        let tag = self.next_tag();
        self.write_line(&format!("{tag} UID STORE {uid} +FLAGS (\\Seen)"))?;
        let response = self.read_until_tag(&tag)?;
        if response.status == Status::Ok {
            Ok(())
        } else {
            Err(classify(&response, "flagging a message"))
        }
    }

    pub fn mark_deleted(&mut self, uid: u32) -> MailResult<()> {
        let tag = self.next_tag();
        self.write_line(&format!("{tag} UID STORE {uid} +FLAGS (\\Deleted)"))?;
        let response = self.read_until_tag(&tag)?;
        if response.status == Status::Ok {
            Ok(())
        } else {
            Err(classify(&response, "deleting a message"))
        }
    }

    pub fn copy(&mut self, uid: u32, folder: &str) -> MailResult<()> {
        let tag = self.next_tag();
        self.write_line(&format!("{tag} UID COPY {uid} {}", quote(folder)))?;
        let response = self.read_until_tag(&tag)?;
        if response.status == Status::Ok {
            Ok(())
        } else {
            Err(classify(&response, "copying a message"))
        }
    }

    /// Create a folder. An existing folder answers `NO`; that is not an error here.
    pub fn ensure_folder(&mut self, folder: &str) -> MailResult<()> {
        let tag = self.next_tag();
        self.write_line(&format!("{tag} CREATE {}", quote(folder)))?;
        let response = self.read_until_tag(&tag)?;
        match response.status {
            Status::Ok | Status::No => Ok(()),
            _ => Err(classify(&response, "creating the protocol folder")),
        }
    }

    pub fn expunge(&mut self) -> MailResult<()> {
        let tag = self.next_tag();
        self.write_line(&format!("{tag} EXPUNGE"))?;
        let response = self.read_until_tag(&tag)?;
        if response.status == Status::Ok {
            Ok(())
        } else {
            Err(classify(&response, "expunging"))
        }
    }

    pub fn logout(&mut self) {
        let tag = self.next_tag();
        if self.write_line(&format!("{tag} LOGOUT")).is_ok() {
            let _ = self.read_until_tag(&tag);
        }
    }

    fn next_tag(&mut self) -> String {
        self.tag += 1;
        format!("A{}", self.tag)
    }

    fn write_line(&mut self, line: &str) -> MailResult<()> {
        let out = self.stream.get_mut();
        out.write_all(line.as_bytes())?;
        out.write_all(b"\r\n")?;
        out.flush()?;
        Ok(())
    }

    fn read_line(&mut self) -> MailResult<String> {
        let mut line = String::new();
        let read = self.stream.read_line(&mut line)?;
        if read == 0 {
            return Err(MailError::transient(
                "the mail server closed the connection",
            ));
        }
        while line.ends_with('\n') || line.ends_with('\r') {
            line.pop();
        }
        Ok(line)
    }

    fn read_until_tag(&mut self, tag: &str) -> MailResult<Response> {
        let mut untagged = Vec::new();
        let mut literals = Vec::new();
        loop {
            let mut line = self.read_line()?;
            // A line may announce a literal that follows immediately.
            loop {
                let Some(length) = literal_length(&line) else {
                    break;
                };
                let mut bytes = vec![0u8; length];
                self.stream.read_exact(&mut bytes)?;
                literals.push(bytes);
                line = self.read_line()?;
            }
            if let Some(rest) = line.strip_prefix(tag) {
                let rest = rest.trim_start();
                let lowered = rest.to_ascii_lowercase();
                let status = if lowered.starts_with("ok") {
                    Status::Ok
                } else if lowered.starts_with("no") {
                    Status::No
                } else if lowered.starts_with("bad") {
                    Status::Bad
                } else {
                    Status::Unknown
                };
                if !rest.is_empty() {
                    untagged.push(rest.to_string());
                }
                return Ok(Response {
                    status,
                    untagged,
                    literals,
                });
            }
            if line.starts_with('+') {
                continue;
            }
            untagged.push(line);
        }
    }
}

fn literal_length(line: &str) -> Option<usize> {
    let trimmed = line.trim_end();
    if !trimmed.ends_with('}') {
        return None;
    }
    let open = trimmed.rfind('{')?;
    trimmed
        .get(open + 1..trimmed.len() - 1)?
        .trim()
        .parse()
        .ok()
}

fn quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn classify(response: &Response, what: &str) -> MailError {
    let detail = response.untagged.join(" ");
    let lowered = detail.to_ascii_lowercase();
    let message = format!("{what}: {}", detail.trim());
    let auth_hint = lowered.contains("authenticationfailed")
        || lowered.contains("invalid credentials")
        || lowered.contains("authentication failed")
        || lowered.contains("authorization failed")
        || lowered.contains("login failed");
    if auth_hint {
        MailError::auth(message)
    } else if lowered.contains("too large") || lowered.contains("toolarge") {
        MailError::too_large(message)
    } else if lowered.contains("unavailable")
        || lowered.contains("try again")
        || lowered.contains("server busy")
        || lowered.contains("overload")
    {
        MailError::transient(message)
    } else {
        MailError::permanent(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_literal_lengths() {
        assert_eq!(literal_length("* 1 FETCH (BODY[] {42}"), Some(42));
        assert_eq!(literal_length("* 1 FETCH (BODY[] {42})"), None);
        assert_eq!(literal_length("no literal here"), None);
    }

    #[test]
    fn quotes_login_arguments() {
        assert_eq!(quote("alice@example.org"), "\"alice@example.org\"");
        assert_eq!(quote("we\"ird\\"), "\"we\\\"ird\\\\\"");
    }

    #[test]
    fn classifies_rejections() {
        let response = Response {
            status: Status::No,
            untagged: vec!["NO [AUTHENTICATIONFAILED] Invalid credentials".to_string()],
            literals: Vec::new(),
        };
        assert_eq!(
            classify(&response, "sign-in").kind,
            crate::FailureKind::Auth
        );
    }
}
