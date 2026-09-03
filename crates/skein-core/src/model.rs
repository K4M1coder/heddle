//! The model provider port (design §4.2, Constitution IV): the core discovers
//! providers through a trait and never names a concrete one.

use crate::content::Message;
use crate::error::Result;
use crate::tool::{ToolCall, ToolSpec};
use serde::{Deserialize, Serialize};

/// One request to a model provider: the conversation so far, and what the model
/// is allowed to ask for. Sampling params still arrive with the HTTP client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnRequest {
    pub run_id: String,
    pub messages: Vec<Message>,
    /// What the model is told it can do — [`ToolGateway::advertise`]'s output,
    /// so already filtered to the run's policy.
    ///
    /// Defaulted **and** skipped when empty, so a run that advertises nothing
    /// serializes no `tools` key at all: the bytes on the wire and the captured
    /// `LlmRequest` payload are identical to the ones produced before this field
    /// existed, in both directions across a revert. Same reasoning as
    /// [`TurnResponse::tool_calls`], from the other end of the conversation.
    ///
    /// [`ToolGateway::advertise`]: crate::tool::ToolGateway::advertise
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolSpec>,
}

/// One provider reply. `final_output` is the model's *claim* that it is done;
/// the `LoopController` adjudicates it (Constitution VIII(a)). `tokens_used` is
/// provider metering, not model self-judgment. `tool_calls` are requests, not
/// permissions: the gateway decides whether any of them runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnResponse {
    pub message: Message,
    pub tokens_used: u64,
    pub final_output: bool,
    /// Defaulted so a response captured before tool wiring existed still
    /// deserializes out of the Ledger.
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
}

/// Synchronous in v0: this slice has no network, and a single-conversation turn
/// loop is inherently sequential. A network-backed client owns its async runtime
/// internally and blocks behind this boundary.
pub trait ModelClient {
    fn turn(&mut self, req: &TurnRequest) -> Result<TurnResponse>;
}
