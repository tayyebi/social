//! End-to-end tests against in-process fake servers.
//!
//! No network egress, no credentials, no TLS: the servers speak just enough SMTP
//! and IMAP to exercise the client, and they record what they received so the
//! assertions are about bytes on the wire.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread::{self, JoinHandle};

use social_mail_mail::{build_message, FailureKind, MailConfig, MailTransport, TlsMode};

fn listener() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port)
}

/// A fake SMTP submission server. Returns the port and the bytes it received.
fn fake_smtp(user: &'static str, password: &'static str) -> (u16, JoinHandle<Vec<u8>>) {
    let (listener, port) = listener();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        write!(stream, "220 fake ESMTP ready\r\n").unwrap();
        let mut received = Vec::new();
        let mut in_data = false;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap() == 0 {
                break;
            }
            if in_data {
                if line == ".\r\n" {
                    in_data = false;
                    write!(stream, "250 2.0.0 queued\r\n").unwrap();
                    continue;
                }
                let unstuffed = line.strip_prefix('.').unwrap_or(&line);
                received.extend_from_slice(unstuffed.as_bytes());
                continue;
            }
            let command = line.trim_end().to_string();
            if command.starts_with("EHLO") {
                write!(
                    stream,
                    "250-fake\r\n250-AUTH PLAIN LOGIN\r\n250 8BITMIME\r\n"
                )
                .unwrap();
            } else if let Some(token) = command.strip_prefix("AUTH PLAIN ") {
                let decoded = social_mail_crypto::base64::decode(token).unwrap();
                let parts: Vec<&[u8]> = decoded.split(|byte| *byte == 0).collect();
                let ok = parts.len() >= 3
                    && parts[1] == user.as_bytes()
                    && parts[2] == password.as_bytes();
                if ok {
                    write!(stream, "235 2.7.0 authenticated\r\n").unwrap();
                } else {
                    write!(stream, "535 5.7.8 credentials rejected\r\n").unwrap();
                }
            } else if command.starts_with("MAIL FROM") {
                write!(stream, "250 2.1.0 ok\r\n").unwrap();
            } else if command.starts_with("RCPT TO") {
                write!(stream, "250 2.1.5 ok\r\n").unwrap();
            } else if command == "DATA" {
                write!(stream, "354 go ahead\r\n").unwrap();
                in_data = true;
            } else if command == "QUIT" {
                write!(stream, "221 bye\r\n").unwrap();
                break;
            } else {
                write!(stream, "250 ok\r\n").unwrap();
            }
        }
        received
    });
    (port, handle)
}

/// A fake IMAP server holding `messages`, recording the commands it saw.
fn fake_imap(messages: Vec<(u32, Vec<u8>)>) -> (u16, JoinHandle<Vec<String>>) {
    let (listener, port) = listener();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        write!(stream, "* OK fake IMAP ready\r\n").unwrap();
        let mut commands = Vec::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap() == 0 {
                break;
            }
            let line = line.trim_end().to_string();
            let (tag, rest) = match line.split_once(' ') {
                Some((tag, rest)) => (tag.to_string(), rest.to_string()),
                None => (line.clone(), String::new()),
            };
            commands.push(line.clone());
            let upper = rest.to_ascii_uppercase();
            if upper.starts_with("LOGIN") {
                write!(stream, "{tag} OK logged in\r\n").unwrap();
            } else if upper.starts_with("SELECT") {
                write!(
                    stream,
                    "* {} EXISTS\r\n* OK [UIDVALIDITY 7] ok\r\n{tag} OK [READ-WRITE] selected\r\n",
                    messages.len()
                )
                .unwrap();
            } else if upper.starts_with("UID SEARCH") {
                let list: Vec<String> = messages.iter().map(|(uid, _)| uid.to_string()).collect();
                write!(
                    stream,
                    "* SEARCH {}\r\n{tag} OK search done\r\n",
                    list.join(" ")
                )
                .unwrap();
            } else if upper.starts_with("UID FETCH") {
                let uid: u32 = rest
                    .split_whitespace()
                    .nth(2)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
                match messages.iter().find(|(value, _)| *value == uid) {
                    Some((_, body)) => {
                        write!(stream, "* 1 FETCH (UID {uid} BODY[] {{{}}}\r\n", body.len())
                            .unwrap();
                        stream.write_all(body).unwrap();
                        write!(stream, ")\r\n{tag} OK fetch done\r\n").unwrap();
                    }
                    None => write!(stream, "{tag} OK fetch done\r\n").unwrap(),
                }
            } else if upper.starts_with("UID STORE") {
                write!(stream, "{tag} OK stored\r\n").unwrap();
            } else if upper.starts_with("UID COPY") {
                write!(stream, "{tag} OK copied\r\n").unwrap();
            } else if upper.starts_with("CREATE") {
                write!(stream, "{tag} NO [ALREADYEXISTS] already there\r\n").unwrap();
            } else if upper.starts_with("EXPUNGE") {
                write!(stream, "* 1 EXPUNGE\r\n{tag} OK expunged\r\n").unwrap();
            } else if upper.starts_with("LOGOUT") {
                write!(stream, "* BYE\r\n{tag} OK bye\r\n").unwrap();
                break;
            } else {
                write!(stream, "{tag} OK done\r\n").unwrap();
            }
        }
        commands
    });
    (port, handle)
}

