//! What every command that runs the loop needs: the model flags, the loopback
//! guard, the budget, and the two honest stand-ins `NativeLoop` requires.
//!
//! `skein chat` and `skein acp-agent` reach the same provider under the same
//! Principle II rule. A second copy of that rule is a second thing to keep
//! right, and the one it guards — "a local provider is the only thing this can
//! talk to" — is NON-NEGOTIABLE, so there is exactly one.

use clap::Args;
use skein_connectors::{
    is_git_repository, local_connector_with_run, FsRoot, LocalConnector, RunAccess,
};
// Only the Windows arm of `RunArgs::resolve` builds one; elsewhere `--allow-run`
// is a refusal before any directory is validated.
#[cfg(windows)]
use skein_connectors::RunDirs;
use skein_core::{
    LoopBudget, ProgressProbe, Redactor, Result, SecretRef, SkeinError, ToolAccess, ToolCall,
    ToolOutcome, ToolPolicy, ToolSpec, ToolTransport,
};
use skein_gateway::{LocalEndpoint, OpenAiCompatClient};
use skein_silo::OsKeychain;
use std::path::PathBuf;
use std::time::Duration;

/// Ollama's own OpenAI-compatible endpoint, which `scripts/bootstrap.ps1
/// -WithOllama` installs. A LiteLLM sidecar is a different `--base-url` and no
/// code change.
const DEFAULT_BASE_URL: &str = "http://localhost:11434/v1";

/// Which model, where, and under what budget. Every budget flag maps onto one
/// `LoopBudget` field, so the CLI names the engine's policy and does not invent
/// its own.
#[derive(Args)]
pub struct ModelArgs {
    /// Model name as the local provider knows it. Required: defaulting to a
    /// model the machine may not have produces a 404 that looks like a bug.
    #[arg(long, value_name = "NAME")]
    pub model: String,
    /// OpenAI-compatible base URL. Defaults to $SKEIN_MODEL_BASE_URL, else
    /// http://localhost:11434/v1. Loopback only.
    #[arg(long, value_name = "URL")]
    pub base_url: Option<String>,
    #[arg(long, value_name = "N", default_value_t = 8)]
    pub max_iters: u32,
    #[arg(long, value_name = "N", default_value_t = 100_000)]
    pub max_tokens: u64,
    #[arg(long, value_name = "N", default_value_t = 8)]
    pub no_progress_limit: u32,
    /// Whole-request budget for one turn.
    #[arg(long, value_name = "S", default_value_t = 120)]
    pub timeout_secs: u64,
}

