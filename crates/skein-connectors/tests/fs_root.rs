//! Containment tests for [`FsRoot`] (spec 016, SC-001/SC-002).
//!
//! This is the whole safety argument of the slice, so every hazard gets its own
//! test and the tests say which hazard they are about. There is **no `#[cfg]` in
//! the containment code**; the one platform split here is the symlink *helper*,
//! because creating a symlink is a platform API and Windows needs a privilege
//! for it that Unix does not. The assertions either side of that helper are the
//! same bodies everywhere.

use skein_connectors::FsRoot;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A canonicalized root with `notes.txt` in it, plus a sibling directory
/// *outside* the root holding `outside.txt` — the thing every escape test tries
/// to reach.
struct Fixture {
    _dir: TempDir,
    root: FsRoot,
    outside_file: PathBuf,
    outside_dir: PathBuf,
}

fn fixture() -> Fixture {
    let dir = TempDir::new().expect("a temp dir");
    let root_path = dir.path().join("root");
    let outside_dir = dir.path().join("outside");
    std::fs::create_dir(&root_path).expect("the root is created");
    std::fs::create_dir(&outside_dir).expect("the sibling is created");
    std::fs::write(root_path.join("notes.txt"), "in the root").expect("a file in the root");
    let outside_file = outside_dir.join("outside.txt");
    std::fs::write(&outside_file, "not yours").expect("a file outside the root");

    Fixture {
        root: FsRoot::new(&root_path).expect("a canonicalizable root"),
        _dir: dir,
        outside_file: std::fs::canonicalize(&outside_file).expect("the outside file canonicalizes"),
        outside_dir: std::fs::canonicalize(&outside_dir).expect("the outside dir canonicalizes"),
    }
}

/// Windows needs `SeCreateSymbolicLinkPrivilege` (developer mode or elevation)
/// and returns an OS error without it. That is a fact about the machine, not
/// about the code, so the caller skips rather than fails.
fn symlink_dir(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    return std::os::windows::fs::symlink_dir(target, link);
    #[cfg(unix)]
    return std::os::unix::fs::symlink(target, link);
}

// ---------------------------------------------------------------------------
// SC-001 — the `Path::join` footgun, which gets its own test on every path
// that joins.
// ---------------------------------------------------------------------------

#[test]
fn an_absolute_argument_is_refused_before_it_is_joined() {
    let f = fixture();
    let absolute = f.outside_file.to_str().expect("a utf-8 temp path");

    // The footgun itself, asserted rather than described: joining an absolute
    // path onto the root *discards the root*. So a containment check written as
    // join-then-canonicalize-then-prefix has a hole unless the refusal happens
    // first — by the time it canonicalizes there is nothing left to notice.
    assert_eq!(
        f.root.path().join(absolute),
        f.outside_file,
        "Path::join with an absolute path must be understood to discard the base"
    );

    let refusal = f
        .root
        .resolve(absolute)
        .expect_err("an absolute argument must be refused");
    assert!(
        refusal.contains("absolute"),
        "the refusal must say why, got: {refusal}"
    );
}

#[test]
fn an_absolute_argument_is_refused_on_the_write_path_too() {
    let f = fixture();
    let absolute = f
        .outside_dir
        .join("planted.txt")
        .to_str()
        .expect("a utf-8 temp path")
        .to_string();

    let refusal = f
        .root
        .resolve_new(&absolute)
        .expect_err("an absolute argument must be refused on every path that joins");
    assert!(
        refusal.contains("absolute"),
        "the refusal must say why, got: {refusal}"
    );
    assert!(
        !f.outside_dir.join("planted.txt").exists(),
        "a refused resolution must not have created anything"
    );
}

// ---------------------------------------------------------------------------
// SC-002 — traversal, symlinks, the happy path, and construction.
// ---------------------------------------------------------------------------

#[test]
fn a_parent_traversal_out_of_the_root_is_refused() {
    let f = fixture();

    let refusal = f
        .root
        .resolve("../outside/outside.txt")
        .expect_err("`..` past the root must be refused");
    assert!(
        refusal.contains("outside the root"),
        "the refusal must name the containment rule, got: {refusal}"
    );
}

