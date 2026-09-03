//! One bounded process, and the two Windows primitives that bound it.
//!
//! **This is the only crate in the workspace that contains `unsafe`, and that
//! is its reason to exist.** There was none anywhere in `crates/` before this
//! slice; a reviewer auditing memory safety has exactly one directory to read
//! — the same discipline that makes `skein-connectors` the only crate naming
//! MCP as a server and its `src/git.rs` the only module naming `git2`.
//!
//! The two bounds are an **AppContainer** (a low-integrity token carrying an
//! AppContainer SID and, deliberately, *zero* capability SIDs) and a **Job
//! Object** with kill-on-close. The first is what makes the child unable to
//! open anything whose DACL does not name its SID, and what leaves the Windows
//! Filtering Platform no permit filter to match on, so it reaches no network
//! (Constitution II, NON-NEGOTIABLE). The second is what makes the whole
//! process tree — grandchildren included — die with the launch.
//!
//! **The containment mechanism is the DACL, not a path check.** A spawned
//! process never passes through `FsRoot::resolve`; what stops it writing
//! outside the configured root is that no directory outside the root has an ACE
//! naming its AppContainer SID. What it *can* still read is anything carrying
//! `ALL APPLICATION PACKAGES` — `C:\Windows\System32` among them, and it must
//! be able to or no executable would launch. So the provable claim is *cannot
//! write outside its root*, never *cannot read anything outside it*.
//!
//! ADR-0006 ships this Windows-first. There is no Linux or macOS backend and
//! none is stubbed: on those platforms [`Sandbox`] is an **uninhabited** type
//! and [`Sandbox::create`] is a loud refusal.

#[cfg(windows)]
mod argv;
#[cfg(windows)]
mod launch;
#[cfg(windows)]
mod profile;

use std::path::Path;
use std::time::Duration;

/// One captured stream, and how much of it was thrown away.
///
/// `dropped_bytes` is not an estimate: the reader keeps draining past the cap
/// and counts, so the child never blocks on a full pipe and the number is the
/// real one.
#[derive(Debug)]
pub struct Captured {
    pub text: String,
    pub dropped_bytes: usize,
}

/// What one bounded launch produced.
#[derive(Debug)]
pub struct Run {
    pub exit_code: u32,
    pub stdout: Captured,
    pub stderr: Captured,
}

/// The AppContainer identity for one directory, and the licence to launch
/// inside it.
///
/// Holds the **string** SID rather than a live `PSID`, and rebuilds a `PSID`
/// per launch. That is what makes this type `Send + Sync` **by construction**
/// — no `unsafe impl` — which matters because rmcp's handler must be `Clone +
/// Send + Sync + 'static` and the connector holds one of these.
#[cfg(windows)]
pub struct Sandbox {
    root: std::path::PathBuf,
    /// The string SID as UTF-16, NUL-terminated, ready for
    /// `ConvertStringSidToSidW`.
    sid: Vec<u16>,
}

/// Uninhabited off Windows, which is the platform gate stated in the type
/// system rather than in a runtime branch: [`Sandbox::create`] is the only way
/// to obtain one and it always refuses, so [`Sandbox::run`] is unreachable and
/// the compiler knows it.
#[cfg(not(windows))]
pub struct Sandbox(std::convert::Infallible);

#[cfg(not(windows))]
const NO_BACKEND: &str = "a sandboxed process launcher has no backend on this platform; shell \
                          tools are Windows-only in v0";

impl Sandbox {
    /// Creates — or reuses — the AppContainer profile for `root`, and grants
    /// its SID an inheritable full-access ACE on `root`.
    ///
    /// **This modifies the ACL of a directory the operator named.** It is the
    /// only way an AppContainer process can see that workspace at all, it is
    /// scoped to the one directory `--fs-root` already designates, and it is
    /// stated in the flag's doc comment, in the tool's description and in
    /// `spec.md`. The rejected alternative — telling the operator to run
    /// `icacls` by hand — trades a stated, scoped side effect for a silent
    /// `ERROR_ACCESS_DENIED` at first use.
    ///
    /// Fails loudly, never silently: a sandbox that cannot be built must be an
    /// exit code before a model sees a tool, not a per-call refusal.
    #[cfg(windows)]
    pub fn create(root: &Path) -> std::result::Result<Sandbox, String> {
        todo!("T3")
    }

    #[cfg(not(windows))]
    pub fn create(_root: &Path) -> std::result::Result<Sandbox, String> {
        Err(NO_BACKEND.to_string())
    }

    /// One process, bounded by the Job Object and the AppContainer.
    ///
    /// `exe` is absolute and already resolved; `args` are argv values, never a
    /// shell command line. `stream_cap` bounds each captured stream and
    /// `timeout` bounds the wall clock — a timeout kills the tree and is an
    /// `Err`, which is a tool error the loop survives.
    #[cfg(windows)]
    pub fn run(
        &self,
        exe: &Path,
        args: &[String],
        stream_cap: usize,
        timeout: Duration,
    ) -> std::result::Result<Run, String> {
        todo!("T4")
    }

    /// Unreachable by construction: [`Sandbox`] is uninhabited on this
    /// platform, so there is no `self` to call this on.
    #[cfg(not(windows))]
    pub fn run(
        &self,
        _exe: &Path,
        _args: &[String],
        _stream_cap: usize,
        _timeout: Duration,
    ) -> std::result::Result<Run, String> {
        match self.0 {}
    }
}