impl ModelArgs {
    /// `--base-url`, else `$SKEIN_MODEL_BASE_URL`, else Ollama's default, run
    /// through the loopback guard.
    ///
    /// Call this **before** opening a silo: an endpoint that cannot be built is
    /// an endpoint no socket was opened to, and a chain holding a one-step run
    /// would be a misleading record of an attempt that never left the process.
    pub fn endpoint(&self) -> Result<LocalEndpoint> {
        let base_url = match &self.base_url {
            Some(url) => url.clone(),
            None => {
                std::env::var("SKEIN_MODEL_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.into())
            }
        };
        LocalEndpoint::parse(&base_url)
    }

    pub fn client(&self, endpoint: LocalEndpoint) -> OpenAiCompatClient {
        OpenAiCompatClient::new(
            endpoint,
            &self.model,
            Duration::from_secs(self.timeout_secs),
        )
    }

    pub fn budget(&self) -> LoopBudget {
        LoopBudget::new(self.max_iters, self.max_tokens, self.no_progress_limit)
    }
}

/// Which secrets this run must never write into its chain. References only:
/// there is no `--redact-value`, for the reason `skein secret set` has no
/// `--value` — a secret in a flag lands in shell history and in process
/// listings.
///
/// Deliberately not part of [`ModelArgs`]: redaction is run-governance, not a
/// model knob.
#[derive(Args)]
pub struct RedactArgs {
    /// keychain://<service>/<account>. Repeatable.
    #[arg(long = "redact", value_name = "REFERENCE")]
    pub redact: Vec<String>,
}

impl RedactArgs {
    /// Resolves every reference through the platform credential store.
    ///
    /// With no `--redact` the store is **not opened**: a run that configures no
    /// secret must not acquire a runtime credential-store dependency.
    ///
    /// Call this **before** opening a silo, for the reason
    /// [`ModelArgs::endpoint`] documents: `Redactor::resolve` is all-or-nothing,
    /// so one bad reference stops the run, and a chain holding a one-step run
    /// would be a misleading record of an attempt that never left the process.
    pub fn redactor(&self) -> Result<Redactor> {
        if self.redact.is_empty() {
            return Ok(Redactor::new(Vec::new()));
        }
        let refs: Vec<SecretRef> = self.redact.iter().cloned().map(SecretRef).collect();
        Redactor::resolve(&OsKeychain::new()?, &refs)
    }
}

/// A conversation with no tools has **no external ground truth**, and
/// Constitution VIII(b) forbids substituting the model's own judgment for one.
/// So every iteration is stale, and a model that never finishes is stopped by
/// the no-progress budget.
///
/// In the normal case this never bites: `should_exit` checks `final_output`
/// first, and a tool-less turn returns `finish_reason: "stop"` on iteration 1.
pub struct NoGroundTruth;

impl ProgressProbe for NoGroundTruth {
    fn observe(&mut self) -> bool {
        false
    }
}

/// Unreachable by construction: paired with an empty [`ToolPolicy`], every tool
/// name is refused before the transport is consulted. It exists because
/// `NativeLoop` is generic over a transport, not because a tool could run —
/// and it is what [`ConfiguredTools::None`] delegates to.
pub struct NoTools;

impl ToolTransport for NoTools {
    fn call(&mut self, call: &ToolCall) -> Result<ToolOutcome> {
        Err(SkeinError::Tool(format!(
            "no tool server is configured in this command: {} was not called",
            call.tool
        )))
    }
}

/// Which directory, if any, this run's tools may reach.
///
/// Deliberately not part of [`ModelArgs`], for the reason [`RedactArgs`] is
/// not: what an agent may touch is run-governance, not a model knob.
#[derive(Args)]
pub struct ToolArgs {
    /// The one directory an agent may work in: the filesystem tools always,
    /// and the git tools when that directory is a git repository. Absent, the
    /// run has no tools at all — every path outside it is unreachable, and so
    /// is every path when no root is named.
    #[arg(long, value_name = "PATH")]
    pub fs_root: Option<PathBuf>,
}

impl ToolArgs {
    /// Proves the root exists **without** starting a server, for the ordering
    /// [`ModelArgs::endpoint`] documents: a mistyped `--fs-root` must be an
    /// exit code before a chain is opened or a protocol handshake happens, not
    /// an error an operator only meets inside an editor afterwards.
    ///
    /// `skein chat` gets this for free from [`ToolArgs::transport`], which it
    /// calls at the same point in its sequence. `skein acp-agent` cannot: it
    /// builds one connector per session, inside the session factory, long after
    /// it has begun serving.
    pub fn verify_root(&self) -> Result<()> {
        match &self.fs_root {
            Some(path) => FsRoot::new(path).map(|_| ()),
            None => Ok(()),
        }
    }

    /// The transport this run's gateway reaches. One embedded server per call:
    /// [`LocalConnector`] is not shareable, so an ACP session gets its own —
    /// and its own tokio runtime with it, which is the accepted v0 cost of
    /// matching the one-client-per-session shape sessions already have.
    ///
    /// **Not callable from inside a tokio context**, per
    /// [`LocalConnector`]'s docstring.
    pub fn transport(&self, run: RunAccess) -> Result<ConfiguredTools> {
        match &self.fs_root {
            Some(path) => Ok(ConfiguredTools::Fs(Box::new(local_connector_with_run(
                FsRoot::new(path)?,
                run,
            )?))),
            None => Ok(ConfiguredTools::None),
        }
    }

    /// `skein chat`'s allowlist: the read-only tools, and **not** `fs_write`.
    ///
    /// The omission is the decision. Constitution VI requires confirmation for
    /// a destructive action and this command is non-interactive, so there is
    /// nobody to ask. Shipping `fs_write` here would mean shipping a tool that
    /// could only ever be denied; leaving it off the list makes it a genuinely
    /// unlisted tool, refused by the policy before the transport is consulted
    /// and with a reason the model is told.
    pub fn chat_policy(&self) -> ToolPolicy {
        let mut allowed = read_only();
        allowed.extend(self.git_tools());
        self.policy(allowed, Vec::new())
    }

