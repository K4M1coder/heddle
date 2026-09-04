//! Containment tests for [`FsRoot`] (spec 016, SC-001/SC-002).
//!
//! This is the whole safety argument of the slice, so every hazard gets its own
//! test and the tests say which hazard they are about. There is **no `#[cfg]` in
//! the containment code**; the one platform split here is the symlink *helper*,
//! because creating a symlink is a platform API and Windows needs a privilege
//! for it that Unix does not. The assertions either side of that helper are the
//! same bodies everywhere.

use skein_connectors::FsRoot;
use std::io::Read;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A canonicalized root with `notes.txt` in it, plus a sibling directory
/// *outside* the root holding `outside.txt` — the thing every escape test tries
/// to reach.
struct Fixture {
    root: FsRoot,
    outside_file: PathBuf,
    outside_dir: PathBuf,
    /// Declared **last**: struct fields drop in declaration order, so a
    /// `TempDir` declared first is removed while `root`'s directory handle is
    /// still open. `TempDir::drop` ignores removal failure, so the only symptom
    /// would be a temp directory leaked on every run.
    _dir: TempDir,
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

/// A reparse point at `link` leading to the **directory** `target`.
///
/// A junction on Windows rather than a symlink, because a junction needs no
/// privilege and `symlink_dir` needs `SeCreateSymbolicLinkPrivilege` — which
/// this project's own developer machines do not have, so every symlink test
/// written against it has silently skipped since slice 016.
/// `std::os::windows::fs::junction_point` would be the direct route and is
/// nightly-only, so `mklink /J` it is.
fn reparse_dir(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let ok = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?
            .success();
        ok.then_some(())
            .ok_or_else(|| std::io::Error::other("mklink /J refused"))
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link)
}

/// A reparse point at `link` leading to the **file** `target`.
///
/// There is no privilege-free equivalent of [`reparse_dir`] for a file — a
/// junction only names a directory — so on Windows this needs the privilege
/// and the caller skips when it is refused. That is a fact about the machine,
/// not about the code.
fn reparse_file(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    return std::os::windows::fs::symlink_file(target, link);
    #[cfg(unix)]
    return std::os::unix::fs::symlink(target, link);
}

