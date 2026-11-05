//! The envelope around a carrier: one plain-text, base64 message.
//!
//! Base64 keeps line lengths sane, avoids dot-stuffing surprises and survives
//! every provider's 8-bit handling. The subject is the only thing a mail client
//! sees, so it says what the message is and nothing else.

use chrono::Utc;

use crate::error::{MailError, MailResult};

/// Subject prefix that marks our own mail, Delta Chat style.
pub const SUBJECT_PREFIX: &str = "SocialMail/";

#[derive(Debug, Clone)]
pub struct ParsedMessage {
    pub subject: String,
    pub from: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl ParsedMessage {
    /// True when this looks like one of our carriers rather than ordinary mail.
    pub fn is_social_mail(&self) -> bool {
        self.subject.starts_with(SUBJECT_PREFIX) || self.header("x-socialmail").is_some()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Build one message carrying `body` (the carrier JSON).
pub fn build_message(from: &str, to: &str, subject: &str, body: &[u8]) -> Vec<u8> {
    let now = Utc::now();
    let message_id = format!(
        "<{}-{}@social-mail.local>",
        now.timestamp_nanos_opt().unwrap_or_default(),
        std::process::id()
    );
    let encoded = wrap64(&social_mail_crypto::base64::encode(body));

    let mut out = String::with_capacity(encoded.len() + 512);
    out.push_str(&format!("From: {from}\r\n"));
    out.push_str(&format!("To: {to}\r\n"));
    out.push_str(&format!("Subject: {subject}\r\n"));
    out.push_str(&format!("Date: {}\r\n", now.format("%a, %d %b %Y %H:%M:%S +0000")));
    out.push_str(&format!("Message-ID: {message_id}\r\n"));
    out.push_str("X-SocialMail: v1\r\n");
    out.push_str("MIME-Version: 1.0\r\n");
    out.push_str("Content-Type: text/plain; charset=utf-8\r\n");
    out.push_str("Content-Transfer-Encoding: base64\r\n");
    out.push_str("\r\n");
    out.push_str(&encoded);
    out.push_str("\r\n");
    out.into_bytes()
}

/// Normalise line endings and dot-stuff the body for SMTP `DATA`.
pub fn dot_stuff(message: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(message);
    let normalised = text.replace("\r\n", "\n");
    let trimmed = normalised.trim_end_matches('\n');
    let mut out = Vec::with_capacity(message.len() + 32);
    for line in trimmed.split('\n') {
        if line.starts_with('.') {
            out.push(b'.');
        }
        out.extend_from_slice(line.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    out
}

/// Parse headers and decode the body of a fetched message.
pub fn parse_message(raw: &[u8]) -> MailResult<ParsedMessage> {
    let (split, width) = find_header_end(raw)
        .ok_or_else(|| MailError::protocol("the message has no header block"))?;
    let headers = parse_headers(&raw[..split]);
    let encoded_body = &raw[split + width..];

    let subject = header_value(&headers, "subject").unwrap_or_default();
    let from = header_value(&headers, "from").unwrap_or_default();
    let encoding = header_value(&headers, "content-transfer-encoding")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let content_type = header_value(&headers, "content-type")
        .unwrap_or_default()
        .to_ascii_lowercase();

    if content_type.starts_with("multipart/") {
        return Err(MailError::protocol(
            "multipart messages are not supported in v0.2",
        ));
    }

    let body = match encoding.trim() {
        "base64" => {
            let text = String::from_utf8_lossy(encoded_body);
            social_mail_crypto::base64::decode(&text)
                .map_err(|error| MailError::protocol(format!("base64 body: {error}")))?
        }
        "quoted-printable" => decode_quoted_printable(encoded_body),
        _ => trim_terminator(encoded_body).to_vec(),
    };

    Ok(ParsedMessage {
        subject,
        from,
        headers,
        body,
    })
}

fn find_header_end(raw: &[u8]) -> Option<(usize, usize)> {
    if let Some(index) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
        return Some((index, 4));
    }
    raw.windows(2)
        .position(|window| window == b"\n\n")
        .map(|index| (index, 2))
}

fn parse_headers(head: &[u8]) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(head);
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(last) = headers.last_mut() {
                last.1.push(' ');
                last.1.push_str(line.trim());
            }
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    headers
}

fn header_value(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}

fn trim_terminator(body: &[u8]) -> &[u8] {
    let mut end = body.len();
    while end > 0 && (body[end - 1] == b'\n' || body[end - 1] == b'\r') {
        end -= 1;
    }
    &body[..end]
}

fn decode_quoted_printable(body: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(body);
    let mut out = Vec::with_capacity(body.len());
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'=' => {
                if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    index += 2;
                    continue;
                }
                if index + 2 < bytes.len() {
                    let hex = &text[index + 1..index + 3];
                    if let Ok(value) = u8::from_str_radix(hex, 16) {
                        out.push(value);
                        index += 3;
                        continue;
                    }
                }
                out.push(b'=');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    out
}

fn wrap64(encoded: &str) -> String {
    let mut out = String::with_capacity(encoded.len() + encoded.len() / 64 * 2);
    for chunk in encoded.as_bytes().chunks(76) {
        out.push_str(&String::from_utf8_lossy(chunk));
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_carrier() {
        let payload = br#"{"kind":"social-mail/carrier","version":1}"#;
        let message = build_message(
            "alice@example.org",
            "bob@example.org",
            &format!("{SUBJECT_PREFIX}v1 carrier"),
            payload,
        );
        let parsed = parse_message(&message).unwrap();
        assert_eq!(parsed.from, "alice@example.org");
        assert!(parsed.is_social_mail());
        assert_eq!(parsed.body, payload);
    }

    #[test]
    fn ordinary_mail_is_not_ours() {
        let message = b"From: friend@example.org\r\nSubject: lunch?\r\n\r\nsee you at one\r\n";
        let parsed = parse_message(message).unwrap();
        assert!(!parsed.is_social_mail());
        assert_eq!(parsed.body, b"see you at one");
    }

    #[test]
    fn dot_stuffing_preserves_leading_dots() {
        let message = b"From: a\r\n\r\n.hidden\r\n..double\r\nplain\r\n";
        let stuffed = dot_stuff(message);
        let text = String::from_utf8_lossy(&stuffed);
        assert!(text.contains("\r\n..hidden\r\n"));
        assert!(text.contains("\r\n...double\r\n"));
        assert!(text.ends_with("\r\n"));
    }

    #[test]
    fn folded_headers_are_unfolded() {
        let message = b"X-SocialMail: v1\r\nSubject: SocialMail/v1\r\n  carrier\r\n\r\naGk=\r\n";
        let parsed = parse_message(message).unwrap();
        assert_eq!(parsed.subject, "SocialMail/v1 carrier");
        assert_eq!(parsed.body, b"hi");
    }

    #[test]
    fn rejects_multipart_for_now() {
        let message = b"Subject: SocialMail/x\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n--x--\r\n";
        assert!(parse_message(message).is_err());
    }
}