    /// `skein acp-agent`'s allowlist: everything [`ToolArgs::chat_policy`]
    /// lists, plus `fs_write` — which is also in `approved`, and that is not a
    /// weakening.
    ///
    /// `ToolGateway::call_captured` consults the policy **before** the
    /// transport, so a `Mutating` tool absent from `approved` never reaches
    /// `AcpPermissionTransport` and therefore never becomes a question for the
    /// human behind the editor. Approving it here is the only way to move the
    /// decision to where a human actually is.
    /// `proc_run` joins on exactly the same terms as `fs_write`, and for the
    /// same recorded reason — and it is omitted in exactly the case
    /// [`EmbeddedServer::with_run`] disables the route, because an allowlisted
    /// name with a disabled route is **fatal** where an unlisted name is a
    /// survivable `denied`.
    pub fn agent_policy(&self, run: &RunAccess) -> ToolPolicy {
        let mut allowed = read_only();
        allowed.push(("fs_write".to_string(), ToolAccess::Mutating));
        allowed.extend(self.git_tools());
        let mut approved = vec!["fs_write".to_string()];
        if matches!(run, RunAccess::Allowed(_)) {
            allowed.push(("proc_run".to_string(), ToolAccess::Mutating));
            approved.push("proc_run".to_string());
        }
        self.policy(allowed, approved)
    }

    /// The two git tools when the configured root is a git repository, and
    /// nothing otherwise.
    ///
    /// Both commands append the same pair: both tools are `ReadOnly` because
    /// neither mutates anything, so there is no `fs_write`-style approval
    /// asymmetry to make — this slice built nothing to confirm.
    ///
    /// **Omitting the names is what keeps a run alive, not what protects the
    /// repository.** `EmbeddedServer` already disables both routes in exactly
    /// this case, but a disabled route is *not found*, which rmcp reports as a
    /// protocol error, `RmcpToolTransport` maps to `SkeinError::Tool`, and
    /// `NativeLoop::mediate` treats as fatal. Leaving the names off the
    /// allowlist turns a model's invented `git_status` into a survivable
    /// `denied` with a reason, which is what [`ToolArgs::policy`] below already
    /// demands for the no-root case.
    ///
    /// Swallowing a bad root here is safe and deliberate: `transport()` and
    /// `verify_root()` have already failed loudly on a mistyped `--fs-root`
    /// earlier in both commands' documented ordering, so this can never be the
    /// thing that hides an operator's typo.
    fn git_tools(&self) -> Vec<(String, ToolAccess)> {
        let is_repository = self
            .fs_root
            .as_ref()
            .and_then(|path| FsRoot::new(path).ok())
            .is_some_and(|root| is_git_repository(&root));
        if is_repository {
            vec![
                ("git_status".to_string(), ToolAccess::ReadOnly),
                ("git_log".to_string(), ToolAccess::ReadOnly),
            ]
        } else {
            Vec::new()
        }
    }

