//! Acceptance tests for the Code view's two commands (spec 041).
//!
//! Same shape as `chat_session.rs`: a real `TempDir`, real files, and **no
//! window** — `code.rs` names no Tauri type, which is what lets these call the
//! functions directly instead of through `invoke`. There is no double for the
//! filesystem here and there deliberately is none: the claim under test is that
//! the Code view lists and reads what is really on disk under the session's
//! `--fs-root`, and a mock filesystem could only ever prove that a mock works.
//!
//! Written before `ui/src-tauri/src/code.rs` existed (Constitution III).

use heddle_connectors::FsRoot;
use heddle_ui::code::{list_directory, read_file, Entry, NO_FS_ROOT};
use std::fs;
use tempfile::TempDir;

/// A root with a small, deliberately unsorted tree in it.
fn tree() -> TempDir {
    let dir = TempDir::new().expect("a temp root");
    fs::write(dir.path().join("zeta.txt"), "the last file").expect("a file");
    fs::write(dir.path().join("main.rs"), "fn main() {}\n").expect("a file");
    fs::create_dir(dir.path().join("src")).expect("a directory");
    fs::write(dir.path().join("src").join("lib.rs"), "pub mod code;\n").expect("a file");
    dir
}

fn root(dir: &TempDir) -> FsRoot {
    FsRoot::new(dir.path()).expect("the temp root opens")
}

fn names(entries: &[Entry]) -> Vec<String> {
    entries.iter().map(|entry| entry.name.clone()).collect()
}

#[test]
fn the_root_listing_is_the_real_directory_with_directories_first_and_then_names() {
    let dir = tree();
    let root = root(&dir);
    let entries = list_directory(Some(&root), ".").expect("the root lists");

    // Directories first, then files, each group by name: the same "reads the
    // same way twice" rule `fs_list` sorts for, so a redraw is not a shuffle.
    assert_eq!(names(&entries), vec!["src", "main.rs", "zeta.txt"]);
    assert!(entries[0].directory, "src is a directory");
    assert!(!entries[1].directory, "main.rs is a file");
}

#[test]
fn an_empty_path_means_the_root_rather_than_a_refusal() {
    let dir = tree();
    let root = root(&dir);
    // The window's initial state has no directory selected yet; that must be
    // "show me the root", not the "an empty path names no file" refusal
    // `FsRoot` gives a model that supplied nothing.
    assert_eq!(
        names(&list_directory(Some(&root), "").expect("an empty path lists the root")),
        names(&list_directory(Some(&root), ".").expect("the root lists"))
    );
}

#[test]
fn a_subdirectory_lists_its_own_real_children() {
    let dir = tree();
    let root = root(&dir);
    let entries = list_directory(Some(&root), "src").expect("the subdirectory lists");
    assert_eq!(names(&entries), vec!["lib.rs"]);
}

#[test]
fn an_empty_directory_lists_as_empty_and_is_not_an_error() {
    let dir = tree();
    fs::create_dir(dir.path().join("empty")).expect("a directory");
    let root = root(&dir);
    assert_eq!(list_directory(Some(&root), "empty"), Ok(Vec::new()));
}

#[test]
fn a_selected_file_shows_its_real_content() {
    let dir = tree();
    let root = root(&dir);
    assert_eq!(
        read_file(Some(&root), "src/lib.rs"),
        Ok("pub mod code;\n".to_string())
    );
}

#[test]
fn a_path_that_escapes_the_root_is_refused_and_says_so() {
    let dir = tree();
    let root = root(&dir);
    let error = read_file(Some(&root), "../secrets.txt")
        .expect_err("a path leaving the root must be refused");
    // `FsRoot`'s own wording, not a second one invented here: the containment
    // decision belongs to the primitive the CLI's tools already resolve through.
    assert!(
        error.contains("outside the root") || error.contains("refused"),
        "the refusal must say the path left the root, got {error:?}"
    );

    let listing = list_directory(Some(&root), "..")
        .expect_err("a listing leaving the root must be refused too");
    assert!(
        listing.contains("outside the root") || listing.contains("refused"),
        "got {listing:?}"
    );
}

#[test]
fn an_absolute_path_is_refused_before_it_is_joined_onto_the_root() {
    let dir = tree();
    let root = root(&dir);
    let error =
        read_file(Some(&root), "/etc/passwd").expect_err("an absolute path must be refused");
    assert!(error.contains("absolute"), "got {error:?}");
}

#[test]
fn no_fs_root_is_reported_as_such_and_not_as_an_empty_directory() {
    // The session was launched without `--fs-root`, so it has no tools at all
    // (`crates/heddle-cli/src/wiring.rs`'s "no root, no tools"). An empty list
    // here would be the window claiming an empty directory it never looked at.
    assert_eq!(list_directory(None, "."), Err(NO_FS_ROOT.to_string()));
    assert_eq!(read_file(None, "main.rs"), Err(NO_FS_ROOT.to_string()));
}

#[test]
fn a_file_that_is_not_utf_8_text_is_refused_with_a_reason_rather_than_decoded_as_garbage() {
    let dir = tree();
    fs::write(dir.path().join("icon.bin"), [0xff, 0xfe, 0x00, 0x9c]).expect("a binary file");
    let root = root(&dir);
    let error = read_file(Some(&root), "icon.bin").expect_err("binary content must be refused");
    assert!(
        error.contains("UTF-8"),
        "the refusal must name why the file cannot be shown, got {error:?}"
    );
}

#[test]
fn a_file_over_the_read_cap_is_refused_on_the_same_terms_fs_read_refuses_it() {
    let dir = tree();
    let oversize = heddle_connectors::READ_BYTE_CAP + 1;
    fs::write(dir.path().join("big.txt"), "x".repeat(oversize)).expect("a large file");
    let root = root(&dir);
    let error = read_file(Some(&root), "big.txt").expect_err("an oversize file must be refused");
    assert!(
        error.contains("cap"),
        "the refusal must name the cap, got {error:?}"
    );
}

#[test]
fn reading_a_directory_is_a_refusal_and_not_a_panic() {
    let dir = tree();
    let root = root(&dir);
    assert!(
        read_file(Some(&root), "src").is_err(),
        "a directory is not a file to show"
    );
}
