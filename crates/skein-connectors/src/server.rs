//! The embedded MCP server: two families of tool over one [`FsRoot`] — the
//! filesystem tools always, and the git tools when that root is a git
//! repository.
//!
//! This is the only place in the product that names MCP as a **server**.
//! `skein-mcp` names it as a client, and the two meet over an in-process duplex
//! in [`crate::local_connector`] — no socket, no child process (Constitution
//! II).
//!
//! rmcp serves exactly one `ServerHandler` per connection, which is why a
//! second family is more `#[tool]` methods here rather than a second connector
//! behind a fan-out transport: that would double the tokio runtimes per ACP
//! session to add a multiplexer with one caller.

use crate::fs::{FsRoot, RunDirs};
use crate::git;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;
use skein_core::SkeinError;
use skein_sandbox::Sandbox;
use std::sync::Arc;
use std::time::Duration;

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

/// The most a single `proc_run` may return **per stream**.
///
/// This one **truncates and labels the drop**, following [`STATUS_ENTRY_CAP`]'s
/// reasoning rather than [`READ_BYTE_CAP`]'s, and the difference is decidable:
/// the process has already run and cannot be un-run, and there is no smaller
/// call to suggest — a model cannot ask for fewer bytes of output. A refusal
/// would throw away a side effect a human approved.
///
/// Half of `READ_BYTE_CAP`, because a run carries **two** streams into the same
/// prompt and the same Ledger row, so 16 KiB × 2 is the same worst case as one
/// 32 KiB read.
pub const RUN_OUTPUT_BYTE_CAP: usize = 16 * 1024;

/// The wall clock one `proc_run` gets.
///
/// Justified against `ModelArgs::timeout_secs`, which defaults to 120 s for a
/// **whole turn**: a single tool that can eat the entire turn budget makes
/// `LoopBudget` meaningless. Thirty seconds covers a linter or a focused test
/// run; the tool's description states the number so a model can plan around it,
/// and exceeding it is an `Err` — a tool error `NativeLoop::mediate` survives.
pub const RUN_TIMEOUT: Duration = Duration::from_secs(30);

/// Whether this run may launch a process at all, and — when it may — which
/// directories its executables may come from.
///
/// A second opt-in on top of `--fs-root`, and unconditional on every OS so no
/// caller needs a `#[cfg]` around a call site. Running a process is a larger
/// capability than writing a file, so deny-by-default is structural here rather
/// than merely policy (Constitution VI).
///
/// The allowlist rides **inside** the `Allowed` arm rather than travelling
/// beside it, which makes run directories without run access unrepresentable.
/// The cost, stated: this is not `Copy`, so a caller that needs it twice clones
/// it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunAccess {
    Denied,
    Allowed(RunDirs),
}

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

/// `proc_run`'s arguments. Public for the reason [`ReadParams`] documents.
///
/// **A vector and never a command line.** There is no `cwd` — it is the
/// configured root — no `env`, no `stdin`, and no per-call timeout, because
/// each of those would be a second answer to a question this tool already
/// answers once.
///
/// The typed boundary is what makes a malformed call a refusal the model is
/// told about: `args` that is not an array of strings fails deserialization,
/// which rmcp reports as `isError: true`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunParams {
    /// Executable: a bare name found in `System32`, or a path relative to the
    /// configured root. Never a shell command line.
    pub command: String,
    /// Arguments, each a separate value. No shell syntax is interpreted.
    pub args: Vec<String>,
}

/// The tool holder. `Clone` because rmcp's router hands each request a clone of
/// the handler; the root is behind an [`Arc`] so every clone enforces the same
/// containment rule rather than a copy of it.
#[derive(Clone)]
pub struct EmbeddedServer {
    root: Arc<FsRoot>,
    /// `Some` exactly when the `proc_run` route is enabled, and behind an
    /// [`Arc`] for the reason the root is.
    ///
    /// Off Windows [`Sandbox`] is uninhabited, so this can only ever be `None`
    /// there — the platform gate needs no `#[cfg]` at this level because the
    /// type already carries it.
    sandbox: Option<Arc<Sandbox>>,
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
impl EmbeddedServer {
    /// Infallible, and the git routes are gated here rather than by refusing
    /// to build: a root that is not a repository is an ordinary, supported
    /// configuration — it is what `--fs-root` meant before this slice existed.
    ///
    /// Disabling the routes makes the server advertise what it can actually
    /// do, which is all `tools/list` has ever been, and it is why every
    /// pre-existing advertisement assertion in the workspace stays green: each
    /// one's fixture root is a plain directory.
    ///
    /// This gate is **necessary and not sufficient**. A disabled route is not
    /// found, which rmcp reports as a protocol error, `RmcpToolTransport` maps
    /// to `SkeinError::Tool`, and `NativeLoop::mediate` treats as fatal — so
    /// `skein-cli`'s allowlist must omit the same two names in the same case,
    /// turning a model's invented `git_status` into a survivable `denied`
    /// instead of the end of the run.
    pub fn new(root: FsRoot) -> Self {
        Self::build(root, None)
    }

