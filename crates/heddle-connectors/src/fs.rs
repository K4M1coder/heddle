//! One operator-named directory, and the rule that nothing leaves it.
//!
//! **Scope honesty:** this is *one* root with no traversal outside it. It is
//! not design §5.5's scope-owner hierarchy — `AccessScope::{Project, Folder,
//! FullComputer}` does not exist in this workspace — and it must not be read as
//! that hierarchy having landed.

use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
use heddle_core::{HeddleError, Result};
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

/// A canonicalized directory, and the only way a tool names a file.
///
/// Every path a model asks for is resolved through this type, and a resolution
/// either lands inside the root or is refused. The refusal is a `String`
/// because it is told to the model: the far end turns it into a tool-level
/// error rather than a transport failure (see [`crate::EmbeddedServer`]).
///
/// The containment decision and the open are **one walk**. Every path a model
/// supplies is resolved component by component against `dir`, a handle taken
/// once at construction, so there is no window in which the name that was
/// checked can come to mean something else. That is what closes slice 016's
/// recorded `canonicalize`-to-open TOCTOU, and it is why this type hands out
/// open files rather than paths: a caller given a `PathBuf` re-walks it.
///
/// **Three residuals remain, named rather than hidden.**
///
/// 1. `git_status` and `git_log` open the repository **by path**
///    ([`FsRoot::path`]): `git2::Repository::open` takes an `AsRef<Path>` and
///    libgit2 performs every subsequent open by path inside its own C code.
///    What this type does give them is the pinning below — the root itself
///    cannot be swapped under a running session — so the window that remains
///    is inside `.git`, below the root.
/// 2. `proc_run` keeps [`FsRoot::resolve`], because `CreateProcessW` takes a
///    path and there is no handle-relative process launch. Containment for the
///    child is the AppContainer's DACL, the Job Object and the per-call human
///    approval, not the path — see `crate::run`.
/// 3. A **hard link** inside the root pointing at a file outside it is read by
///    this mechanism and was read by the old one, because the directory entry
///    genuinely is inside the root. Not a TOCTOU, and not closed: telling it
///    apart needs device+inode identity, which is unstable in std on Windows.
///
/// **The handle is held for this value's whole lifetime, and that is a second
/// property rather than a side effect:** the operator-named directory cannot be
/// renamed, replaced or deleted while a session runs. On Windows an attempt
/// fails with a sharing violation. Opening the handle per call would avoid the
/// lock, at the price of leaving the root's *own name* re-walkable between
/// calls — the outermost component of the very window being closed.
#[derive(Debug)]
pub struct FsRoot {
    root: PathBuf,
    dir: Dir,
}

