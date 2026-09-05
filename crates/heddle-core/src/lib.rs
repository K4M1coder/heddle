//! Heddle core — headless product library (design §4.2).
//! v0 strict-local slice: typed content, event-sourced Ledger (§4.11) over a
//! pluggable durable store, engine-enforced LoopController (§4.14), a pluggable
//! TaskTracker (§4.13) and the config hierarchy that resolves it (§5.5).

pub mod content;
pub mod error;
pub mod hierarchy;
pub mod ledger;
pub mod loop_ctl;
pub mod model;
pub mod native_loop;
pub mod secret;
pub mod task;
pub mod tool;

pub use content::{Content, Message, Role};
pub use error::{HeddleError, Result};
pub use hierarchy::{Hierarchy, Lock, Mode, Scope, Setting};
pub use ledger::{Ledger, LedgerStore, Step, StepKind};
pub use loop_ctl::{Exit, LoopBudget, LoopController};
pub use model::{ModelClient, TextSink, TurnRequest, TurnResponse, WireExchange};
pub use native_loop::{LoopRun, NativeLoop, ProgressProbe};
pub use secret::{SecretProvider, SecretRef, SecretValue};
pub use task::{NewTask, NoTracker, Task, TaskId, TaskQuery, TaskStatus, TaskTracker};
pub use tool::{
    replay_tool_calls, ApprovalRecord, ApprovalVerdict, CapturedResult, Decision, Redactor,
    ToolAccess, ToolCall, ToolGateway, ToolOutcome, ToolPolicy, ToolSpec, ToolTransport,
};
