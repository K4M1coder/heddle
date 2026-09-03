//! One operator-named directory, and the rule that nothing leaves it.
//!
//! **Scope honesty:** this is *one* root with no traversal outside it. It is
//! not design §5.5's scope-owner hierarchy — `AccessScope::{Project, Folder,
//! FullComputer}` does not exist in this workspace — and it must not be read as
//! that hierarchy having landed.

use skein_core::{Result, SkeinError};
use std::path::{Component, Path, PathBuf};

/// A canonicalized directory, and the only way a tool names a file.
///
/// Every path a model asks for is resolved through this type, and a resolution
/// either lands inside the root or is refused. The refusal is a `String`
/// because it is told to the model: the far end turns it into a tool-level
/// error rather than a transport failure (see [`crate::EmbeddedServer`]).
///
/// **Residual, recorded rather than hidden:** there is a TOCTOU window between
/// the `canonicalize` below and the `File::open` that follows it — a symlink
/// swapped into that window escapes the root. Closing it needs `cap-std`-style
/// directory-handle-relative opens, which this slice deliberately does not add.
/// This is the same species of residual `skein_gateway::LocalEndpoint::parse`
/// records about `ureq` re-resolving DNS after its loopback check: a policy
/// layer above a boundary the process does not yet have.
#[derive(Debug)]
pub struct FsRoot {
    root: PathBuf,
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
            .map_err(|e| SkeinError::Tool(format!("fs root {}: {e}", path.display())))?;
        if !root.is_dir() {
            return Err(SkeinError::Tool(format!(
                "fs root {} is not a directory",
                path.display()
            )));
        }
        Ok(FsRoot { root })
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    /// An existing path under the root, canonicalized.
    ///
    /// Canonicalizing before the prefix check is what closes `..` traversal
    /// **and symlink escape** in one step: a link inside the root pointing
    /// outside it canonicalizes to its target, which fails the check.
    pub fn resolve(&self, arg: &str) -> std::result::Result<PathBuf, String> {
        let candidate = self.root.join(rooted_relative(arg)?);
        let canonical = std::fs::canonicalize(&candidate).map_err(|e| format!("{arg}: {e}"))?;
        self.contained(canonical, arg)
    }

    /// A path under the root that may not exist yet — `fs_write`'s case.
    ///
    /// The **parent** is canonicalized and checked, then the file name is
    /// re-appended: a path whose parent does not exist is refused rather than
    /// created, so no tool call can bring a directory tree into being.
    pub fn resolve_new(&self, arg: &str) -> std::result::Result<PathBuf, String> {
        let candidate = self.root.join(rooted_relative(arg)?);
        let name = candidate
            .file_name()
            .ok_or_else(|| format!("{arg} does not name a file"))?
            .to_owned();
        let parent = candidate
            .parent()
            .ok_or_else(|| format!("{arg} has no parent directory"))?;
        let canonical_parent = std::fs::canonicalize(parent).map_err(|e| format!("{arg}: {e}"))?;
        Ok(self.contained(canonical_parent, arg)?.join(name))
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
