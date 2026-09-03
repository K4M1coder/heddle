//! Containment for the git tools (spec 017, SC-001…SC-004): exactly one
//! repository is reachable, and it is the one the operator named.
//!
//! The git tools take **no path arguments whatsoever**, so containment here is
//! not `fs_root.rs`'s "resolve this path safely" — it is "open exactly one
//! repository and refuse everything else". Two of the refusals are measured
//! escapes rather than hypotheses, and each has its own test saying which
//! escape it is about.
//!
//! Every fixture is a **real repository with real commits**, built through
//! `git2` rather than by shelling out to a `git` binary: no `PATH` assumption,
//! deterministic on all three OSes, and still real objects on disk.
//!
//! There is **no `#[cfg]`** here, and none in the containment code: the check
//! compares two canonicalized paths, which on Windows means both sides are
//! `\\?\`-verbatim.

use git2::{Repository, Signature};
use skein_connectors::{is_git_repository, FsRoot};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A repository at `path` with one commit in it, so no test is accidentally
/// asserting against an unborn branch when it means to assert containment.
fn repository_with_a_commit(path: &Path) -> Repository {
    let repo = Repository::init(path).expect("a repository is initialised");
    std::fs::write(path.join("tracked.txt"), "committed contents").expect("a file to commit");
    commit(&repo, "tracked.txt", "the parent repository's own commit");
    repo
}

/// Stages one path and commits it onto `HEAD`.
fn commit(repo: &Repository, path: &str, message: &str) {
    let mut index = repo.index().expect("the index opens");
    index.add_path(Path::new(path)).expect("the path is staged");
    index.write().expect("the index is written");
    let tree = repo
        .find_tree(index.write_tree().expect("the index writes a tree"))
        .expect("the tree is found");
    let who = Signature::now("Fixture Author", "fixture@example.invalid").expect("a signature");
    let parents: Vec<git2::Commit> = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok())
        .into_iter()
        .collect();
    repo.commit(
        Some("HEAD"),
        &who,
        &who,
        message,
        &tree,
        &parents.iter().collect::<Vec<_>>(),
    )
    .expect("the commit is written");
}

fn root_of(path: &Path) -> FsRoot {
    FsRoot::new(path).expect("a canonicalizable root")
}

// ---------------------------------------------------------------------------
// The happy path, and the plain refusals.
// ---------------------------------------------------------------------------

#[test]
fn a_repository_at_the_root_is_the_one_case_that_opens() {
    let dir = TempDir::new().expect("a temp dir");
    let repo = repository_with_a_commit(dir.path());

    assert!(
        is_git_repository(&root_of(dir.path())),
        "a repository whose worktree *is* the root is the only thing this connector serves"
    );
    // The property the containment check relies on, asserted rather than
    // assumed: an ordinary repository's worktree is the directory it was
    // opened at, so comparing the two costs nothing in the normal case.
    assert_eq!(
        std::fs::canonicalize(
            repo.workdir()
                .expect("a non-bare repository has a worktree")
        )
        .expect("the worktree canonicalizes"),
        std::fs::canonicalize(dir.path()).expect("the root canonicalizes")
    );
}

#[test]
fn a_directory_that_is_not_a_repository_is_refused() {
    let dir = TempDir::new().expect("a temp dir");
    std::fs::write(dir.path().join("notes.txt"), "just a directory").expect("a plain file");

    assert!(
        !is_git_repository(&root_of(dir.path())),
        "a plain directory has no repository to report on"
    );
}

#[test]
fn a_bare_repository_is_refused() {
    let dir = TempDir::new().expect("a temp dir");
    Repository::init_bare(dir.path()).expect("a bare repository is initialised");

    // A bare repository has no worktree at all, so `git_status` would be
    // meaningless and `workdir()` is `None`. Refused explicitly rather than
    // left to fall out of an `unwrap` somewhere.
    assert!(
        !is_git_repository(&root_of(dir.path())),
        "a bare repository has no working tree to report on"
    );
}

// ---------------------------------------------------------------------------
// SC-001 — the upward walk. `open` refuses it; `discover` is the hole, and
// this test asserts which of the two the guarantee depends on.
// ---------------------------------------------------------------------------

