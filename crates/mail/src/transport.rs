//! The mailbox as a transport: send one carrier, receive the ones waiting.
//!
//! One connection per operation. Queue depth in v0.2 is small, and reopening a
//! session is cheaper to reason about than keeping a pool alive across retries.

use crate::error::{FailureKind, MailError, MailResult};
use crate::imap::ImapClient;
use crate::mime::{self, SUBJECT_PREFIX};
use crate::net::TlsMode;
use crate::smtp::SmtpClient;

/// The 2 MB protocol ceiling from PLAN.md §9.
pub const MAX_MESSAGE_BYTES: usize = 2 * 1024 * 1024;

/// Everything needed to reach one mailbox.
#[derive(Debug, Clone)]
pub struct MailConfig {
    pub account: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_tls: TlsMode,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_tls: TlsMode,
    pub username: String,
    /// Name of the environment variable holding the app password.
    pub password_env: String,
    pub folder: String,
    pub protocol_folder: String,
    pub move_after_reading: bool,
}

impl MailConfig {
    pub fn client_name(&self) -> String {
        match self.account.rsplit_once('@') {
            Some((_, domain)) if !domain.is_empty() => format!("social-mail.{domain}"),
            _ => "social-mail.local".to_string(),
        }
    }

    pub fn login(&self) -> String {
        if self.username.trim().is_empty() {
            self.account.trim().to_string()
        } else {
            self.username.trim().to_string()
        }
    }

    /// Reject obviously incomplete settings before touching the network.
    pub fn validate(&self) -> MailResult<()> {
        if self.account.trim().is_empty() || !self.account.contains('@') {
            return Err(MailError::config(
                "set `mail.account` to the mailbox address (for example alice@example.org)",
            ));
        }
        if self.imap_host.trim().is_empty() {
            return Err(MailError::config(
                "set `mail.imap_host` (for example imap.example.org)",
            ));
        }
        if self.smtp_host.trim().is_empty() {
            return Err(MailError::config(
                "set `mail.smtp_host` (for example smtp.example.org)",
            ));
        }
        if self.password_env.trim().is_empty() {
            return Err(MailError::config(
                "set `mail.password_env` to a variable name",
            ));
        }
        Ok(())
    }
}

/// One message pulled from the mailbox, already decoded.
#[derive(Debug, Clone)]
pub struct ReceivedMessage {
    /// Stable handle for acknowledge/quarantine: `folder#uid`.
    pub reference: String,
    pub uid: u32,
    pub body: String,
}

pub struct MailTransport {
    config: MailConfig,
    password: String,
}

/// Debug output never contains the password.
impl std::fmt::Debug for MailTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MailTransport")
            .field("config", &self.config)
            .field("password", &"<redacted>")
            .finish()
    }
}

impl MailTransport {
    /// Read the app password from the environment. It is never stored.
    pub fn from_env(config: MailConfig) -> MailResult<Self> {
        config.validate()?;
        let password = std::env::var(&config.password_env)
            .ok()
            .filter(|value| !value.is_empty())
            .or_else(|| {
                std::env::var("SOCIAL_MAIL_PASSWORD")
                    .ok()
                    .filter(|value| !value.is_empty())
            })
            .ok_or_else(|| {
                MailError::auth(format!(
                    "no app password: set {} (or SOCIAL_MAIL_PASSWORD)",
                    config.password_env
                ))
            })?;
        Ok(MailTransport { config, password })
    }

    pub fn with_password(config: MailConfig, password: impl Into<String>) -> MailResult<Self> {
        config.validate()?;
        Ok(MailTransport {
            config,
            password: password.into(),
        })
    }

    pub fn config(&self) -> &MailConfig {
        &self.config
    }

    fn imap(&self) -> MailResult<ImapClient> {
        let mut client = ImapClient::connect(
            &self.config.imap_host,
            self.config.imap_port,
            self.config.imap_tls,
        )?;
        client.login(&self.config.login(), &self.password)?;
        Ok(client)
    }

    fn smtp(&self) -> MailResult<SmtpClient> {
        let mut client = SmtpClient::connect(
            &self.config.smtp_host,
            self.config.smtp_port,
            self.config.smtp_tls,
            &self.config.client_name(),
        )?;
        client.login(&self.config.login(), &self.password)?;
        Ok(client)
    }

    /// Send one carrier to one address.
    pub fn send(&self, to: &str, carrier: &str) -> MailResult<String> {
        if to.trim().is_empty() {
            return Err(MailError::permanent("this contact has no delivery address"));
        }
        if !to.contains('@') {
            return Err(MailError::permanent(format!(
                "{to:?} is not a delivery address"
            )));
        }
        if carrier.len() > MAX_MESSAGE_BYTES {
            return Err(MailError::too_large(format!(
                "the message is {} bytes; the limit is {MAX_MESSAGE_BYTES}",
                carrier.len()
            )));
        }
        let subject = format!("{SUBJECT_PREFIX}v1 carrier");
        let message = mime::build_message(&self.config.account, to, &subject, carrier.as_bytes());
        let mut client = self.smtp()?;
        client.send(&self.config.account, to, &message)?;
        client.quit();
        Ok(format!(
            "smtp://{}:{}",
            self.config.smtp_host, self.config.smtp_port
        ))
    }