impl FsRoot {
    /// Canonicalized once, here, so that every later prefix comparison compares
    /// two canonical paths — on Windows that means both sides are
    /// `\\?\`-verbatim, and comparing a verbatim path against a non-verbatim
    /// one would never match.
    ///
    /// A root that does not exist is a **loud** failure at construction rather
    /// than a per-call refusal, because an operator who mistyped `--fs-root`
    /// wants to hear about it before a model does.
    pub fn new(path: impl AsRef<Path>) -> Result<FsRoot> {
        let path = path.as_ref();
        let root = std::fs::canonicalize(path)
            .map_err(|e| HeddleError::Tool(format!("fs root {}: {e}", path.display())))?;
        if !root.is_dir() {
            return Err(HeddleError::Tool(format!(
                "fs root {} is not a directory",
                path.display()
            )));
        }
        // Held from here until this value drops, which is the pinning the
        // docstring above states. A root that cannot be opened is the same
        // loud construction failure a root that cannot be canonicalized is.
        let dir = Dir::open_ambient_dir(&root, ambient_authority())
            .map_err(|e| HeddleError::Tool(format!("fs root {}: {e}", path.display())))?;
        Ok(FsRoot { root, dir })
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    /// An existing path under the root, canonicalized.
    ///
    /// Canonicalizing before the prefix check closes `..` traversal **and**
    /// symlink escape in one step: a link inside the root pointing outside it
    /// canonicalizes to its target, which fails the check. What it does not
    /// close is the window between that check and whatever the caller does with
    /// the `PathBuf` afterwards.
    ///
    /// **One product caller: `crate::run::resolve_exe`**, which needs a path
    /// because `CreateProcessW` takes one. It therefore keeps residual 2 named
    /// in this type's docstring. Every `fs` tool goes through
    /// [`FsRoot::open_file`], [`FsRoot::create_file`] or [`FsRoot::read_dir`]
    /// instead, and no new caller should be added here.
    pub fn resolve(&self, arg: &str) -> std::result::Result<PathBuf, String> {
        let candidate = self.root.join(rooted_relative(arg)?);
        let canonical = std::fs::canonicalize(&candidate).map_err(|e| format!("{arg}: {e}"))?;
        self.contained(canonical, arg)
    }

    /// The file at `arg`, opened relative to the root handle.
    ///
    /// These three methods exist so that no caller ever holds a resolved path.
    /// [`rooted_relative`] runs first — it keeps the absolute-path and
    /// empty-path refusals, which the handle walk would report less usefully —
    /// and then exactly one handle-relative call does both the containment and
    /// the open.
    pub fn open_file(&self, arg: &str) -> std::result::Result<cap_std::fs::File, String> {
        let rel = rooted_relative(arg)?;
        self.dir.open(&rel).map_err(|e| self.explain(arg, e))
    }

    /// The file at `arg`, truncated if it exists and created if it does not.
    ///
    /// The **leaf** is walked like every other component, which is the hole
    /// this replaces: the old parent-canonicalize-then-append followed a
    /// symlink at the leaf straight out of the root, with no race involved.
    pub fn create_file(&self, arg: &str) -> std::result::Result<cap_std::fs::File, String> {
        let rel = rooted_relative(arg)?;
        self.dir
            .open_with(
                &rel,
                OpenOptions::new().write(true).create(true).truncate(true),
            )
            .map_err(|e| self.explain(arg, e))
    }

    /// The directory at `arg`, opened for iteration.
    pub fn read_dir(&self, arg: &str) -> std::result::Result<cap_std::fs::ReadDir, String> {
        let rel = rooted_relative(arg)?;
        self.dir.read_dir(&rel).map_err(|e| self.explain(arg, e))
    }

    /// A refusal in the model's terms, and the one place that decides whether
    /// an `io::Error` means *escape* or merely *denied*.
    ///
    /// `cap-primitives` reports an escape as a `PermissionDenied` it built
    /// itself, carrying **no** raw OS error; a genuine access denial from the
    /// operating system carries one. Mislabelling the second as the first would
    /// tell a model its path left the root when it did not. That discriminator
    /// is a dependency's internal detail, so a test pins both arms.
    fn explain(&self, arg: &str, e: std::io::Error) -> String {
        if e.kind() == ErrorKind::PermissionDenied && e.raw_os_error().is_none() {
            format!(
                "{arg} resolves outside the root {} and is refused",
                self.root.display()
            )
        } else {
            format!("{arg}: {e}")
        }
    }

    fn contained(&self, canonical: PathBuf, arg: &str) -> std::result::Result<PathBuf, String> {
        if canonical.starts_with(&self.root) {
            Ok(canonical)
        } else {
            Err(format!(
                "{arg} resolves outside the root {} and is refused",
                self.root.display()
            ))
        }
    }
}

/// The argument as a path that can safely be joined onto a root.
///
/// **`Path::join` with an absolute path discards the base** — `root.join("/etc/
/// passwd")` is `/etc/passwd` — so an argument carrying a root or a drive prefix
/// has to be refused *before* the join, not after. Afterwards there is nothing
/// left to notice: the joined path is a legitimate path to somewhere else, and
/// canonicalizing it succeeds.
///
/// Rejecting on [`Component::Prefix`] as well as [`Component::RootDir`] is what
/// makes the rule hold on Windows, where `C:foo` is drive-relative rather than
/// absolute and `\\?\` and UNC paths are prefixes of their own. `is_absolute`
/// alone would let all three through.
fn rooted_relative(arg: &str) -> std::result::Result<PathBuf, String> {
    let path = Path::new(arg);
    for component in path.components() {
        if matches!(component, Component::Prefix(_) | Component::RootDir) {
            return Err(format!(
                "{arg} is an absolute path; name a path relative to the root"
            ));
        }
    }
    if path.components().next().is_none() {
        return Err("an empty path names no file".to_string());
    }
    Ok(path.to_path_buf())
}

/// The directories an operator named with `--run-dir`, canonicalized and
/// deduplicated.
///
/// [`FsRoot::new`]'s shape and its recorded reason, applied to a list: an
/// operator who mistyped a directory wants to hear about it before a model
/// does. Canonicalizing here means every later comparison and every Win32 call
/// sees one spelling, exactly as [`FsRoot`] does.
///
/// It lives here rather than beside the rule that *uses* it because that rule
/// is `#[cfg(windows)]` and this type must exist on all three platforms —
/// [`crate::RunAccess`], which carries one, already does for the reason its own
/// docstring gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunDirs(Vec<PathBuf>);

impl RunDirs {
    /// The empty allowlist: `--allow-run` with no `--run-dir`, which is
    /// everything slice 019 shipped.
    pub fn none() -> RunDirs {
        RunDirs(Vec::new())
    }

    /// Canonicalized once, here, for [`FsRoot::new`]'s reason.
    ///
    /// Duplicates are dropped **after** canonicalization, so two spellings of
    /// one directory neither write its ACL twice nor put it on the child's
    /// `PATH` twice. Order is otherwise the operator's, and that is observable:
    /// a tie between two run directories goes to the first named.
    pub fn new(paths: &[PathBuf]) -> Result<RunDirs> {
        let mut dirs: Vec<PathBuf> = Vec::with_capacity(paths.len());
        for path in paths {
            let dir = std::fs::canonicalize(path)
                .map_err(|e| HeddleError::Tool(format!("run dir {}: {e}", path.display())))?;
            if !dir.is_dir() {
                return Err(HeddleError::Tool(format!(
                    "run dir {} is not a directory",
                    path.display()
                )));
            }
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
        Ok(RunDirs(dirs))
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.0
    }
}
