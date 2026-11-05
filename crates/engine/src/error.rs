use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("storage error: {0}")]
    Storage(#[from] social_mail_storage::StorageError),
    #[error("protocol error: {0}")]
    Protocol(#[from] social_mail_protocol::ProtocolError),
    #[error("crypto error: {0}")]
    Crypto(#[from] social_mail_crypto::CryptoError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("this profile is not initialised: {0}")]
    NotInitialised(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("{0}")]
    Invalid(String),
}
