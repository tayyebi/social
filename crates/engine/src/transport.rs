//! Transports: the shared spool directory (local mode) or a real mailbox.
//!
//! Both implement [`Transport`]. The engine only ever sees this trait, so the
//! decision "is this a spool file or an SMTP submission" stays here, and the
//! delivery state machine never learns about mail servers.
//!
//! Failure kinds matter: `Auth` means the human must fix credentials before any
//! retry can work, `Transient` means try later, `Permanent` and `TooLarge` mean
//! the message itself is the problem.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use social_mail_mail::{FailureKind, MailConfig, MailTransport, TlsMode};

use crate::config::Config;
use crate::EngineError;

/// One message pulled from a transport, with a handle for ack/quarantine.
#[derive(Clone, Debug)]
pub struct Incoming {
    /// Spool: the file path. Mail: `folder#uid`.
    pub reference: String,
    pub body: String,
}

/// Where one copy of an event is going.
#[derive(Clone, Copy, Debug)]
pub struct Recipient<'a> {
    pub fingerprint: &'a str,
    /// Delivery address; the spool transport ignores it, mail needs it.
    pub address: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportFailure {
    Auth,
    Transient,
    Permanent,
    TooLarge,
    Unavailable,
}

#[derive(Clone, Debug)]
pub struct TransportError {
    pub failure: TransportFailure,
    pub message: String,
}

impl TransportError {
    pub fn new(failure: TransportFailure, message: impl Into<String>) -> Self {
        TransportError {
            failure,
            message: message.into(),
        }
    }

    pub fn transient(message: impl Into<String>) -> Self {
        Self::new(TransportFailure::Transient, message)
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for TransportError {}

impl From<TransportError> for EngineError {
    fn from(error: TransportError) -> Self {
        EngineError::Transport(error.message)
    }
}

impl From<social_mail_mail::MailError> for TransportError {
    fn from(error: social_mail_mail::MailError) -> Self {
        let failure = match error.kind {
            FailureKind::Auth => TransportFailure::Auth,
            FailureKind::Transient => TransportFailure::Transient,
            FailureKind::Permanent => TransportFailure::Permanent,
            FailureKind::TooLarge => TransportFailure::TooLarge,
            FailureKind::Config | FailureKind::Protocol => TransportFailure::Permanent,
        };
        TransportError::new(failure, error.message)
    }
}

pub trait Transport {
    fn send(&self, to: &Recipient<'_>, body: &str) -> Result<(), TransportError>;
    fn receive(&self) -> Result<Vec<Incoming>, TransportError>;
    fn acknowledge(&self, incoming: &Incoming) -> Result<(), TransportError>;
    fn quarantine(&self, incoming: &Incoming) -> Result<(), TransportError>;
}

// ------------------------------------------------------------------- spool

/// A shared directory that any file-sync tool (or two profiles on one machine)
/// can move. Still supported, still tested, still the local-mode story.
pub struct SpoolTransport {
    root: PathBuf,
    me: String,
}

impl SpoolTransport {
    pub fn new(root: impl Into<PathBuf>, me: impl Into<String>) -> Result<Self, EngineError> {
        let root = root.into();
        let me = me.into();
        if me.is_empty() || !me.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(EngineError::Transport(
                "spool recipient must be a hex fingerprint".into(),
            ));
        }
        fs::create_dir_all(root.join(&me))?;
        Ok(SpoolTransport { root, me })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn send_to(&self, recipient: &str, body: &str) -> Result<String, EngineError> {
        if recipient.is_empty() || !recipient.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(EngineError::Transport(format!(
                "cannot spool a message for {recipient:?}"
            )));
        }
        let dir = self.root.join(recipient);
        fs::create_dir_all(&dir)?;
        let stamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
        let name = format!("{stamp:020}-{}.json", std::process::id());
        let temp = dir.join(format!(".{name}.tmp"));
        fs::write(&temp, body)?;
        fs::rename(&temp, dir.join(&name))?;
        Ok(name)
    }

    pub fn receive_all(&self) -> Result<Vec<Incoming>, EngineError> {
        let dir = self.root.join(&self.me);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut paths: Vec<PathBuf> = fs::read_dir(&dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().map(|ext| ext == "json").unwrap_or(false))
            .collect();
        paths.sort();
        let mut out = Vec::new();
        for path in paths {
            let body = fs::read_to_string(&path)?;
            out.push(Incoming {
                reference: path.to_string_lossy().to_string(),
                body,
            });
        }
        Ok(out)
    }

    pub fn acknowledge_reference(&self, reference: &str) -> Result<(), EngineError> {
        let path = PathBuf::from(reference);
        let done = path.with_extension("json.done");
        fs::rename(path, done)?;
        Ok(())
    }

    pub fn quarantine_reference(&self, reference: &str) -> Result<(), EngineError> {
        let path = PathBuf::from(reference);
        let rejected = path.with_extension("json.rejected");
        fs::rename(path, rejected)?;
        Ok(())
    }
}

impl Transport for SpoolTransport {
    fn send(&self, to: &Recipient<'_>, body: &str) -> Result<(), TransportError> {
        self.send_to(to.fingerprint, body)
            .map(|_| ())
            .map_err(|error| TransportError::transient(error.to_string()))
    }

    fn receive(&self) -> Result<Vec<Incoming>, TransportError> {
        self.receive_all()
            .map_err(|error| TransportError::transient(error.to_string()))
    }

    fn acknowledge(&self, incoming: &Incoming) -> Result<(), TransportError> {
        self.acknowledge_reference(&incoming.reference)
            .map_err(|error| TransportError::transient(error.to_string()))
    }

    fn quarantine(&self, incoming: &Incoming) -> Result<(), TransportError> {
        self.quarantine_reference(&incoming.reference)
            .map_err(|error| TransportError::transient(error.to_string()))
    }
}

// -------------------------------------------------------------------- mail

/// IMAP receive, SMTP send, app passwords.
pub struct MailboxTransport {
    mail: MailTransport,
}

impl fmt::Debug for MailboxTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MailboxTransport")
            .field("mail", &self.mail)
            .finish()
    }
}

