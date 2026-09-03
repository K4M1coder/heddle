//! Error type for the Skein core.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkeinError {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ledger integrity broken at seq {seq}: {detail}")]
    LedgerIntegrity { seq: u64, detail: String },
    /// A durable Ledger backing refused, or a silo could not be opened.
    #[error("storage: {0}")]
    Storage(String),
    #[error("not found: {0}")]
    NotFound(String),
    /// A secret reference could not be resolved: unknown scheme, malformed URI,
    /// or no such credential in the backing store.
    #[error("secret: {0}")]
    Secret(String),
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
