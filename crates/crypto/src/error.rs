use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("invalid base64: {0}")]
    Base64(String),
    #[error("invalid key material: {0}")]
    Key(String),
    #[error("signature verification failed")]
    BadSignature,
    #[error("decryption failed")]
    Decrypt,
    #[error("backup format error: {0}")]
    Backup(String),
    #[error("wrong passphrase or corrupted backup")]
    Passphrase,
    #[error("serialization error: {0}")]
    Serde(String),
}

impl From<serde_json::Error> for CryptoError {
    fn from(value: serde_json::Error) -> Self {
        CryptoError::Serde(value.to_string())
    }
}
