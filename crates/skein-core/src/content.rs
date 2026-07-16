//! Typed content abstraction (design §4.2). v0 carries Text; image/audio/doc/
//! video land in v2 without changing the pipeline.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Content {
    Text { text: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub parts: Vec<Content>,
}

impl Message {
    pub fn user_text(s: impl Into<String>) -> Self {
        Message {
            role: Role::User,
            parts: vec![Content::Text { text: s.into() }],
        }
    }
    pub fn assistant_text(s: impl Into<String>) -> Self {
        Message {
            role: Role::Assistant,
            parts: vec![Content::Text { text: s.into() }],
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
