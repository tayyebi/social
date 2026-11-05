//! Failure classification.
//!
//! The engine needs to know *why* something failed, because "retry" and "ask the
//! human to fix the password" are different actions. Everything here maps to one
//! of four kinds.

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// Bad credentials or a refused login: retrying will not help.
    Auth,
    /// Network trouble, a busy server, or a temporary rejection: retry later.
    Transient,
    /// The server refused this message for a reason that will not change.
    Permanent,
    /// The message is bigger than the 2 MB protocol ceiling.
    TooLarge,
    /// Local configuration problem; treated as permanent until fixed.
    Config,
    /// The server said something we could not parse.
    Protocol,
}

/// A transport failure with a machine-readable kind and a human-readable reason.
///
/// Messages never contain credentials: the password is only ever placed in the
/// AUTH command, never formatted into an error.
#[derive(Debug, Clone, Error)]
#[error("{message}")]
pub struct MailError {
    pub kind: FailureKind,
    pub message: String,
}

pub type MailResult<T> = Result<T, MailError>;

impl MailError {
    pub fn new(kind: FailureKind, message: impl Into<String>) -> Self {
        MailError {
            kind,
            message: message.into(),
        }
    }

    pub fn auth(message: impl Into<String>) -> Self {
        MailError::new(FailureKind::Auth, message)
    }

    pub fn transient(message: impl Into<String>) -> Self {
        MailError::new(FailureKind::Transient, message)
    }

    pub fn permanent(message: impl Into<String>) -> Self {
        MailError::new(FailureKind::Permanent, message)
    }

    pub fn config(message: impl Into<String>) -> Self {
        MailError::new(FailureKind::Config, message)
    }

    pub fn protocol(message: impl Into<String>) -> Self {
        MailError::new(FailureKind::Protocol, message)
    }

    pub fn too_large(message: impl Into<String>) -> Self {
        MailError::new(FailureKind::TooLarge, message)
    }
}

impl From<std::io::Error> for MailError {
    fn from(error: std::io::Error) -> Self {
        // Every I/O problem we can see here (timeout, reset, closed socket) is
        // worth retrying; permanent failures arrive as protocol replies.
        MailError::transient(format!("network error: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_errors_are_transient() {
        let error = MailError::from(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "timed out",
        ));
        assert_eq!(error.kind, FailureKind::Transient);
    }

    #[test]
    fn display_never_repeats_secrets() {
        let error = MailError::auth("sign-in failed: 535 5.7.8 credentials rejected");
        assert!(error.to_string().contains("535"));
        assert!(!error.to_string().contains("password"));
    }
}
