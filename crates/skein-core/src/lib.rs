//! Skein core — headless product library (design §4.2).
//! v0 strict-local slice: typed content, event-sourced Ledger (§4.11),
//! engine-enforced LoopController (§4.14). Runtime/gateway/silo land next.

pub mod content;
pub mod error;
pub mod ledger;
pub mod loop_ctl;

pub use content::{Content, Message, Role};
pub use error::{Result, SkeinError};
pub use ledger::{Ledger, Step, StepKind};
pub use loop_ctl::{Exit, LoopBudget, LoopController};
