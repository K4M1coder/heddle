//! The Atlassian connector (spec 039): Jira and Confluence over MCP, gated by
//! the egress boundary (ADR-0002 D4) exactly as the model gateway's cloud
//! routes are (specs/035-model-gateway-routing).
//!
//! [`client`] is the transport half — the proved site address, the wire, and
//! the encoding helpers neither Jira nor Confluence's request shapes can do
//! without. [`server`] is the MCP surface: [`AtlassianConfig`], the egress
//! gate, and the six tools a model is shown.

mod client;
mod server;

pub use server::{AtlassianConfig, AtlassianServer};
