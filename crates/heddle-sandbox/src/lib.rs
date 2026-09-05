//! One bounded process, and the two Windows primitives that bound it.
//!
//! **This is the only crate in the workspace that contains `unsafe`, and that
//! is its reason to exist.** There was none anywhere in `crates/` before this
//! slice; a reviewer auditing memory safety has exactly one directory to read
//! — the same discipline that makes `heddle-connectors` the only crate naming
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
mod cleanup;
#[cfg(windows)]
mod launch;
#[cfg(windows)]
mod profile;
#[cfg(windows)]
mod record;

use std::path::{Path, PathBuf};
use std::time::Duration;

/// The most arguments one launch may carry.
///
/// A bound on how much of a model's turn can become one command line, in the
/// spirit of `heddle-connectors`' own read and log caps. It lives here rather
/// than beside them because the refusal is the launcher's: it is what
/// `CreateProcessW` can be handed, not what a tool chooses to offer.
pub const ARG_COUNT_CAP: usize = 64;

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
    /// The AppContainer profile's name, `heddle-` plus 16 hex characters of
    /// `sha256(root)`. Kept rather than re-derived because it is what
    /// [`prune`] selects on and what the record file is filed under, and one
    /// derivation cannot disagree with itself.
    profile: String,
    /// The **only** store of the operator's allowlist: executable resolution
    /// and the child's `PATH` both read it back out of here, so the
    /// directories that are searched and the directories that were granted an
    /// ACE cannot drift apart.
    run_dirs: Vec<std::path::PathBuf>,
    sid: String,
}

/// Uninhabited off Windows, which is the platform gate stated in the type
/// system rather than in a runtime branch: [`Sandbox::create`] is the only way
/// to obtain one and it always refuses, so [`Sandbox::run`] is unreachable and
/// the compiler knows it.
#[cfg(not(windows))]
pub struct Sandbox(std::convert::Infallible);

/// A path in the form the **name-based** Win32 APIs accept.
///
/// `FsRoot` canonicalizes once in its constructor, which on Windows yields a
/// `\\?\`-verbatim path — and neither the ADVAPI32 name-based security
/// functions nor `CreateProcessW`'s `lpCurrentDirectory` is documented to
/// accept that prefix. Stripping it here rather than in each caller keeps the
/// one rule in one place, and keeps the verbatim form everywhere Rust's own
/// path comparisons need it.
#[cfg(windows)]
fn win32_path(path: &Path) -> String {
    let rendered = path.to_string_lossy();
    match rendered.strip_prefix(r"\\?\") {
        // `\\?\UNC\server\share` is a share, and dropping the whole prefix
        // would leave the bare word `UNC` as the volume.
        Some(rest) => match rest.strip_prefix(r"UNC\") {
            Some(share) => format!(r"\\{share}"),
            None => rest.to_string(),
        },
        None => rendered.into_owned(),
    }
}

#[cfg(not(windows))]
const NO_BACKEND: &str = "a sandboxed process launcher has no backend on this platform; shell \
                          tools are Windows-only in v0";

/// Which flag put a directory on a profile's record: the one workspace, or one
/// of the executable directories.
///
/// It is the record's line order, not a second stored field — [`Sandbox::create`]
/// writes the root first — so the two cannot drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantKind {
    Root,
    RunDir,
}

/// What the directory's DACL says **right now**, not what the record claims.
///
/// Computed with one `GetNamedSecurityInfoW` per directory. That cost is the
/// point: a listing that echoed the record would report a grant an `icacls` had
/// already removed, and an operator would prune something that was not there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantState {
    Granted,
    Clear,
    Missing,
}

/// One directory a profile was recorded against, and its live state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantedDir {
    pub path: PathBuf,
    pub kind: GrantKind,
    pub state: GrantState,
}

/// One AppContainer profile Heddle created.
///
/// `dirs` is `None` when the profile carries no record — every profile made
/// before this slice, and the shape [`prune`] reports as `unrecorded`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub profile: String,
    pub sid: String,
    pub dirs: Option<Vec<GrantedDir>>,
}

/// What one [`prune`] removed, per directory and then the profile itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pruned {
    pub profile: String,
    pub revoked: Vec<PathBuf>,
    pub clear: Vec<PathBuf>,
    pub missing: Vec<PathBuf>,
    /// The profile carried no record, so its directories are unknown and none
    /// was touched. Not an error: refusing these would leave every profile made
    /// before this slice permanently unremovable.
    pub unrecorded: bool,
}

/// Every AppContainer profile on this machine that Heddle could have created,
/// with the directories each was recorded against and their live DACL state.
///
/// A free function rather than a method: no [`Sandbox`] is alive at cleanup
/// time, and off Windows the type is uninhabited.
///
/// `%LOCALAPPDATA%\Packages` is scanned for `heddle-` plus 16 lowercase hex
/// characters and nothing else, because `Win32::Security::Isolation` offers no
/// enumeration API — the profile folder is the only self-naming, machine-wide
/// artifact a created profile leaves behind.
#[cfg(windows)]
pub fn grants() -> std::result::Result<Vec<Grant>, String> {
    cleanup::grants()
}

#[cfg(not(windows))]
pub fn grants() -> std::result::Result<Vec<Grant>, String> {
    Err(NO_CLEANUP.to_string())
}