fn config(port: u16) -> MailConfig {
    MailConfig {
        account: "alice@example.org".to_string(),
        imap_host: "127.0.0.1".to_string(),
        imap_port: port,
        imap_tls: TlsMode::None,
        smtp_host: "127.0.0.1".to_string(),
        smtp_port: port,
        smtp_tls: TlsMode::None,
        username: String::new(),
        password_env: "SOCIAL_MAIL_TEST_PASSWORD".to_string(),
        folder: "INBOX".to_string(),
        protocol_folder: "SocialMail".to_string(),
        move_after_reading: true,
    }
}

fn connect(port: u16) -> MailTransport {
    MailTransport::with_password(config(port), "app-password").unwrap()
}

#[test]
fn sends_a_carrier_over_smtp() {
    let (port, server) = fake_smtp("alice@example.org", "app-password");
    let transport = connect(port);
    transport
        .send("bob@example.org", "{\"kind\":\"social-mail/carrier\"}")
        .unwrap();
    let received = server.join().unwrap();
    let text = String::from_utf8_lossy(&received);

    assert!(text.contains("Subject: SocialMail/v1 carrier"), "{text}");
    assert!(text.contains("To: bob@example.org"), "{text}");
    assert!(text.contains("X-SocialMail: v1"), "{text}");
    assert!(text.contains("Content-Transfer-Encoding: base64"), "{text}");

    let split = text.find("\r\n\r\n").expect("header block");
    let decoded = social_mail_crypto::base64::decode(&text[split + 4..]).unwrap();
    assert_eq!(
        String::from_utf8(decoded).unwrap(),
        "{\"kind\":\"social-mail/carrier\"}"
    );
}

#[test]
fn bad_credentials_are_an_auth_failure() {
    let (port, server) = fake_smtp("alice@example.org", "the-right-one");
    let transport = MailTransport::with_password(config(port), "the-wrong-one").unwrap();
    let error = transport.send("bob@example.org", "{}").unwrap_err();
    assert_eq!(error.kind, FailureKind::Auth);
    let _ = server.join();
}

#[test]
fn fetches_only_social_mail() {
    let carrier = build_message(
        "carol@example.org",
        "alice@example.org",
        "SocialMail/v1 carrier",
        b"{\"x\":1}",
    );
    let ordinary = b"From: friend@example.org\r\nSubject: lunch?\r\n\r\none o'clock\r\n".to_vec();
    let (port, server) = fake_imap(vec![(1, ordinary), (2, carrier)]);

    let transport = connect(port);
    let messages = transport.fetch(10).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body, "{\"x\":1}");
    assert_eq!(messages[0].reference, "INBOX#2");
    assert_eq!(messages[0].uid, 2);
    let commands = server.join().unwrap();
    assert!(commands
        .iter()
        .any(|command| command.contains("UID FETCH 2 (BODY.PEEK[])")));
}

#[test]
fn acknowledging_moves_mail_to_the_protocol_folder() {
    let (port, server) = fake_imap(Vec::new());
    let transport = connect(port);
    transport.acknowledge("INBOX#3").unwrap();
    let commands = server.join().unwrap();
    assert!(commands
        .iter()
        .any(|command| command.contains("UID STORE 3 +FLAGS (\\Seen)")));
    assert!(commands
        .iter()
        .any(|command| command.contains("UID COPY 3 \"SocialMail\"")));
    assert!(commands
        .iter()
        .any(|command| command.contains("+FLAGS (\\Deleted)")));
    assert!(commands.iter().any(|command| command.contains("EXPUNGE")));
}

#[test]
fn quarantine_marks_read_and_leaves_the_message() {
    let (port, server) = fake_imap(Vec::new());
    let transport = MailTransport::with_password(
        MailConfig {
            move_after_reading: false,
            ..config(port)
        },
        "app-password",
    )
    .unwrap();
    transport.quarantine("INBOX#9").unwrap();
    let commands = server.join().unwrap();
    assert!(commands
        .iter()
        .any(|command| command.contains("UID STORE 9 +FLAGS (\\Seen)")));
    assert!(!commands.iter().any(|command| command.contains("UID COPY")));
    assert!(!commands.iter().any(|command| command.contains("EXPUNGE")));
}

#[test]
fn probe_reports_both_protocols() {
    // The probe talks to IMAP and then SMTP, so two listeners are needed.
    let (imap_port, imap_server) = fake_imap(Vec::new());
    let (smtp_port, smtp_server) = fake_smtp("alice@example.org", "app-password");
    let transport = MailTransport::with_password(
        MailConfig {
            smtp_port,
            ..config(imap_port)
        },
        "app-password",
    )
    .unwrap();
    let summary = transport.probe().unwrap();
    assert!(summary.contains("127.0.0.1"), "{summary}");
    let _ = imap_server.join();
    let _ = smtp_server.join();
}
