//! Error type for the Skein core.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkeinError {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ledger integrity broken at seq {seq}: {detail}")]
    LedgerIntegrity { seq: u64, detail: String },
    #[error("not found: {0}")]
    NotFound(String),
    #[error("model provider: {0}")]
    Model(String),
}

pub type Result<T> = std::result::Result<T, SkeinError>;