#[test]
fn a_subdirectory_of_a_repository_is_refused_rather_than_walked_up_from() {
    let dir = TempDir::new().expect("a temp dir");
    let repo_path = dir.path().join("repo");
    std::fs::create_dir(&repo_path).expect("the repository directory is created");
    repository_with_a_commit(&repo_path);
    let sub = repo_path.join("sub");
    std::fs::create_dir(&sub).expect("a subdirectory of the repository");

    assert!(
        !is_git_repository(&root_of(&sub)),
        "a root inside a repository must not become that repository"
    );

    // The footgun itself, asserted rather than described — `fs_root.rs`'s move
    // with the `Path::join` hazard. `discover` walks **up** and succeeds here,
    // reporting the *enclosing* repository's worktree, so a containment rule
    // written on `discover` would silently report on a repository the operator
    // never named. This is also why `Repository::discover`, `open_ext` and
    // `open_from_env` must never appear in this slice: if one ever does, this
    // assertion is what turns it into a failing test rather than a review miss.
    let discovered = Repository::discover(&sub)
        .expect("`discover` must be understood to walk up out of the named directory");
    assert_eq!(
        std::fs::canonicalize(discovered.workdir().expect("a worktree"))
            .expect("the worktree canonicalizes"),
        std::fs::canonicalize(&repo_path).expect("the repository path canonicalizes"),
        "`discover` reports the enclosing repository, not the directory it was given"
    );
}

// ---------------------------------------------------------------------------
// SC-002 — `core.worktree`. A repository's own config can point its worktree
// outside the root, which is why the check compares `workdir()` against the
// root rather than trusting the path passed to `open`.
// ---------------------------------------------------------------------------

/// A repository at `<tmp>/root` whose `.git/config` sets `core.worktree` to
/// `<tmp>/outside`, which holds `outside.txt`. Returns the root and that file.
fn worktree_escape(dir: &TempDir) -> (PathBuf, PathBuf) {
    let root = dir.path().join("root");
    let outside = dir.path().join("outside");
    std::fs::create_dir(&root).expect("the root is created");
    std::fs::create_dir(&outside).expect("the sibling outside the root is created");
    let outside_file = outside.join("outside.txt");
    std::fs::write(&outside_file, "not yours").expect("a file outside the root");

    let repo = repository_with_a_commit(&root);
    // Git's config takes forward slashes on every OS, so a Windows path is
    // written the way `git config core.worktree` would write it.
    let target = std::fs::canonicalize(&outside)
        .expect("the sibling canonicalizes")
        .to_string_lossy()
        .replace('\\', "/");
    repo.config()
        .expect("the repository config opens")
        .set_str("core.worktree", target.trim_start_matches("//?/"))
        .expect("core.worktree is set");

    (root, outside_file)
}

#[test]
fn a_repository_whose_config_points_its_worktree_outside_the_root_is_refused() {
    let dir = TempDir::new().expect("a temp dir");
    let (root, outside_file) = worktree_escape(&dir);

    // First: the escape is real, and the test proves it rather than trusting
    // the plan that measured it. The repository opens at the root the operator
    // named, reports a worktree somewhere else entirely, and `statuses()` lists
    // a file outside the root. Without the `workdir()` comparison this is what
    // `git_status` would have reported on.
    let escaped = Repository::open(&root).expect("the repository still opens at the named root");
    let workdir = std::fs::canonicalize(escaped.workdir().expect("a worktree"))
        .expect("the worktree canonicalizes");
    assert_ne!(
        workdir,
        std::fs::canonicalize(&root).expect("the root canonicalizes"),
        "core.worktree must be understood to move the worktree out of the named directory"
    );
    assert!(
        escaped
            .statuses(None)
            .expect("statuses are readable")
            .iter()
            .any(|e| e.path().as_deref() == Ok("outside.txt")),
        "the escape must really reach a file outside the root, or this test proves nothing"
    );

    // Then: refused.
    assert!(
        !is_git_repository(&root_of(&root)),
        "a repository whose worktree is not the root must be refused"
    );
    assert!(
        outside_file.exists(),
        "sanity: the out-of-root file is still there to have been leaked"
    );
}