impl MailboxTransport {
    pub fn new(config: &Config) -> Result<Self, TransportError> {
        let mail = MailTransport::from_env(mail_config(config)?)?;
        Ok(MailboxTransport { mail })
    }

    pub fn probe(&self) -> Result<String, TransportError> {
        Ok(self.mail.probe()?)
    }

    pub fn send_test(&self, carrier: &str) -> Result<String, TransportError> {
        Ok(self.mail.send_test(carrier)?)
    }
}

pub fn mail_config(config: &Config) -> Result<MailConfig, TransportError> {
    let imap_tls = TlsMode::parse(&config.imap_tls)?;
    let smtp_tls = TlsMode::parse(&config.smtp_tls)?;
    Ok(MailConfig {
        account: config.account.clone(),
        imap_host: config.imap_host.clone(),
        imap_port: config.imap_port.clamp(1, 65535) as u16,
        imap_tls,
        smtp_host: config.smtp_host.clone(),
        smtp_port: config.smtp_port.clamp(1, 65535) as u16,
        smtp_tls,
        username: config.username.clone(),
        password_env: config.password_env.clone(),
        folder: config.mail_folder.clone(),
        protocol_folder: config.protocol_folder.clone(),
        move_after_reading: config.move_protocol_messages,
    })
}

impl Transport for MailboxTransport {
    fn send(&self, to: &Recipient<'_>, body: &str) -> Result<(), TransportError> {
        self.mail.send(to.address, body)?;
        Ok(())
    }

    fn receive(&self) -> Result<Vec<Incoming>, TransportError> {
        let messages = self.mail.fetch(50)?;
        Ok(messages
            .into_iter()
            .map(|message| Incoming {
                reference: message.reference,
                body: message.body,
            })
            .collect())
    }