    /// [`EmbeddedServer::new`] plus the process launcher, and **fallible**
    /// where `new` is infallible.
    ///
    /// The asymmetry is the decision. A root that is not a git repository is an
    /// ordinary configuration, so `new` gates those routes and carries on; a
    /// [`RunAccess::Allowed`] whose sandbox cannot be built is an operator
    /// error — an unsupported platform, or a directory whose ACL cannot be
    /// written — and it must be an exit code before a model is shown a tool,
    /// not a refusal per call.
    ///
    /// The route gate here is **necessary and not sufficient**, exactly as
    /// `new`'s git gate is: a disabled route is *not found*, which rmcp reports
    /// as a protocol error, `RmcpToolTransport` maps to `SkeinError::Tool`, and
    /// `NativeLoop::mediate` treats as fatal. So `skein-cli`'s allowlist must
    /// omit `proc_run` in exactly the cases this disables it, turning a model's
    /// invented `proc_run` into a survivable `denied` instead of the end of the
    /// run.
    pub fn with_run(root: FsRoot, run: RunAccess) -> skein_core::Result<Self> {
        let sandbox = match run {
            RunAccess::Denied => None,
            RunAccess::Allowed(dirs) => Some(Arc::new(
                Sandbox::create(root.path(), dirs.paths()).map_err(SkeinError::Tool)?,
            )),
        };
        Ok(Self::build(root, sandbox))
    }

    /// The one place a route is gated, so the two constructors cannot disagree
    /// about what this server can actually do.
    fn build(root: FsRoot, sandbox: Option<Arc<Sandbox>>) -> Self {
        let mut tool_router = Self::tool_router();
        if !git::is_git_repository(&root) {
            tool_router.disable_route("git_status");
            tool_router.disable_route("git_log");
        }
        // Only registered on Windows, so only disablable there. Everywhere
        // else there is no such route to advertise in the first place.
        #[cfg(windows)]
        if sandbox.is_none() {
            tool_router.disable_route("proc_run");
        }
        // The advertised description is the only channel this reaches a model
        // through: `RmcpToolTransport::list` maps name, description and
        // parameters into a `ToolSpec` and drops the server's `instructions`.
        // Appended rather than rewritten, so the `#[tool]` attribute stays the
        // single home of the rule and the caps and this only enumerates.
        #[cfg(windows)]
        if let Some(dirs) = sandbox
            .as_ref()
            .map(|sandbox| sandbox.run_dirs())
            .filter(|dirs| !dirs.is_empty())
        {
            // rmcp 2.2's `ToolRouter::map` and `ToolRoute::attr` are `pub`;
            // `#[non_exhaustive]` forbids constructing one of these outside the
            // crate, not mutating a field of one its macro already built. A
            // future minor that reorganises them is a compile error at this
            // line, which is what the advertisement test exists to catch.
            if let Some(route) = tool_router.map.get_mut("proc_run") {
                let places: Vec<String> = dirs.iter().map(|dir| crate::run::named(dir)).collect();
                let base = route.attr.description.take().unwrap_or_default();
                route.attr.description = Some(
                    format!(
                        "{base} A bare name is also looked for in: {}.",
                        places.join(", ")
                    )
                    .into(),
                );
            }
        }
        EmbeddedServer {
            root: Arc::new(root),
            sandbox,
            tool_router,
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

    /// Windows-only in v0 (ADR-0006). On the other two platforms this route
    /// does not exist, so nothing advertises it and nothing can call it.
    #[cfg(windows)]
    #[tool(
        description = "Run one program inside a Windows sandbox over the configured root, and \
                       return its exit code and both output streams. `command` is either a bare \
                       name found in %SystemRoot%\\System32 (PATH is not searched) or a path \
                       relative to the configured root; `args` is a list of separate values. No \
                       shell is involved: pipes, redirection, `&&`, globbing and variable \
                       expansion are not interpreted. The process cannot reach the network and \
                       cannot write outside the configured root, it starts in that root, it gets \
                       no stdin, and it is killed after 30 seconds. Each output stream is \
                       truncated at 16384 bytes with a note saying how much was dropped."
    )]
    pub fn proc_run(&self, params: Parameters<RunParams>) -> Result<String, String> {
        let RunParams { command, args } = params.0;
        // Unreachable while `build` is the only constructor of this field: it
        // disables the route in exactly the `None` case. It is an `Err` rather
        // than an `expect` because a panic inside an rmcp handler would take
        // the session with it, where a tool error is something the model is
        // told and the run survives.
        let sandbox = self
            .sandbox
            .as_ref()
            .ok_or_else(|| "this run was not started with process launching enabled".to_string())?;
        crate::run::execute(sandbox, &self.root, &command, &args)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for EmbeddedServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Skein's embedded connector over one operator-named root. Every filesystem path is \
             relative to that root and nothing outside it is reachable. The git tools, when they \
             are offered at all, report on the git repository at exactly that root and take no \
             path arguments; when the root is not a repository they are not offered.",
        )
    }
}
