//! The embedded `fs` MCP server: three tools over one [`FsRoot`].
//!
//! This is the only place in the product that names MCP as a **server**.
//! `skein-mcp` names it as a client, and the two meet over an in-process duplex
//! in [`crate::fs_connector`] — no socket, no child process (Constitution II).

use crate::fs::FsRoot;
use crate::git;
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

/// The most commits a single `git_log` may walk.
///
/// [`READ_BYTE_CAP`]'s reasoning, in commits: a log is prompt and it is a
/// Ledger row. This one **refuses** rather than truncates, and that is not an
/// inconsistency with `git_status` below — `git_log` takes a `count`, so a
/// refusal naming the cap leaves the model a smaller call it can make.
pub const LOG_COUNT_CAP: u32 = 50;

/// The most entries a single `git_status` may list.
///
/// This one **truncates and says how many it dropped**, where every other cap
/// in this file refuses. `git_status` takes no arguments, so there is no
/// smaller call to fall back to and a refusal would leave a dirty repository
/// permanently unreadable. A labelled truncation is a smaller answer; a silent
/// one would be a wrong answer in a right answer's shape.
pub const STATUS_ENTRY_CAP: usize = 200;

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

/// `git_log`'s arguments, and the **only** model-supplied value anywhere in
/// the git tools. Public for the reason [`ReadParams`] documents.
///
/// A `u32` and not a string, which is the whole injection story: there is no
/// subprocess, no argument vector and no shell in the git path, and the one
/// value a model does supply cannot carry text. A crafted `count` fails
/// deserialization, which rmcp reports as `isError: true` — a refusal the
/// model is told about and the run survives.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct LogParams {
    /// How many of the most recent commits to return, newest first.
    pub count: u32,
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

    #[tool(
        description = "Report the state of the git repository at the configured root, in `git \
                       status --porcelain -b` form: a `## <branch>` header, then one `XY<TAB>path` \
                       line per change. Takes no arguments — the repository is the one the \
                       operator named and no other. Rename detection is off, so a rename appears \
                       as a delete plus an add."
    )]
    pub fn git_status(&self) -> Result<String, String> {
        git::status(&self.root)
    }

    #[tool(
        description = "Return the most recent commits of the git repository at the configured \
                       root, newest first, one `<short oid><TAB><UTC date><TAB><author \
                       name><TAB><summary>` line each. `count` names how many and must be between \
                       1 and 50. Only each commit's summary line is returned, never its body."
    )]
    pub fn git_log(&self, params: Parameters<LogParams>) -> Result<String, String> {
        let count = params.0.count;
        if count == 0 {
            return Err("name at least one commit to return".to_string());
        }
        if count > LOG_COUNT_CAP {
            return Err(format!(
                "{count} commits is over the {LOG_COUNT_CAP}-commit log cap; ask for fewer"
            ));
        }
        git::log(&self.root, count)
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
