//! The Microsoft 365 connector (spec 040): Outlook, SharePoint and Teams over
//! Microsoft Graph, gated by the egress boundary (ADR-0002 D4) exactly as the
//! Atlassian connector is (specs/039) and as the model gateway's cloud routes
//! are (specs/035-model-gateway-routing).
//!
//! [`client`] is the transport half — the proved Graph address, the wire, and
//! the encoding helpers Graph's ids and paths cannot do without. [`server`] is
//! the MCP surface: [`M365Config`], the egress gate, and the five tools a model
//! is shown.

mod client;
mod server;

pub use server::{M365Config, M365Server};
