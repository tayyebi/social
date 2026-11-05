//! A small SMTP submission client: EHLO, STARTTLS, AUTH PLAIN/LOGIN, DATA.
//!
//! Only what submission needs. No pipelining, no BURL, no DSN.

use std::io::{BufRead, BufReader, Write};

use crate::error::{MailError, MailResult};
use crate::mime;
use crate::net::{Conn, TlsMode};

struct Reply {
    code: u16,
    lines: Vec<String>,
}

pub struct SmtpClient {
    stream: BufReader<Conn>,
    client_name: String,
}

impl SmtpClient {
    /// Connect, greet, EHLO, and upgrade with STARTTLS when the mode asks for it.
    pub fn connect(
        host: &str,
        port: u16,
        mode: TlsMode,
        client_name: &str,
    ) -> MailResult<Self> {
        let conn = Conn::connect(host, port, mode)?;
        let mut client = SmtpClient {
            stream: BufReader::new(conn),
            client_name: client_name.to_string(),
        };
        let greeting = client.read_reply()?;
        client.expect(&greeting, 220, "server greeting")?;
        client.ehlo()?;
        if mode.is_starttls() {
            client.write_line("STARTTLS")?;
            let reply = client.read_reply()?;
            client.expect(&reply, 220, "STARTTLS")?;
            let upgraded = client.stream.into_inner().upgrade(host)?;
            let mut client = SmtpClient {
                stream: BufReader::new(upgraded),
                client_name: client_name.to_string(),
            };
            // RFC 3207: the session restarts, so EHLO again before AUTH.
            client.ehlo()?;
            return Ok(client);
        }
        Ok(client)
    }

    pub fn login(&mut self, username: &str, password: &str) -> MailResult<()> {
        let mut token = Vec::with_capacity(username.len() + password.len() + 2);
        token.push(0);
        token.extend_from_slice(username.as_bytes());
        token.push(0);
        token.extend_from_slice(password.as_bytes());
        let encoded = social_mail_crypto::base64::encode(&token);

        self.write_line(&format!("AUTH PLAIN {encoded}"))?;
        let reply = self.read_reply()?;
        match reply.code {
            235 => Ok(()),
            334 => {
                // Some servers ask for the token on a second line.
                self.write_line(&encoded)?;
                let reply = self.read_reply()?;
                if reply.code == 235 {
                    Ok(())
                } else {
                    Err(classify(
                        reply.code,
                        format!("sign-in failed: {}", reply.lines.join(" ")),
                    ))
                }
            }
            500 | 502 | 504 => self.login_legacy(username, password),
            _ => Err(classify(
                reply.code,
                format!("sign-in failed: {}", reply.lines.join(" ")),
            )),
        }
    }

    fn login_legacy(&mut self, username: &str, password: &str) -> MailResult<()> {
        self.write_line("AUTH LOGIN")?;
        let reply = self.read_reply()?;
        if reply.code != 334 {
            return Err(classify(
                reply.code,
                format!("sign-in failed: {}", reply.lines.join(" ")),
            ));
        }
        self.write_line(&social_mail_crypto::base64::encode(username.as_bytes()))?;
        let reply = self.read_reply()?;
        if reply.code != 334 {
            return Err(classify(
                reply.code,
                format!("sign-in failed: {}", reply.lines.join(" ")),
            ));
        }
        self.write_line(&social_mail_crypto::base64::encode(password.as_bytes()))?;
        let reply = self.read_reply()?;
        if reply.code == 235 {
            Ok(())
        } else {
            Err(classify(
                reply.code,
                format!("sign-in failed: {}", reply.lines.join(" ")),
            ))
        }
    }

    /// Hand one complete message to the server.
    pub fn send(&mut self, from: &str, to: &str, message: &[u8]) -> MailResult<()> {
        self.write_line(&format!("MAIL FROM:<{from}>"))?;
        let reply = self.read_reply()?;
        self.expect(&reply, 250, "MAIL FROM")?;

        self.write_line(&format!("RCPT TO:<{to}>"))?;
        let reply = self.read_reply()?;
        if reply.code != 250 && reply.code != 251 {
            return Err(classify(
                reply.code,
                format!("the server refused {to}: {}", reply.lines.join(" ")),
            ));
        }

        self.write_line("DATA")?;
        let reply = self.read_reply()?;
        self.expect(&reply, 354, "DATA")?;

        let stuffed = mime::dot_stuff(message);
        let out = self.stream.get_mut();
        out.write_all(&stuffed)?;
        out.write_all(b".\r\n")?;
        out.flush()?;

        let reply = self.read_reply()?;
        self.expect(&reply, 250, "sending the message")?;
        Ok(())
    }

    pub fn quit(&mut self) {
        if self.write_line("QUIT").is_ok() {
            let _ = self.read_reply();
        }
    }

    fn ehlo(&mut self) -> MailResult<()> {
        let name = self.client_name.clone();
        self.write_line(&format!("EHLO {name}"))?;
        let reply = self.read_reply()?;
        if reply.code == 250 {
            return Ok(());
        }
        self.write_line(&format!("HELO {name}"))?;
        let reply = self.read_reply()?;
        self.expect(&reply, 250, "HELO")?;
        Ok(())
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

    fn read_reply(&mut self) -> MailResult<Reply> {
        let mut lines = Vec::new();
        let mut code = 0u16;
        loop {
            let line = self.read_line()?;
            if line.len() < 4 {
                return Err(MailError::protocol(format!(
                    "malformed SMTP reply {line:?}"
                )));
            }
            let parsed: u16 = line[..3]
                .parse()
                .map_err(|_| MailError::protocol(format!("malformed SMTP code {line:?}")))?;
            code = parsed;
            let separator = line.as_bytes()[3];
            let text = line.get(4..).unwrap_or("").to_string();
            lines.push(text);
            match separator {
                b' ' => break,
                b'-' => continue,
                _ => {
                    return Err(MailError::protocol(format!(
                        "malformed SMTP continuation {line:?}"
                    )))
                }
            }
        }
        Ok(Reply { code, lines })
    }

    fn expect(&self, reply: &Reply, wanted: u16, what: &str) -> MailResult<()> {
        if reply.code == wanted {
            return Ok(());
        }
        Err(classify(
            reply.code,
            format!("{what}: {}", reply.lines.join(" ")),
        ))
    }
}

fn classify(code: u16, message: String) -> MailError {
    match code {
        530 | 534 | 535 | 538 => MailError::auth(message),
        552 | 553 => MailError::too_large(message),
        421 | 450 | 451 | 452 | 454 => MailError::transient(message),
        code if code >= 500 => MailError::permanent(message),
        _ => MailError::transient(message),
    }
}
