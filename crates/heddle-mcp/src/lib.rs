//! The rmcp-backed `ToolTransport` (design §4.3, Constitution IV): the only
//! crate in the product that names the MCP protocol **as a client**.
//! `heddle-connectors` is the only one that names it as a **server**, and the two
//! meet over an in-process duplex there. `heddle-core` reaches an MCP server
//! through the port it defines and never depends on either crate.

use heddle_core::{HeddleError, Result, ToolCall, ToolOutcome, ToolSpec, ToolTransport};
use rmcp::model::{CallToolRequestParams, Tool};
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::IntoTransport;
use rmcp::ServiceExt;
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
        let runtime = Runtime::new().map_err(|e| HeddleError::Tool(e.to_string()))?;
        let client = runtime
            .block_on(().serve(transport))
            .map_err(|e| HeddleError::Tool(e.to_string()))?;
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
            .map_err(|e| HeddleError::Tool(e.to_string()))?;

        // The whole result, not only its content: `is_error` and any structured
        // content are part of what the tool actually said.
        Ok(ToolOutcome {
            content: serde_json::to_string(&result)?,
        })
    }

    /// MCP's `tools/list`, paginated to exhaustion by `list_all_tools`.
    ///
    /// Overriding matters more than it looks: [`ToolTransport::list`] is
    /// defaulted to the empty catalogue, so a client that did not override it
    /// would advertise **nothing** against a server offering everything, with
    /// no compile error and no runtime error to say so.
    fn list(&mut self) -> Result<Vec<ToolSpec>> {
        let tools = self
            .runtime
            .block_on(self.client.list_all_tools())
            .map_err(|e| HeddleError::Tool(e.to_string()))?;
        Ok(tools.iter().map(spec_of).collect())
    }
}

/// One MCP tool as the model is told about it. `input_schema` is passed through
/// untouched — it is the server's own document, derived at the far end from the
/// type the tool deserializes against, and re-deriving it here would be the
/// drift `ToolSpec` exists to prevent.
fn spec_of(tool: &Tool) -> ToolSpec {
    ToolSpec::new(
        tool.name.to_string(),
        // Optional on the wire. A server that describes nothing gets an empty
        // description rather than being dropped from the catalogue: whether the
        // operator may use it is the policy's decision, not the schema's.
        tool.description.as_deref().unwrap_or_default(),
        serde_json::Value::Object((*tool.input_schema).clone()),
    )
}
