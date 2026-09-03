//! The embedded `fs` MCP server: three tools over one [`FsRoot`].
//!
//! This is the only place in the product that names MCP as a **server**.
//! `skein-mcp` names it as a client, and the two meet over an in-process duplex
//! in [`crate::fs_connector`] — no socket, no child process (Constitution II).

use crate::fs::FsRoot;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;

/// The most a single `fs_read` may return.
///
/// A tool result does not stop at the caller: `NativeLoop::mediate` feeds it
/// back into the next turn's messages and the gateway records it as a
/// `ToolResult` step, so an unbounded read is an unbounded prompt *and* an
/// unbounded Ledger row. The cap refuses rather than truncates — a silently
/// shortened file is a wrong answer in a right answer's shape, and it would
/// land on the chain as one.
pub const READ_BYTE_CAP: usize = 64 * 1024;

/// `fs_read`'s arguments. Public because the schema `schemars` derives from
/// this type **is** the contract the model is shown: [`crate::LocalConnector`]
/// discovers it over `tools/list` and `skein-gateway` puts it on the wire. A
/// hand-written copy next to the allowlist would be a second source of truth
/// for a document the server validates against.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadParams {
    /// Path relative to the configured root.
    pub path: String,
}

/// `fs_list`'s arguments. Public for the reason [`ReadParams`] documents.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListParams {
    /// Directory relative to the configured root; `.` is the root itself.
    pub path: String,
}

/// `fs_write`'s arguments. Public for the reason [`ReadParams`] documents.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteParams {
    /// Path relative to the configured root.
    pub path: String,
    /// The file's entire new contents.
    pub content: String,
}

/// The tool holder. `Clone` because rmcp's router hands each request a clone of
/// the handler; the root is behind an [`Arc`] so every clone enforces the same
/// containment rule rather than a copy of it.
#[derive(Clone)]
pub struct FsServer {
    root: Arc<FsRoot>,
    tool_router: ToolRouter<Self>,
}

/// Every tool returns `Result<String, String>`, and that signature is the
/// governance decision it looks like.
///
/// rmcp's `impl<T: IntoCallToolResult, E: IntoCallToolResult> IntoCallToolResult
/// for Result<T, E>` sets `is_error = true` on the `Err` arm, so a containment
/// refusal reaches the model as a **tool-level** error it is told about and can
/// act on. The alternative — failing the transport — would end the run, which
/// would make every governed conversation one wrong path away from over.
#[tool_router]
impl FsServer {
    pub fn new(root: FsRoot) -> Self {
        FsServer {
            root: Arc::new(root),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Read a UTF-8 text file inside the configured root. `path` is relative to \
                       that root; an absolute path or one that escapes the root is refused."
    )]
    pub fn fs_read(&self, params: Parameters<ReadParams>) -> Result<String, String> {
        let arg = params.0.path;
        let path = self.root.resolve(&arg)?;
        let size = std::fs::metadata(&path)
            .map_err(|e| format!("{arg}: {e}"))?
            .len();
        if size > READ_BYTE_CAP as u64 {
            return Err(format!(
                "{arg} is {size} bytes, over the {READ_BYTE_CAP}-byte read cap; read a smaller \
                 file"
            ));
        }
        std::fs::read_to_string(&path).map_err(|e| format!("{arg}: {e}"))
    }

    #[tool(
        description = "List one directory inside the configured root, one entry per line as \
                       `dir<TAB>name` or `file<TAB>name`. Not recursive: list a subdirectory to \
                       see inside it. `path` is relative to the root and `.` is the root itself."
    )]
    pub fn fs_list(&self, params: Parameters<ListParams>) -> Result<String, String> {
        let arg = params.0.path;
        let dir = self.root.resolve(&arg)?;
        let mut lines = Vec::new();
        for entry in std::fs::read_dir(&dir).map_err(|e| format!("{arg}: {e}"))? {
            let entry = entry.map_err(|e| format!("{arg}: {e}"))?;
            let kind = match entry.file_type() {
                Ok(t) if t.is_dir() => "dir",
                _ => "file",
            };
            lines.push(format!("{kind}\t{}", entry.file_name().to_string_lossy()));
        }
        // Sorted, so the same directory reads the same way twice: a model given
        // a different order on every call cannot tell a change from a shuffle.
        lines.sort();
        Ok(lines.join("\n"))
    }

    #[tool(
        description = "Replace a file's entire contents inside the configured root. `path` is \
                       relative to that root and its parent directory must already exist; no \
                       directory is created."
    )]
    pub fn fs_write(&self, params: Parameters<WriteParams>) -> Result<String, String> {
        let WriteParams { path: arg, content } = params.0;
        let path = self.root.resolve_new(&arg)?;
        std::fs::write(&path, &content).map_err(|e| format!("{arg}: {e}"))?;
        Ok(format!("wrote {} bytes to {arg}", content.len()))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for FsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Skein's embedded filesystem connector. Every path is relative to one operator-named \
             root and nothing outside it is reachable.",
        )
    }
}
