//! Mail transport: IMAP for receiving, SMTP for sending, app passwords for both.
//!
//! The clients are deliberately small, blocking and dependency-light: the wire
//! formats are simple, and a hand-written client keeps the trust boundary in
//! reviewable code. TLS comes from `rustls` with bundled Mozilla roots, so there
//! is no OpenSSL or system certificate store to configure on Windows or Linux.
//!
//! Nothing here decides what a message means. That is the engine's job; this
//! crate moves bytes and classifies failures.

mod error;
mod imap;
mod mime;
mod net;
mod smtp;
mod transport;

pub use error::{FailureKind, MailError, MailResult};
pub use imap::{ImapClient, MailboxInfo};
pub use mime::{build_message, parse_message, ParsedMessage, SUBJECT_PREFIX};
pub use net::TlsMode;
pub use smtp::SmtpClient;
pub use transport::{MailConfig, MailTransport, ReceivedMessage, MAX_MESSAGE_BYTES};
