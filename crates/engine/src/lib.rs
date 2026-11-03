//! The application engine shared by the CLI and the TUI.

pub mod config;
mod engine;
mod error;
pub mod transport;

pub use config::Config;
pub use engine::{
    data_path, ContactView, ConversationItem, Engine, FeedItem, ImportOutcome, IngestOutcome,
    InitOptions, QueueItem, SyncReport, ACCEPTED_LABEL,
};
pub use error::EngineError;
pub use transport::SpoolTransport;
