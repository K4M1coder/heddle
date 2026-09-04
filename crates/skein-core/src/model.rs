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

/// One provider round trip, as bytes rather than as meaning: the literal
/// request body that was transmitted and the literal response body that was
/// parsed. It is what makes the chain able to disagree with itself — a
/// mistranslation on either side of the port is invisible while the only record
/// is [`TurnRequest`] and [`TurnResponse`], which are both this side of it.
///
/// Bodies only. Headers and the request line are deliberately absent, so no
/// provider credential can become a chain payload before the slice that
/// designs for one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireExchange {
    pub url: String,
    /// The one wire fact neither body carries.
    pub status: u16,
    pub request: String,
    pub response: String,
}

/// Synchronous in v0: this slice has no network, and a single-conversation turn
/// loop is inherently sequential. A network-backed client owns its async runtime
/// internally and blocks behind this boundary.
pub trait ModelClient {
    fn turn(&mut self, req: &TurnRequest) -> Result<TurnResponse>;

    /// The bytes of the exchange the last [`ModelClient::turn`] performed, if
    /// this client has a wire at all.
    ///
    /// Defaulted to `None` because for a scripted or in-process client that is
    /// the *true* answer rather than a convenience: there were no bytes.
    ///
    /// Taken, not borrowed: an exchange belongs to exactly one turn, so a
    /// client whose next turn fails before reaching a socket cannot re-offer
    /// the previous one's bytes as if they were this turn's.
    fn take_wire_exchange(&mut self) -> Option<WireExchange> {
        None
    }
}
