//! Reading one git repository, and the rule that it is the one the operator
//! named.
//!
//! **This is the only module in the workspace that names `git2`.** The boundary
//! is deliberate: `gix` was rejected on shipped footprint (112 packages for
//! `status` + `revision`, against `git2`'s four) rather than on quality, and if
//! that footprint ever stops mattering the swap touches this file and nothing
//! else. `skein-cli` asks [`is_git_repository`] and never sees a `git2` type.
//!
//! **No subprocess is spawned here, and there is no argument vector anywhere.**
//! That is not tidiness. A fixed `git` argv would still not have been safe: a
//! target repository's own `.git/config` can name `core.fsmonitor`, which
//! `git status` **executes** — measured — and which libgit2 reads and does not
//! run. Nor would it have been contained: `git -C <dir>` discovers upward, so a
//! root inside a repository silently reports on the enclosing one.

use crate::fs::FsRoot;
use git2::Repository;

/// The one door into a repository. Every git tool starts here.
///
/// `Repository::open` and **never** `discover`, `open_ext` or
/// `open_from_env`: `open` refuses a directory that is not itself a repository,
/// where the other three walk up the tree and would silently open a repository
/// the operator never named. `tests/git_root.rs` asserts `discover`'s
/// contrasting behaviour explicitly, so a helpful refactor to it becomes a
/// failing test rather than a review miss.
///
/// The worktree comparison at the end is not defensive padding — it closes a
/// measured escape. A repository's own `.git/config` may set `core.worktree`,
/// and such a repository opens fine at the named root while reporting, and
/// reporting *on*, a worktree somewhere else entirely. Comparing
/// `repo.workdir()` against the root is what refuses it; comparing the path
/// passed to `open` would not have noticed. Both sides are canonicalized, so on
/// Windows the comparison is verbatim against verbatim — the same reasoning
/// [`FsRoot::new`]'s docstring records.
///
/// Every refusal is an `Err(String)`, which rmcp turns into `isError: true`: a
/// tool error the model is told about and can act on, never a transport failure
/// that ends the run.
///
/// **Opened per call, never held in the server.** `git2::Repository` is not
/// `Sync` and rmcp's handler must be `Clone + Send + Sync + 'static`, so
/// opening per call sidesteps that entirely — and it re-verifies containment on
/// every call instead of caching a handle across a configuration change.
///
/// Slice 016's TOCTOU residual is inherited unchanged: a directory swapped
/// between `FsRoot::new`'s `canonicalize` and the `open` below escapes the root.
pub(crate) fn open_contained(root: &FsRoot) -> std::result::Result<Repository, String> {
    let repo = Repository::open(root.path()).map_err(|e| {
        format!(
            "{} is not a git repository: {}",
            root.path().display(),
            e.message()
        )
    })?;
    // `workdir()` is `None` for a bare repository and only for one, so this is
    // the bare refusal rather than a second check next to it.
    let Some(workdir) = repo.workdir() else {
        return Err(format!(
            "{} is a bare repository and has no working tree to report on",
            root.path().display()
        ));
    };
    let workdir = std::fs::canonicalize(workdir).map_err(|e| {
        format!(
            "{}: its working tree does not resolve: {e}",
            root.path().display()
        )
    })?;
    if workdir != root.path() {
        return Err(format!(
            "{} is a git repository whose working tree is {} — outside the configured root — and \
             is refused",
            root.path().display(),
            workdir.display()
        ));
    }
    Ok(repo)
}

/// True when [`open_contained`] succeeds, and nothing more.
///
/// Public because two gates need it and **neither may see `git2`**:
/// [`crate::EmbeddedServer::new`] disables the git routes when it is false, and
/// `skein-cli`'s `wiring::ToolArgs` omits the git names from its allowlist in
/// the same case. Both gates are required — an allowlisted name whose route is
/// disabled reaches the model as a transport error, which ends the run.
///
/// Delegating rather than re-deriving is the point: the two gates and the tools
/// cannot disagree about what a servable repository is, because there is one
/// answer and this is it.
pub fn is_git_repository(root: &FsRoot) -> bool {
    open_contained(root).is_ok()
}