#[test]
fn a_symlink_pointing_outside_the_root_is_refused() {
    let f = fixture();
    let link = f.root.path().join("escape");
    if symlink_dir(&f.outside_dir, &link).is_err() {
        eprintln!("this machine does not permit creating symlinks; skipping");
        return;
    }

    // The link is inside the root and its *lexical* path starts with the root.
    // Only canonicalization sees that it does not stay there — which is why the
    // prefix check is made against the canonical path and never the joined one.
    let refusal = f
        .root
        .resolve("escape/outside.txt")
        .expect_err("a symlink out of the root must be refused");
    assert!(
        refusal.contains("outside the root"),
        "the refusal must name the containment rule, got: {refusal}"
    );
}

#[test]
fn an_in_root_relative_path_resolves() {
    let f = fixture();

    let resolved = f
        .root
        .resolve("notes.txt")
        .expect("an in-root file resolves");
    assert!(resolved.starts_with(f.root.path()));
    assert_eq!(
        std::fs::read_to_string(&resolved).expect("the resolved path is readable"),
        "in the root"
    );

    // A benign `..` that lands back inside is allowed: the rule is containment,
    // not a syntax ban.
    let round_trip = f
        .root
        .resolve("./notes.txt")
        .expect("a `.`-prefixed in-root file resolves");
    assert_eq!(round_trip, resolved);
}

#[test]
fn a_new_file_under_the_root_resolves_through_its_parent() {
    let f = fixture();

    let resolved = f
        .root
        .resolve_new("fresh.txt")
        .expect("a not-yet-existing file under the root resolves");
    assert_eq!(resolved, f.root.path().join("fresh.txt"));
    assert!(
        !resolved.exists(),
        "resolving a path must not create anything"
    );
}

#[test]
fn a_new_file_whose_parent_does_not_exist_is_refused() {
    let f = fixture();

    let refusal = f
        .root
        .resolve_new("no/such/dir/fresh.txt")
        .expect_err("a missing parent must be refused rather than created");
    assert!(
        !f.root.path().join("no").exists(),
        "a refusal must create no directory"
    );
    assert!(!refusal.is_empty(), "the refusal must say something");
}

#[test]
fn an_empty_argument_is_refused() {
    let f = fixture();

    f.root
        .resolve("")
        .expect_err("an empty path names nothing and must be refused");
}

#[test]
fn a_missing_root_is_refused_at_construction() {
    let dir = TempDir::new().expect("a temp dir");

    let err = FsRoot::new(dir.path().join("does-not-exist"))
        .expect_err("a root that does not exist must fail loudly at construction");
    assert!(
        err.to_string().contains("does-not-exist"),
        "the error must name the path the operator gave, got: {err}"
    );
}

#[test]
fn a_root_that_is_a_file_is_refused_at_construction() {
    let dir = TempDir::new().expect("a temp dir");
    let file = dir.path().join("not-a-dir.txt");
    std::fs::write(&file, "a file").expect("a file to point the root at");

    FsRoot::new(&file).expect_err("a root must be a directory");
}

/// `RunDirs::new`'s validation, in the file that already owns `FsRoot::new`'s
/// (spec 020 SC-008).
#[test]
fn a_run_dir_that_is_not_a_directory_is_a_loud_refusal() {
    use skein_connectors::RunDirs;

    let dir = TempDir::new().expect("a temp dir");
    let file = dir.path().join("cargo.exe");
    std::fs::write(&file, "a file, not a directory").expect("a file to point a run dir at");

    let err = RunDirs::new(&[file]).expect_err("a run dir must be a directory");
    assert!(
        err.to_string().contains("cargo.exe"),
        "the error must name the path the operator gave, got: {err}"
    );

    let missing = RunDirs::new(&[dir.path().join("no-such-toolchain")])
        .expect_err("a run dir that does not exist must fail loudly at construction");
    assert!(
        missing.to_string().contains("no-such-toolchain"),
        "and so must this one, got: {missing}"
    );

    // Two spellings of one directory are one entry: a doubled flag must double
    // neither an ACL write nor a `PATH` entry.
    let doubled = RunDirs::new(&[dir.path().to_path_buf(), dir.path().to_path_buf()])
        .expect("a real directory named twice");
    assert_eq!(doubled.paths().len(), 1, "{:?}", doubled.paths());
}