    /// Fetch up to `limit` carriers, oldest first. Ordinary mail is skipped.
    pub fn fetch(&self, limit: usize) -> MailResult<Vec<ReceivedMessage>> {
        let mut client = self.imap()?;
        client.select(&self.config.folder)?;
        let uids = client.search_all()?;
        let mut out = Vec::new();
        for uid in uids.into_iter().rev().take(limit).rev() {
            let raw = client.fetch(uid)?;
            let parsed = match mime::parse_message(&raw) {
                Ok(parsed) => parsed,
                Err(error) => {
                    // Not our mail, or not something we can read: leave it alone.
                    if error.kind == FailureKind::Protocol {
                        continue;
                    }
                    return Err(error);
                }
            };
            if !parsed.is_social_mail() {
                continue;
            }
            let Ok(body) = String::from_utf8(parsed.body) else {
                continue;
            };
            out.push(ReceivedMessage {
                reference: reference(&self.config.folder, uid),
                uid,
                body,
            });
        }
        client.logout();
        Ok(out)
    }

    /// Mark a processed message as read and move it to the protocol folder.
    pub fn acknowledge(&self, reference_value: &str) -> MailResult<()> {
        let (folder, uid) = split_reference(reference_value)?;
        let mut client = self.imap()?;
        client.select(folder)?;
        client.mark_seen(uid)?;
        if self.config.move_after_reading {
            client.ensure_folder(&self.config.protocol_folder)?;
            client.copy(uid, &self.config.protocol_folder)?;
            client.mark_deleted(uid)?;
            client.expunge()?;
        }
        client.logout();
        Ok(())
    }

    /// Leave a rejected message in place, flagged read, with a reason in the
    /// engine's Problems list. We never delete mail we did not understand.
    pub fn quarantine(&self, reference_value: &str) -> MailResult<()> {
        let (folder, uid) = split_reference(reference_value)?;
        let mut client = self.imap()?;
        client.select(folder)?;
        client.mark_seen(uid)?;
        client.logout();
        Ok(())
    }

    /// Check sign-in on both protocols without sending anything.
    pub fn probe(&self) -> MailResult<String> {
        let mut client = self.imap()?;
        let info = client.select(&self.config.folder)?;
        let uids = client.search_all()?;
        client.logout();

        let mut smtp = self.smtp()?;
        smtp.quit();

        Ok(format!(
            "signed in to {} ({} in {}, {} waiting) and to {} ({})",
            self.config.imap_host,
            info.exists,
            self.config.folder,
            uids.len(),
            self.config.smtp_host,
            self.config.smtp_tls.as_str()
        ))
    }

    /// Send a carrier-shaped test message to ourselves.
    pub fn send_test(&self, carrier: &str) -> MailResult<String> {
        self.send(&self.config.account.clone(), carrier)
    }
}

pub fn reference(folder: &str, uid: u32) -> String {
    format!("{folder}#{uid}")
}

fn split_reference(value: &str) -> MailResult<(&str, u32)> {
    let (folder, uid) = value
        .rsplit_once('#')
        .ok_or_else(|| MailError::protocol(format!("bad message reference {value:?}")))?;
    let uid: u32 = uid
        .parse()
        .map_err(|_| MailError::protocol(format!("bad message reference {value:?}")))?;
    Ok((folder, uid))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> MailConfig {
        MailConfig {
            account: "alice@example.org".to_string(),
            imap_host: "imap.example.org".to_string(),
            imap_port: 993,
            imap_tls: TlsMode::Implicit,
            smtp_host: "smtp.example.org".to_string(),
            smtp_port: 587,
            smtp_tls: TlsMode::StartTls,
            username: String::new(),
            password_env: "SOCIAL_MAIL_PASSWORD".to_string(),
            folder: "INBOX".to_string(),
            protocol_folder: "SocialMail".to_string(),
            move_after_reading: true,
        }
    }

    #[test]
    fn validates_settings_before_connecting() {
        let mut config = base_config();
        assert!(config.validate().is_ok());
        config.imap_host.clear();
        let error = config.validate().unwrap_err();
        assert_eq!(error.kind, FailureKind::Config);
        assert!(error.to_string().contains("imap_host"));
    }

    #[test]
    fn login_falls_back_to_the_account() {
        let mut config = base_config();
        assert_eq!(config.login(), "alice@example.org");
        config.username = "  alice ".to_string();
        assert_eq!(config.login(), "alice");
    }

    #[test]
    fn references_round_trip() {
        let value = reference("INBOX", 42);
        assert_eq!(value, "INBOX#42");
        let (folder, uid) = split_reference(&value).unwrap();
        assert_eq!(folder, "INBOX");
        assert_eq!(uid, 42);
        assert!(split_reference("nonsense").is_err());
    }

    #[test]
    fn refuses_an_oversized_carrier() {
        let transport = MailTransport::with_password(base_config(), "secret").unwrap();
        let oversized = "x".repeat(MAX_MESSAGE_BYTES + 1);
        let error = transport.send("bob@example.org", &oversized).unwrap_err();
        assert_eq!(error.kind, FailureKind::TooLarge);
    }

    #[test]
    fn refuses_a_recipient_without_an_address() {
        let transport = MailTransport::with_password(base_config(), "secret").unwrap();
        let error = transport.send("bob", "{}").unwrap_err();
        assert_eq!(error.kind, FailureKind::Permanent);
    }

    #[test]
    fn missing_password_is_an_auth_error() {
        let mut config = base_config();
        config.password_env = "SOCIAL_MAIL_TEST_ABSENT_VARIABLE".to_string();
        // Make sure the fallback cannot rescue the test.
        std::env::remove_var("SOCIAL_MAIL_PASSWORD");
        let error = MailTransport::from_env(config).unwrap_err();
        assert_eq!(error.kind, FailureKind::Auth);
    }
}
