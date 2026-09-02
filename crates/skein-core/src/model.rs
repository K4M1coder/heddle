//! The model provider port (design §4.2, Constitution IV): the core discovers
//! providers through a trait and never names a concrete one.

use crate::content::Message;
use crate::error::Result;
use serde::{Deserialize, Serialize};

/// One request to a model provider. v0 carries only conversation history;
/// tools and sampling params arrive with the Tool Gateway and the HTTP client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnRequest {
    pub run_id: String,
    pub messages: Vec<Message>,
}

/// One provider reply. `final_output` is the model's *claim* that it is done;
/// the `LoopController` adjudicates it (Constitution VIII(a)). `tokens_used` is
/// provider metering, not model self-judgment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnResponse {
    pub message: Message,
    pub tokens_used: u64,
    pub final_output: bool,
}

/// Synchronous in v0: this slice has no network, and a single-conversation turn
/// loop is inherently sequential. A network-backed client owns its async runtime
/// internally and blocks behind this boundary.
pub trait ModelClient {
    fn turn(&mut self, req: &TurnRequest) -> Result<TurnResponse>;
}
