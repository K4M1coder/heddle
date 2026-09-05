//! Error type for the Heddle core.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HeddleError {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ledger integrity broken at seq {seq}: {detail}")]
    LedgerIntegrity { seq: u64, detail: String },
    /// A durable Ledger backing refused, or a silo could not be opened.
    #[error("storage: {0}")]
    Storage(String),
    #[error("not found: {0}")]
    NotFound(String),
    /// A config setting was addressed at a scope that does not exist in this
    /// mode — a Team-level value in Local mode, which has no Team level
    /// (design §5.5).
    #[error("config: {0}")]
    Config(String),
    /// A scope tried to override a value an ancestor scope locked (spec 002's
    /// Edge Cases: "a lower level attempts to override a setting locked higher
    /// up → explicit refusal"). Separate from [`HeddleError::Config`] because
    /// the caller's remedy is different: this one is answered by the owner of
    /// `locked_at`, not by rewriting the request.
    #[error("config scope {scope} may not override the value locked at {locked_at}")]
    ConfigLocked { scope: String, locked_at: String },
    /// A secret reference could not be resolved: unknown scheme, malformed URI,
    /// or no such credential in the backing store.
    #[error("secret: {0}")]
    Secret(String),
    #[error("model provider: {0}")]
    Model(String),
    /// A run ended on a budget rather than with an answer. Not a provider
    /// failure: the engine stopped the model, which is Constitution VIII
    /// working. A client needs its own name for this, because
    /// [`HeddleError::Model`] would blame a provider that made no decision.
    #[error("run {run_id} ended without a final answer: {exit}")]
    Unfinished { run_id: String, exit: String },
    /// The governor refused: the transport was never reached.
    #[error("tool denied: {tool}: {reason}")]
    ToolDenied { tool: String, reason: String },
    /// The tool itself failed; it may already have had an effect.
    #[error("tool transport: {0}")]
    Tool(String),
    /// A protocol adapter's transport failed: the ACP/MCP connection itself,
    /// not the model behind it and not the tool behind it.
    #[error("protocol: {0}")]
    Protocol(String),
    /// A capability the type system names but this build does not implement —
    /// a workflow node kind reserved by spec 002's vocabulary whose executor is
    /// a later slice. Distinct from [`HeddleError::NotFound`], which is about a
    /// thing that could have existed: this is about a thing that will, and the
    /// caller's retry after a future release is the sensible response.
    #[error("unsupported: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, HeddleError>;
