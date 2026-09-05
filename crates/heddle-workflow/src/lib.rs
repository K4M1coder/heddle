//! Heddle's native workflow engine (spec 002, design §4.12).
//!
//! A [`Workflow`] is a named, ordered list of [`Node`]s. [`WorkflowEngine::run`]
//! replays the run's Ledger to learn which nodes are already recorded as
//! complete, executes exactly the unlogged remainder in order, and returns as
//! soon as it either finishes or reaches an [`Node::Approval`] with no decision
//! on the chain.
//!
//! **Resume is not a second API.** "The process died after node 2" and "a human
//! has not decided yet" are the same code path: both are answered by scanning
//! the chain and skipping what it already holds. Nothing blocks a thread, so a
//! run interrupted by a kill is recoverable by a *new* process opening the same
//! Ledger — which a parked thread would not be.
//!
//! This crate depends on `heddle-core`, `serde` and `serde_json`, and names no
//! provider, protocol or connector (Constitution IV).

pub mod engine;
pub mod node;

pub use engine::{WorkflowEngine, WorkflowExit, WorkflowRun};
pub use node::Node;

use serde::{Deserialize, Serialize};

/// A workflow definition: spec 002's `{name, params, graph: [Node]}`.
///
/// Where a definition *lives* is not this slice's question — a `Workflow`
/// value, however obtained, is what [`WorkflowEngine::run`] consumes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    /// Author-supplied parameters, carried opaquely. The engine does not
    /// interpret them in this slice.
    #[serde(default)]
    pub params: serde_json::Value,
    pub graph: Vec<Node>,
}

impl Workflow {
    pub fn new(name: impl Into<String>, graph: Vec<Node>) -> Self {
        Workflow {
            name: name.into(),
            params: serde_json::Value::Null,
            graph,
        }
    }
}
