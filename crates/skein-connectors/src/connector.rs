//! An MCP server hosted in this process, behind `skein-core`'s synchronous
//! `ToolTransport` port.
//!
//! Client and server meet over a `tokio::io::duplex` — no socket, no child
//! process, no third-party runtime. That is not a convenience: an
//! out-of-process connector would hand back at runtime the local-only
//! guarantee `skein-gateway` makes a property of the build (Constitution II,
//! NON-NEGOTIABLE).

use crate::fs::FsRoot;
use crate::server::EmbeddedServer;
use rmcp::ServiceExt;
use skein_core::{Result, SkeinError, ToolCall, ToolOutcome, ToolSpec, ToolTransport};
use skein_mcp::RmcpToolTransport;
use tokio::runtime::Runtime;

/// In-flight bytes between the two halves: a whole capped `fs_read` plus room
/// for the JSON-RPC envelope around it. A narrower pipe still works — both
/// sides are polled concurrently — but it turns every large read into a
/// handful of round trips.
const DUPLEX_BUFFER: usize = crate::server::READ_BYTE_CAP + 8 * 1024;

/// An MCP server running in this process, and the client that reaches it.
///
/// Like [`RmcpToolTransport`], which it wraps, this owns a runtime and blocks
/// on it — so **no method of this type may be called from inside an async
/// context**: `Runtime::block_on` panics when a runtime is already entered.
/// Both shipped call sites were traced against that rule: `skein chat` builds
/// the connector on the main thread, and `skein acp-agent` builds it inside
/// `SkeinAgent::open`, which runs under `futures::executor::block_on` rather
/// than a tokio context.
pub struct LocalConnector {
    // Field order is load-bearing, exactly as it is in `RmcpToolTransport`:
    // declared before the runtime so the client is torn down while the runtime
    // driving the server task is still alive.
    transport: RmcpToolTransport,
    // Read by nothing and dropped last: it exists to outlive the client above.
    _runtime: Runtime,
}

/// One [`EmbeddedServer`] over `root`, connected in-process.
///
/// The server gets its **own** runtime rather than sharing the client's,
/// because the client's is inside `RmcpToolTransport` and is occupied blocking
/// on each call; a server task on it could not answer.
pub fn local_connector(root: FsRoot) -> Result<LocalConnector> {
    let runtime = Runtime::new().map_err(|e| SkeinError::Tool(e.to_string()))?;
    let (server_side, client_side) = runtime.block_on(async { tokio::io::duplex(DUPLEX_BUFFER) });

    let server = EmbeddedServer::new(root);
    runtime.spawn(async move {
        if let Ok(running) = server.serve(server_side).await {
            let _ = running.waiting().await;
        }
    });

    Ok(LocalConnector {
        transport: RmcpToolTransport::connect(client_side)?,
        _runtime: runtime,
    })
}

/// Pure delegation. The connector's job is lifetime — owning the runtime the
/// server runs on for exactly as long as the client that talks to it — and not
/// policy: the governor is `skein_core::ToolGateway`, above this.
impl ToolTransport for LocalConnector {
    fn call(&mut self, call: &ToolCall) -> Result<ToolOutcome> {
        self.transport.call(call)
    }

    fn list(&mut self) -> Result<Vec<ToolSpec>> {
        self.transport.list()
    }
}