/// Revokes `profile`'s AppContainer SID from each directory it was recorded
/// against, then deletes the profile.
///
/// **It cannot remove an ACE it did not write, and that is structural rather
/// than careful.** The name is refused unless it matches `heddle-` plus 16
/// lowercase hex, before any Win32 call is reached; the ACL write is one
/// `REVOKE_ACCESS` entry naming one `TRUSTEE_IS_SID` — the SID derived from that
/// very name — so no other trustee's ACE is *representable* in it; and each
/// directory's live DACL is read first, so one carrying no such ACE is reported
/// and not written at all.
///
/// Order is ACEs first and the profile last. Deleting the profile deletes the
/// record with it, so the reverse order would orphan every ACE not yet revoked,
/// with nothing left to say where they were.
#[cfg(windows)]
pub fn prune(profile: &str) -> std::result::Result<Pruned, String> {
    cleanup::prune(profile)
}

#[cfg(not(windows))]
pub fn prune(_profile: &str) -> std::result::Result<Pruned, String> {
    Err(NO_CLEANUP.to_string())
}

/// The refusal [`grants`] and [`prune`] return off Windows.
///
/// Distinct from [`NO_BACKEND`] because the reason is one step further along:
/// nothing on this platform ever *created* a profile, so there is nothing to
/// find and nothing to remove. The refusal is the honest answer rather than a
/// stub standing in for missing work.
#[cfg(not(windows))]
const NO_CLEANUP: &str = "there are no sandbox profiles to list or prune on this platform; \r
                          the app container sandbox is Windows-only in v0";

impl Sandbox {
    /// Creates — or reuses — the AppContainer profile for `root`, grants its
    /// SID an inheritable full-access ACE on `root`, and grants it an
    /// inheritable **read-and-execute** ACE on each of `run_dirs`.
    ///
    /// **This modifies the ACL of every directory the operator named.** It is
    /// the only way an AppContainer process can see that workspace or launch
    /// those executables at all, each directory is one `--fs-root` or
    /// `--run-dir` already designates, and it is stated in each flag's doc
    /// comment, in the tool's description and in `spec.md`. The rejected
    /// alternative — telling the operator to run `icacls` by hand — trades a
    /// stated, scoped side effect for a silent `ERROR_ACCESS_DENIED` at first
    /// use.
    ///
    /// The two masks differ because the two capabilities do: a workspace has
    /// to be writable and a toolchain directory does not, and a child that
    /// could overwrite `cargo.exe` would leave a side effect outliving the run.
    ///
    /// Fails loudly, never silently: a sandbox that cannot be built must be an
    /// exit code before a model sees a tool, not a per-call refusal.
    #[cfg(windows)]
    pub fn create(
        root: &Path,
        run_dirs: &[std::path::PathBuf],
    ) -> std::result::Result<Sandbox, String> {
        profile::create(root, run_dirs)
    }

    #[cfg(not(windows))]
    pub fn create(
        _root: &Path,
        _run_dirs: &[std::path::PathBuf],
    ) -> std::result::Result<Sandbox, String> {
        Err(NO_BACKEND.to_string())
    }

    /// The AppContainer identity this sandbox launches under, in `S-1-15-2-…`
    /// form.
    ///
    /// Public so a test can read the grant back off the directory's **own**
    /// security descriptor and compare, rather than trusting what
    /// [`Sandbox::create`] says it wrote.
    #[cfg(windows)]
    pub fn string_sid(&self) -> &str {
        &self.sid
    }

    /// The name of the AppContainer profile this sandbox uses, which is also
    /// the selector [`prune`] takes.
    ///
    /// Public for the reason [`Sandbox::string_sid`] is: a caller that has just
    /// created a sandbox must be able to name the machine state it created —
    /// which is what a test fixture's cleanup guard needs, and what an operator
    /// reads out of `heddle sandbox list`.
    #[cfg(windows)]
    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// The directories this sandbox actually granted, canonical and in the
    /// operator's order.
    ///
    /// Public for the reason [`Sandbox::string_sid`] is, and load-bearing for a
    /// second one: this is where executable resolution and the child's `PATH`
    /// read the allowlist from, so the only searchable directories are the ones
    /// that were really granted.
    #[cfg(windows)]
    pub fn run_dirs(&self) -> &[std::path::PathBuf] {
        &self.run_dirs
    }

    /// One process, bounded by the Job Object and the AppContainer.
    ///
    /// `exe` is absolute and already resolved; `args` are argv values, never a
    /// shell command line. `stream_cap` bounds each captured stream and
    /// `timeout` bounds the wall clock — a timeout kills the tree and is an
    /// `Err`, which is a tool error the loop survives.
    ///
    /// `cancelled` is the third bound and the only one the caller controls
    /// while the child runs: setting it kills the tree the same way the timeout
    /// does, within 50 ms, and yields an `Err` naming the cancellation rather
    /// than the clock. Borrowed rather than owned because this call does not
    /// outlive it, and read from another thread — in the product it is the ACP
    /// session's flag, set by the connection's dispatch task while the loop
    /// thread is blocked in here.
    #[cfg(windows)]
    pub fn run(
        &self,
        exe: &Path,
        args: &[String],
        stream_cap: usize,
        timeout: Duration,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> std::result::Result<Run, String> {
        launch::run(self, exe, args, stream_cap, timeout, cancelled)
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
        _cancelled: &std::sync::atomic::AtomicBool,
    ) -> std::result::Result<Run, String> {
        match self.0 {}
    }
}
