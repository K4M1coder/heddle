//! The three `fs` tools, exercised as the server's own methods (spec 016,
//! SC-003).
//!
//! These call the `#[tool]` methods directly, which is the level that can show
//! a *tool-level* refusal as an `Err(String)` before rmcp turns it into a
//! `CallToolResult { is_error: true }`. The end-to-end proof that it really
//! arrives that way is `governed_fs_run.rs`.

use rmcp::handler::server::wrapper::Parameters;
use skein_connectors::{
    EmbeddedServer, FsRoot, ListParams, ReadParams, WriteParams, READ_BYTE_CAP,
};
use tempfile::TempDir;

struct Fixture {
    _dir: TempDir,
    outside: String,
    server: EmbeddedServer,
}

fn fixture() -> Fixture {
    let dir = TempDir::new().expect("a temp dir");
    let root_path = dir.path().join("root");
    std::fs::create_dir(&root_path).expect("the root is created");
    std::fs::create_dir(root_path.join("sub")).expect("a subdirectory");
    std::fs::write(root_path.join("notes.txt"), "first line\nsecond line")
        .expect("a file in the root");
    std::fs::write(root_path.join("sub").join("deep.txt"), "deep").expect("a file one level down");
    std::fs::write(dir.path().join("outside.txt"), "not yours").expect("a file outside the root");

    Fixture {
        server: EmbeddedServer::new(FsRoot::new(&root_path).expect("a canonicalizable root")),
        outside: dir
            .path()
            .join("outside.txt")
            .to_str()
            .expect("a utf-8 temp path")
            .to_string(),
        _dir: dir,
    }
}

fn read(server: &EmbeddedServer, path: &str) -> Result<String, String> {
    server.fs_read(Parameters(ReadParams {
        path: path.to_string(),
    }))
}

fn list(server: &EmbeddedServer, path: &str) -> Result<String, String> {
    server.fs_list(Parameters(ListParams {
        path: path.to_string(),
    }))
}

fn write(server: &EmbeddedServer, path: &str, content: &str) -> Result<String, String> {
    server.fs_write(Parameters(WriteParams {
        path: path.to_string(),
        content: content.to_string(),
    }))
}

#[test]
fn fs_read_returns_a_files_contents() {
    let f = fixture();

    assert_eq!(
        read(&f.server, "notes.txt").expect("an in-root file reads"),
        "first line\nsecond line"
    );
    assert_eq!(
        read(&f.server, "sub/deep.txt").expect("a nested in-root file reads"),
        "deep"
    );
}

#[test]
fn fs_read_refuses_a_file_over_the_byte_cap_and_names_it() {
    let f = fixture();
    let oversized = "x".repeat(READ_BYTE_CAP + 1);
    write(&f.server, "big.txt", &oversized).expect("the oversized file is written");

    let refusal = read(&f.server, "big.txt").expect_err("a file over the cap must be refused");

    assert!(
        refusal.contains(&READ_BYTE_CAP.to_string()),
        "the refusal must name the cap so the model can act on it, got: {refusal}"
    );
    // Refused, not truncated: a silently shortened file would be a wrong answer
    // wearing a right one's shape, and it would land on the chain as one.
    assert!(
        !refusal.contains(&oversized),
        "a refusal must not carry the contents it refused to return"
    );
}

#[test]
fn fs_read_refuses_a_path_outside_the_root() {
    let f = fixture();

    let refusal = read(&f.server, &f.outside).expect_err("an out-of-root read must be refused");
    assert!(refusal.contains("absolute"), "{refusal}");
    let refusal =
        read(&f.server, "../outside.txt").expect_err("an out-of-root read must be refused");
    assert!(refusal.contains("outside the root"), "{refusal}");
}

#[test]
fn fs_list_names_one_directorys_entries_and_does_not_recurse() {
    let f = fixture();

    let listing = list(&f.server, ".").expect("the root itself lists");

    assert_eq!(listing, "dir\tsub\nfile\tnotes.txt");
    assert!(
        !listing.contains("deep.txt"),
        "fs_list is non-recursive: a file one level down must not appear, got: {listing}"
    );
    assert_eq!(
        list(&f.server, "sub").expect("a subdirectory lists"),
        "file\tdeep.txt"
    );
}

#[test]
fn fs_write_replaces_a_files_contents_under_the_root() {
    let f = fixture();

    let report = write(&f.server, "notes.txt", "replaced").expect("an in-root write succeeds");

    assert!(
        report.contains("8"),
        "the report must name the byte count it wrote, got: {report}"
    );
    assert_eq!(
        read(&f.server, "notes.txt").expect("the file reads back"),
        "replaced"
    );
}

#[test]
fn fs_write_refuses_a_path_outside_the_root_and_creates_nothing() {
    let f = fixture();

    write(&f.server, &f.outside, "planted").expect_err("an absolute write target must be refused");
    write(&f.server, "../planted.txt", "planted")
        .expect_err("an out-of-root write target must be refused");

    assert_eq!(
        std::fs::read_to_string(&f.outside).expect("the outside file is still there"),
        "not yours",
        "a refused write must not have touched anything"
    );
}

#[test]
fn fs_write_refuses_a_target_whose_directory_does_not_exist() {
    let f = fixture();

    write(&f.server, "no/such/dir/fresh.txt", "planted")
        .expect_err("a missing parent directory must be refused rather than created");
}