/// Read a file the way `fs_read` does — through the root's own handle rather
/// than through `std::fs` on a path it handed back. A test that read by path
/// would be testing the operating system, not the containment walk.
fn read(root: &FsRoot, arg: &str) -> Result<String, String> {
    let mut file = root.open_file(arg)?;
    let mut contents = String::new();
    Read::read_to_string(&mut file, &mut contents).map_err(|e| e.to_string())?;
    Ok(contents)
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
        .create_file(&absolute)
        .expect_err("an absolute argument must be refused on every path that opens");
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
    reparse_dir(&f.outside_dir, &link).expect("a junction needs no privilege");

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
fn a_new_file_under_the_root_is_created_where_it_was_named() {
    let f = fixture();

    f.root
        .create_file("fresh.txt")
        .expect("a not-yet-existing file under the root is created");

    assert!(
        f.root.path().join("fresh.txt").is_file(),
        "the file must appear at the name the caller gave, under the root"
    );
}

#[test]
fn a_new_file_whose_parent_does_not_exist_is_refused() {
    let f = fixture();

    let refusal = f
        .root
        .create_file("no/such/dir/fresh.txt")
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

// ---------------------------------------------------------------------------
// Spec 021 — the root is a *directory*, not a name that a directory currently
// answers to.
// ---------------------------------------------------------------------------

/// The two outcomes are the two platforms' honest behaviour, asserted rather
/// than `#[cfg]`-selected: holding a directory handle open makes the directory
/// unrenameable on Windows and merely detaches the name from the inode on Unix.
/// Both are containment; neither is "whatever is at that name now".
#[test]
fn an_impostor_at_the_roots_name_is_not_the_root() {
    let dir = TempDir::new().expect("a temp dir");
    let root_path = dir.path().join("root");
    std::fs::create_dir(&root_path).expect("the root is created");
    std::fs::write(root_path.join("notes.txt"), "in the root").expect("a file in the root");

    let root = FsRoot::new(&root_path).expect("a canonicalizable root");

    match std::fs::rename(&root_path, dir.path().join("moved")) {
        Err(_) => {
            // The held handle refused the swap outright, so there is no
            // impostor to plant. The root is still the root.
            assert_eq!(
                read(&root, "notes.txt").expect("the real root still reads"),
                "in the root"
            );
        }
        Ok(()) => {
            std::fs::create_dir(&root_path).expect("an impostor at the vacated name");
            std::fs::write(root_path.join("impostor.txt"), "planted")
                .expect("a file the operator never named");

            read(&root, "impostor.txt")
                .expect_err("a file planted at the root's vacated name must not be reachable");
            assert_eq!(
                read(&root, "notes.txt").expect("the handle followed the directory, not the name"),
                "in the root"
            );
        }
    }
}

/// `resolve_new` canonicalized the **parent** and re-appended the leaf name
/// untouched, so a pre-existing symlink at the leaf was written straight
/// through to its target. No timing was involved: this was a hole, not a race.
#[test]
fn a_symlink_leaf_on_the_write_path_does_not_reach_its_target() {
    let f = fixture();
    if reparse_file(&f.outside_file, &f.root.path().join("link.txt")).is_err() {
        eprintln!("this machine does not permit creating file symlinks; skipping");
        return;
    }

    f.root
        .create_file("link.txt")
        .expect_err("a leaf that leads outside the root must be refused, not followed");
    assert_eq!(
        std::fs::read_to_string(&f.outside_file).expect("the outside file is still there"),
        "not yours",
        "even opening the leaf for writing must not have truncated its target"
    );
}

/// The read-side mirror of the test above, and the same hole: the leaf is the
/// one component a parent-canonicalize-then-append walk never checks, so it
/// has to be walked like every other one on **every** operation `FsRoot`
/// offers, not only on the writes that first exposed it.
#[test]
fn a_symlink_leaf_on_the_read_path_does_not_reach_its_target() {
    let f = fixture();
    let link = f.root.path().join("link.txt");
    if reparse_file(&f.outside_file, &link).is_err() {
        eprintln!("this machine does not permit creating file symlinks; skipping");
        return;
    }

    assert_eq!(
        std::fs::read_to_string(&link).expect("the symlink really escapes"),
        "not yours",
        "positive control: without containment this leaf reads the outside file"
    );

    read(&f.root, "link.txt")
        .expect_err("a leaf that leads outside the root must be refused, not followed");
}

/// The mechanism itself: a directory swapped for a reparse point **after** the
/// root was constructed is refused by the handle walk.
///
/// The unsandboxed read is the point of the test rather than decoration — it
/// proves the swap is a real escape, so the three refusals below are a
/// guarantee and not a tautology about a path that never worked.
#[test]
fn a_reparse_point_swapped_under_the_root_is_refused() {
    let dir = TempDir::new().expect("a temp dir");
    let root_path = dir.path().join("root");
    let outside_dir = dir.path().join("outside");
    std::fs::create_dir(&root_path).expect("the root is created");
    std::fs::create_dir(root_path.join("sub")).expect("a real subdirectory");
    std::fs::create_dir(&outside_dir).expect("the sibling is created");
    std::fs::write(root_path.join("sub").join("deep.txt"), "deep").expect("a file one level down");
    std::fs::write(outside_dir.join("secret.txt"), "not yours").expect("a file outside the root");

    let root = FsRoot::new(&root_path).expect("a canonicalizable root");

    let swapped = root_path.join("sub");
    std::fs::remove_dir_all(&swapped).expect("the real subdirectory goes");
    reparse_dir(&outside_dir, &swapped).expect("a junction needs no privilege");

    assert_eq!(
        std::fs::read_to_string(swapped.join("secret.txt")).expect("the swap really escapes"),
        "not yours",
        "positive control: without containment this path reads the outside file"
    );

    let refusal = read(&root, "sub/secret.txt").expect_err("a read through the swap is refused");
    assert!(refusal.contains("outside the root"), "{refusal}");
    let refusal = root
        .read_dir("sub")
        .expect_err("a listing through the swap is refused");
    assert!(refusal.contains("outside the root"), "{refusal}");
    let refusal = root
        .create_file("sub/planted.txt")
        .expect_err("a write through the swap is refused");
    assert!(refusal.contains("outside the root"), "{refusal}");
    assert!(
        !outside_dir.join("planted.txt").exists(),
        "a refused write must have planted nothing outside the root"
    );
}

/// `explain`'s two arms, pinned.
///
/// The refusal a model is told depends on `cap-primitives` reporting an escape
/// as a `PermissionDenied` carrying **no** raw OS error, where the operating
/// system's own denial carries one. That is a dependency's internal detail, so
/// a change to it must surface here as a failing assertion rather than as a
/// silently wrong refusal message.
#[test]
fn an_escape_is_named_as_one_and_a_real_denial_is_not() {
    let f = fixture();

    let escape = read(&f.root, "../outside/outside.txt").expect_err("an escape is refused");
    assert!(
        escape.contains("outside the root"),
        "an escape must be named as one, got: {escape}"
    );

    // Opening a directory as a file is `PermissionDenied` too, but with a raw
    // OS error behind it. Reported as itself, never as an escape.
    std::fs::create_dir(f.root.path().join("sub")).expect("a real subdirectory");
    let denial = read(&f.root, "sub").expect_err("a directory does not read as a file");
    assert!(
        !denial.contains("outside the root"),
        "a real access denial must not be dressed up as an escape, got: {denial}"
    );
}
