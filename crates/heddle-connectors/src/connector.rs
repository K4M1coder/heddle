//! An MCP server hosted in this process, behind `heddle-core`'s synchronous
//! `ToolTransport` port.
//!
//! Client and server meet over a `tokio::io::duplex` — no socket, no child
//! process, no third-party runtime. That is not a convenience: an
//! out-of-process connector would hand back at runtime the local-only
//! guarantee `heddle-gateway` makes a property of the build (Constitution II,
//! NON-NEGOTIABLE).

use crate::atlassian::{AtlassianConfig, AtlassianServer};
use crate::fs::FsRoot;
use crate::m365::{M365Config, M365Server};
use crate::server::{EmbeddedServer, RunAccess};
use heddle_core::{
    HeddleError, Result, SecretProvider, ToolCall, ToolOutcome, ToolSpec, ToolTransport,
};
use heddle_mcp::RmcpToolTransport;
use rmcp::{ServerHandler, ServiceExt};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
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
/// Both shipped call sites were traced against that rule: `heddle chat` builds
/// the connector on the main thread, and `heddle acp-agent` builds it inside
/// `HeddleAgent::open`, which runs under `futures::executor::block_on` rather
/// than a tokio context.
pub struct LocalConnector {
    // Field order is load-bearing, exactly as it is in `RmcpToolTransport`:
    // declared before the runtime so the client is torn down while the runtime
    // driving the server task is still alive.
    transport: RmcpToolTransport,
    // Read by nothing and dropped last: it exists to outlive the client above.
    _runtime: Runtime,
}

/// Written by hand: neither field derives `Debug`, and a caller only ever
/// needs this to name what failed to build, not to inspect a live connector —
/// `expect_err` in the Atlassian connector's egress-refusal tests is the only
/// caller.
impl std::fmt::Debug for LocalConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalConnector").finish_non_exhaustive()
    }
}

/// One [`EmbeddedServer`] over `root`, connected in-process.
///
/// The server gets its **own** runtime rather than sharing the client's,
/// because the client's is inside `RmcpToolTransport` and is occupied blocking
/// on each call; a server task on it could not answer.
pub fn local_connector(root: FsRoot) -> Result<LocalConnector> {
    serve(EmbeddedServer::new(root))
}

/// [`local_connector`] plus the process launcher, and fallible for the reason
/// [`EmbeddedServer::with_run`] documents: a sandbox that cannot be built must
/// be an exit code before a model is shown a tool.
///
/// `cancelled` is passed straight through. It is the caller's — in the product,
/// one ACP session's — and this crate neither resets it nor sets it.
pub fn local_connector_with_run(
    root: FsRoot,
    run: RunAccess,
    cancelled: Arc<AtomicBool>,
) -> Result<LocalConnector> {
    serve(EmbeddedServer::with_run(root, run, cancelled)?)
}

/// One [`AtlassianServer`] over `config`, connected in-process, exactly as
/// [`local_connector`] connects one [`EmbeddedServer`] over a root.
///
/// Fallible where `local_connector` is not: [`AtlassianServer::connect`] is
/// the egress gate (ADR-0002 D4), and a network connector this crate is asked
/// to build without permission to leave the machine is refused here, before
/// any socket exists — matching `EmbeddedServer::with_run`'s reasoning for a
/// sandbox that cannot be built.
pub fn atlassian_connector(
    config: AtlassianConfig,
    secrets: &dyn SecretProvider,
    egress_allowed: bool,
) -> Result<LocalConnector> {
    serve(AtlassianServer::connect(config, secrets, egress_allowed)?)
}

/// One [`M365Server`] over `config`, connected in-process, exactly as
/// [`atlassian_connector`] connects one [`AtlassianServer`] over a site.
///
/// Fallible for the same reason and at the same point:
/// [`M365Server::connect`] is the egress gate (ADR-0002 D4), and a connector
/// this crate is asked to build without permission to reach Microsoft Graph is
/// refused there, before any socket exists.
pub fn m365_connector(
    config: M365Config,
    secrets: &dyn SecretProvider,
    egress_allowed: bool,
) -> Result<LocalConnector> {
    serve(M365Server::connect(config, secrets, egress_allowed)?)
}

/// Generic over the handler, and that is the whole of what the Atlassian
/// connector needed from this file: [`EmbeddedServer`] and
/// [`crate::AtlassianServer`] are both `ServerHandler`s over the same duplex
/// shape, so a second copy of these fifteen lines would be a second place the
/// runtime-ownership rule above could go wrong.
pub(crate) fn serve<S: ServerHandler>(server: S) -> Result<LocalConnector> {
    let runtime = Runtime::new().map_err(|e| HeddleError::Tool(e.to_string()))?;
    let (server_side, client_side) = runtime.block_on(async { tokio::io::duplex(DUPLEX_BUFFER) });

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
/// policy: the governor is `heddle_core::ToolGateway`, above this.
impl ToolTransport for LocalConnector {
    fn call(&mut self, call: &ToolCall) -> Result<ToolOutcome> {
        self.transport.call(call)
    }

    fn list(&mut self) -> Result<Vec<ToolSpec>> {
        self.transport.list()
    }
}
