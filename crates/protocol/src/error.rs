use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("malformed JSON: {0}")]
    Json(String),
    #[error("serialization error: {0}")]
    Serde(String),
    #[error("unsupported protocol name {0:?}")]
    UnsupportedProtocol(String),
    #[error("unsupported protocol version {0}")]
    UnsupportedVersion(u32),
    #[error("invalid fingerprint {0:?}")]
    InvalidFingerprint(String),
    #[error("invalid nonce")]
    InvalidNonce,
    #[error("invalid timestamp {0:?}")]
    InvalidTimestamp(String),
    #[error("invalid payload: {0}")]
    InvalidPayload(String),
    #[error("event is too large ({0} bytes)")]
    TooLarge(usize),
    #[error("canonical JSON does not support non-integer numbers")]
    NonIntegerNumber,
    #[error("event id does not match the canonical event bytes")]
    EventIdMismatch,
    #[error("event author does not match the signing key")]
    WrongAuthor,
    #[error("signature verification failed")]
    BadSignature,
    #[error("contact card error: {0}")]
    Card(String),
    #[error("limit exceeded: {0}")]
    Limits(&'static str),
}

impl From<serde_json::Error> for ProtocolError {
    fn from(value: serde_json::Error) -> Self {
        ProtocolError::Serde(value.to_string())
    }
}
