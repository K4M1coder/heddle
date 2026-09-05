//! The node vocabulary (spec 002 Key Entities, FR-013).
//!
//! All seven kinds the spec names are present from the first slice, and only
//! three of them execute. That is deliberate: the enum is the serialized shape
//! of a `Workflow`, so growing it variant-by-variant would force every workflow
//! written by an earlier slice to be migrated by a later one. The four that do
//! not execute refuse loudly at
//! [`WorkflowEngine::run`](crate::WorkflowEngine::run) — **before** anything is
//! logged for them — rather than being absent from the type.

use heddle_core::{Message, ToolCall};
use serde::{Deserialize, Serialize};

/// One node of a [`Workflow`](crate::Workflow)'s graph.
///
/// Every variant carries an `id`, and the id is how a resumed run says *which*
/// node it is pending on. Deriving that from graph position instead would work
/// today, when the graph is a `Vec` walked in order, and stop working the moment
/// `Parallel` makes "position" ambiguous.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Node {
    /// Exactly one bounded [`ModelClient::turn`] plus tool mediation for that
    /// turn — not an open-ended ReAct loop. See `plan.md` D4 for why this node
    /// carries no budget: there is no iteration here for one to guard.
    ///
    /// [`ModelClient::turn`]: heddle_core::ModelClient::turn
    Agent { id: String, prompt: Message },
    /// One call through [`ToolGateway::call_captured`], policy and all. The
    /// governed path is wrapped, never re-implemented.
    ///
    /// [`ToolGateway::call_captured`]: heddle_core::ToolGateway::call_captured
    Tool { id: String, call: ToolCall },
    /// A gate that blocks until a **human** decision is recorded out of band by
    /// [`WorkflowEngine::decide`](crate::WorkflowEngine::decide). Distinct from
    /// the gateway's automatic, policy-driven approval of a mutating tool call,
    /// which is a different decision by a different decider — see `plan.md` D2.
    Approval { id: String, message: String },
    /// Deferred: run another workflow by name as one node.
    Subagent { id: String, workflow: String },
    /// Deferred: branch on a predicate.
    Condition { id: String, on: String },
    /// Deferred: fan out over named branches.
    Parallel { id: String, branches: Vec<String> },
    /// Deferred: FR-017's ground-truth loop bodies (ReAct, Reflexion,
    /// Self-Refine, evaluator-optimizer). This is the one node kind that will
    /// need a `LoopController`, which is exactly why it is not implemented
    /// alongside nodes that do not.
    Loop { id: String, body: String },
}

impl Node {
    /// The node's own identity, whatever its kind.
    pub fn id(&self) -> &str {
        match self {
            Node::Agent { id, .. }
            | Node::Tool { id, .. }
            | Node::Approval { id, .. }
            | Node::Subagent { id, .. }
            | Node::Condition { id, .. }
            | Node::Parallel { id, .. }
            | Node::Loop { id, .. } => id,
        }
    }

    /// The kind's name, for the refusal message a deferred variant produces.
    /// Taken from the same `match` that decides the behaviour, so a variant
    /// added later cannot be named here and forgotten there.
    pub(crate) fn kind_name(&self) -> &'static str {
        match self {
            Node::Agent { .. } => "agent",
            Node::Tool { .. } => "tool",
            Node::Approval { .. } => "approval",
            Node::Subagent { .. } => "subagent",
            Node::Condition { .. } => "condition",
            Node::Parallel { .. } => "parallel",
            Node::Loop { .. } => "loop",
        }
    }
}
