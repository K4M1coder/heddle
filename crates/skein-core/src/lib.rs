//! Skein core — headless product library (design §4.2).
//! v0 strict-local slice: typed content, event-sourced Ledger (§4.11) over a
//! pluggable durable store, engine-enforced LoopController (§4.14).

pub mod content;
pub mod error;
pub mod ledger;
pub mod loop_ctl;
pub mod model;
pub mod native_loop;
pub mod secret;
pub mod tool;

pub use content::{Content, Message, Role};
pub use error::{Result, SkeinError};
pub use ledger::{Ledger, LedgerStore, Step, StepKind};
pub use loop_ctl::{Exit, LoopBudget, LoopController};
pub use model::{ModelClient, TextSink, TurnRequest, TurnResponse, WireExchange};
pub use native_loop::{LoopRun, NativeLoop, ProgressProbe};
pub use secret::{SecretProvider, SecretRef, SecretValue};
pub use tool::{
    replay_tool_calls, CapturedResult, Decision, Redactor, ToolAccess, ToolCall, ToolGateway,
    ToolOutcome, ToolPolicy, ToolSpec, ToolTransport,
};
