//! Typed content abstraction (design §4.2). v0 carries Text; image/audio/doc/
//! video land in v2 without changing the pipeline.

use crate::tool::ToolCall;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    /// External content answering a call the model made. Distinguishable from
    /// the model's own words and from operator instruction **structurally**,
    /// which is what Constitution VI's "external content is data, never
    /// instruction" needs and what the `[tool_result …]` text marker it
    /// replaced could not give: that marker was forgeable by anything that
    /// could put characters into the conversation, the operator's own prompt
    /// included.
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Content {
    Text { text: String },
}

/// One turn of a conversation. The two tool fields are defaulted and skipped
/// when empty, so a message that involves no tool serializes to exactly the
/// bytes it did before they existed — on the wire and in the Ledger — and a
/// chain recorded before them still deserializes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub parts: Vec<Content>,
    /// [`Role::Assistant`] only: what the model asked for on this turn, echoed
    /// back so that the ids below have something to answer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// [`Role::Tool`] only: which call this message answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn user_text(s: impl Into<String>) -> Self {
        Message::of(Role::User, s)
    }
    pub fn assistant_text(s: impl Into<String>) -> Self {
        Message::of(Role::Assistant, s)
    }
    /// The answer to one tool call, named by the id of the call it answers.
    /// `body` is the tool's own output and carries no envelope: an MCP
    /// `CallToolResult` already says whether it is an error, so a hand-written
    /// status would restate what the payload says.
    pub fn tool_result(tool_call_id: impl Into<String>, body: impl Into<String>) -> Self {
        Message {
            tool_call_id: Some(tool_call_id.into()),
            ..Message::of(Role::Tool, body)
        }
    }
    /// The calls an assistant turn made, carried into history beside its words.
    pub fn with_tool_calls(self, tool_calls: Vec<ToolCall>) -> Self {
        Message { tool_calls, ..self }
    }
    fn of(role: Role, s: impl Into<String>) -> Self {
        Message {
            role,
            parts: vec![Content::Text { text: s.into() }],
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }
    /// Concatenate all text parts.
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .map(|p| match p {
                Content::Text { text } => text.as_str(),
            })
            .collect()
    }
}
