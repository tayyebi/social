use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("data error: {0}")]
    Data(String),
    #[error("not found: {0}")]
    NotFound(String),
}