    fn acknowledge(&self, incoming: &Incoming) -> Result<(), TransportError> {
        self.mail.acknowledge(&incoming.reference)?;
        Ok(())
    }

    fn quarantine(&self, incoming: &Incoming) -> Result<(), TransportError> {
        self.mail.quarantine(&incoming.reference)?;
        Ok(())
    }
}

/// Build the transport the configuration asks for.
pub fn for_config(config: &Config, me: &str) -> Result<Box<dyn Transport>, EngineError> {
    match config.transport.as_str() {
        "mail" | "imap" => Ok(Box::new(MailboxTransport::new(config)?)),
        _ => Ok(Box::new(SpoolTransport::new(
            config.spool_dir.clone(),
            me.to_string(),
        )?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn temp_root() -> PathBuf {
        let unique = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "sm-spool-{}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
            unique
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn recipient(fingerprint: &str) -> Recipient<'_> {
        Recipient {
            fingerprint,
            address: "someone@example.org",
        }
    }

    #[test]
    fn spools_and_receives_a_message() {
        let root = temp_root();
        let alice = SpoolTransport::new(&root, "AA".repeat(20)).unwrap();
        let bob = SpoolTransport::new(&root, "BB".repeat(20)).unwrap();
        let bob_fingerprint = "BB".repeat(20);
        alice
            .send(&recipient(&bob_fingerprint), "{\"hello\":true}")
            .unwrap();
        let incoming = bob.receive().unwrap();
        assert_eq!(incoming.len(), 1);
        assert_eq!(incoming[0].body, "{\"hello\":true}");
        bob.acknowledge(&incoming[0]).unwrap();
        assert!(bob.receive().unwrap().is_empty());
    }

    #[test]
    fn rejects_non_hex_recipients() {
        let root = temp_root();
        let alice = SpoolTransport::new(&root, "AA".repeat(20)).unwrap();
        assert!(alice.send(&recipient("bob@example.org"), "{}").is_err());
        assert!(SpoolTransport::new(&root, "../evil").is_err());
    }

    #[test]
    fn quarantine_moves_the_file_aside() {
        let root = temp_root();
        let alice = SpoolTransport::new(&root, "AA".repeat(20)).unwrap();
        let bob = SpoolTransport::new(&root, "BB".repeat(20)).unwrap();
        let bob_fingerprint = "BB".repeat(20);
        alice.send(&recipient(&bob_fingerprint), "garbage").unwrap();
        let incoming = bob.receive().unwrap();
        bob.quarantine(&incoming[0]).unwrap();
        assert!(bob.receive().unwrap().is_empty());
        assert!(Path::new(&incoming[0].reference)
            .with_extension("json.rejected")
            .exists());
    }

    #[test]
    fn mail_config_maps_settings_and_defaults() {
        let mut config = Config::default();
        config.account = "alice@example.org".to_string();
        config.imap_host = "imap.example.org".to_string();
        config.smtp_host = "smtp.example.org".to_string();
        config.imap_port = 993;
        config.smtp_port = 587;
        config.imap_tls = "implicit".to_string();
        config.smtp_tls = "starttls".to_string();
        let mail = mail_config(&config).unwrap();
        assert_eq!(mail.imap_port, 993);
        assert_eq!(mail.smtp_port, 587);
        assert_eq!(mail.imap_tls, TlsMode::Implicit);
        assert_eq!(mail.smtp_tls, TlsMode::StartTls);
        assert_eq!(mail.folder, "INBOX");
    }

    #[test]
    fn mail_config_rejects_unknown_tls_modes() {
        let mut config = Config::default();
        config.imap_tls = "kinda".to_string();
        assert!(mail_config(&config).is_err());
    }

    #[test]
    fn missing_mail_settings_point_at_the_config_key() {
        let config = Config::default();
        let error = MailboxTransport::new(&config).unwrap_err();
        assert_eq!(error.failure, TransportFailure::Permanent);
        assert!(error.message.contains("mail.account"));
    }
}
