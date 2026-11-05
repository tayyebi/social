//! The application engine shared by the CLI and the TUI.

pub mod config;
mod engine;
mod error;
pub mod transport;

pub use config::Config;
pub use engine::{
    data_path, ContactView, ConversationItem, Engine, FeedItem, ImportOutcome, IngestOutcome,
    InitOptions, QueueItem, RedistributeReport, SyncReport, ACCEPTED_LABEL,
};
pub use error::EngineError;
pub use social_mail_crypto::group_fingerprint;
pub use social_mail_protocol::{Audience, ContactCard};
pub use social_mail_storage::{DeliveryState, FollowState, Tab, Trust};
pub use transport::SpoolTransport;