    /// Deny-by-default when no root is configured, and this is **not**
    /// cosmetic. [`ConfiguredTools::None`] *fails* a call rather than serving
    /// it, and `NativeLoop::mediate` survives only `ToolDenied` — any other
    /// transport error ends the run. So an allowlisted name with no connector
    /// behind it would turn a model's invented tool call into a dead run
    /// instead of a refusal it is told about.
    fn policy(&self, allowed: Vec<(String, ToolAccess)>, approved: Vec<String>) -> ToolPolicy {
        match self.fs_root {
            Some(_) => ToolPolicy::new(allowed, approved),
            None => ToolPolicy::new(Vec::new(), Vec::new()),
        }
    }
}

/// Whether this run may launch a process, and the honest statement of what
/// saying yes costs.
///
/// A **second** opt-in on top of `--fs-root`, deliberately: running a process
/// is a larger capability than writing a file, and Constitution VI's
/// deny-by-default is worth making structural rather than leaving to policy.
/// Flattened into `skein acp-agent` and nowhere else — [`ToolArgs::chat_policy`]
/// records why a mutating tool that could only ever be denied belongs *absent*
/// from a non-interactive command rather than listed in it.
#[derive(Args)]
pub struct RunArgs {
    /// Offer the sandboxed `proc_run` tool over --fs-root. **Windows only in
    /// v0**, and elsewhere a refusal rather than a missing tool. Grants this
    /// run's AppContainer identity an inheritable entry on that directory's
    /// ACL, which is a real and lasting change to the directory's permissions.
    #[arg(long)]
    pub allow_run: bool,
    /// A directory whose executables `proc_run` may resolve by bare name and
    /// run. Repeatable, and needs --allow-run. Grants this run's AppContainer
    /// identity a read-and-execute entry on that directory — narrower than
    /// --fs-root's, and still a real and lasting change to its permissions.
    /// Nothing is discovered: %PATH% is never searched and no toolchain
    /// location is guessed at.
    #[arg(long = "run-dir", value_name = "PATH", requires = "allow_run")]
    pub run_dir: Vec<PathBuf>,
}

impl RunArgs {
    /// Resolves the flags against the platform and the filesystem, **loudly**.
    ///
    /// Call this **before** opening a silo, in the position
    /// [`ToolArgs::verify_root`] already occupies and for the same documented
    /// reason: an unsupported flag or a mistyped directory must be an exit code
    /// and a message, not a JSON-RPC error an operator only meets inside an
    /// editor after a successful handshake.
    ///
    /// The `#[cfg]` is here rather than deeper because this is the layer that
    /// owns the *operator's* answer. Below it, `skein-sandbox` refuses on the
    /// same platforms for the same reason — but by then a session exists.
    ///
    /// The `--run-dir`-without-`--allow-run` check duplicates clap's own
    /// `requires` relation on purpose. A second reader of this flag — a future
    /// subcommand that flattens `RunArgs` without clap's derive doing the
    /// checking — must not be able to lose the gate silently.
    pub fn resolve(&self) -> Result<RunAccess> {
        if !self.allow_run {
            if !self.run_dir.is_empty() {
                return Err(SkeinError::Tool(
                    "--run-dir names a directory for a tool this run does not offer; it needs \
                     --allow-run"
                        .into(),
                ));
            }
            return Ok(RunAccess::Denied);
        }
        #[cfg(windows)]
        {
            Ok(RunAccess::Allowed(RunDirs::new(&self.run_dir)?))
        }
        #[cfg(not(windows))]
        {
            Err(SkeinError::Tool(
                "--allow-run needs a sandboxed process launcher, which has no backend on this \
                 platform; shell tools are Windows-only in v0"
                    .into(),
            ))
        }
    }
}

/// `fs_read` and `fs_list` are `ReadOnly` because they mutate nothing;
/// `fs_write` is `Mutating` because it destroys a file's prior contents.
/// Classification is operator configuration, never read from the server's own
/// annotations — a server does not get to declare its own risk.
fn read_only() -> Vec<(String, ToolAccess)> {
    vec![
        ("fs_read".to_string(), ToolAccess::ReadOnly),
        ("fs_list".to_string(), ToolAccess::ReadOnly),
    ]
}

/// What `--fs-root` resolved to. An enum rather than `Box<dyn ToolTransport>`
/// because `NativeLoop` is generic over its transport by deliberate design and
/// the dispatch is a two-arm match.
///
/// The `Fs` variant is boxed: [`LocalConnector`] owns a tokio runtime and
/// [`NoTools`] is zero-sized, which is `clippy::large_enum_variant`.
pub enum ConfiguredTools {
    None,
    Fs(Box<LocalConnector>),
}

/// Delegation both ways, so the "no root" arm keeps [`NoTools`]'s message and
/// its reasoning in one place rather than a second copy of them here.
impl ToolTransport for ConfiguredTools {
    fn call(&mut self, call: &ToolCall) -> Result<ToolOutcome> {
        match self {
            ConfiguredTools::None => NoTools.call(call),
            ConfiguredTools::Fs(connector) => connector.call(call),
        }
    }

    fn list(&mut self) -> Result<Vec<ToolSpec>> {
        match self {
            ConfiguredTools::None => NoTools.list(),
            ConfiguredTools::Fs(connector) => connector.list(),
        }
    }
}
