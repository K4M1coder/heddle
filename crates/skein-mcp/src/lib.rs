//! The rmcp-backed `ToolTransport` (design §4.3, Constitution IV): the only
//! crate in the product that names the MCP protocol. `skein-core` reaches an MCP
//! server through the port it defines and never depends on this crate.

use rmcp::model::CallToolRequestParams;
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::IntoTransport;
use rmcp::ServiceExt;
use skein_core::{Result, SkeinError, ToolCall, ToolOutcome, ToolTransport};
use tokio::runtime::Runtime;

/// An MCP client behind the synchronous [`ToolTransport`] port. It owns its
/// runtime and blocks on it, exactly as `ModelClient` expects a network-backed
/// provider to.
///
/// Because of that, **no method of this type may be called from inside an async
/// context**: `Runtime::block_on` panics when a runtime is already entered.
pub struct RmcpToolTransport {
    // Declared before the runtime so the service is torn down while the runtime
    // that drives it is still alive.
    client: RunningService<RoleClient, ()>,
    runtime: Runtime,
}

impl RmcpToolTransport {
    /// Performs the MCP initialize handshake over `transport` — any byte stream,
    /// which is what keeps a real server out of this crate's tests.
    pub fn connect<T, E, A>(transport: T) -> Result<Self>
    where
        T: IntoTransport<RoleClient, E, A>,
        E: std::error::Error + Send + Sync + 'static,
    {
        let runtime = Runtime::new().map_err(|e| SkeinError::Tool(e.to_string()))?;
        let client = runtime
            .block_on(().serve(transport))
            .map_err(|e| SkeinError::Tool(e.to_string()))?;
        Ok(RmcpToolTransport { client, runtime })
    }
}

impl ToolTransport for RmcpToolTransport {
    fn call(&mut self, call: &ToolCall) -> Result<ToolOutcome> {
        let mut params = CallToolRequestParams::new(call.tool.clone());
        if let Some(args) = call.args.as_object().filter(|a| !a.is_empty()) {
            params = params.with_arguments(args.clone());
        }

        let result = self
            .runtime
            .block_on(self.client.call_tool(params))
            .map_err(|e| SkeinError::Tool(e.to_string()))?;

        // The whole result, not only its content: `is_error` and any structured
        // content are part of what the tool actually said.
        Ok(ToolOutcome {
            content: serde_json::to_string(&result)?,
        })
    }
}
