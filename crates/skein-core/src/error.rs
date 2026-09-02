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
    /// The governor refused: the transport was never reached.
    #[error("tool denied: {tool}: {reason}")]
    ToolDenied { tool: String, reason: String },
    /// The tool itself failed; it may already have had an effect.
    #[error("tool transport: {0}")]
    Tool(String),
}

pub type Result<T> = std::result::Result<T, SkeinError>;
