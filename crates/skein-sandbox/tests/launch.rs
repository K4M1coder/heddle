//! The launcher walking skeleton (spec 019 SC-002).
//!
//! One test, and it proves four things at once: that a System32 binary is
//! launchable inside an AppContainer at all, that the child can traverse into a
//! temp-dir root, that the ACE `Sandbox::create` granted is what let it read a
//! file there, and that both pipes come back drained.
//!
//! It is deliberately the **earliest** behavioural step in the slice for that
//! reason — if the DACL model is wrong, this is where it shows, before anything
//! is built on top of it.
#![cfg(windows)]

use skein_sandbox::Sandbox;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

fn system32(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").expect("Windows names its own root"))
        .join("System32")
        .join(name)
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| v.to_string()).collect()
}

#[test]
fn a_sandboxed_process_reads_a_file_in_its_granted_root() {
    let dir = TempDir::new().expect("a temp dir");
    std::fs::write(
        dir.path().join("hello.txt"),
        "read from inside the container",
    )
    .expect("a file in the root");
    let sandbox = Sandbox::create(dir.path()).expect("the profile is created and the root granted");

    let run = sandbox
        .run(
            &system32("cmd.exe"),
            &args(&["/c", "type", "hello.txt"]),
            16 * 1024,
            Duration::from_secs(30),
        )
        .expect("a System32 binary launches inside the container");

    assert_eq!(
        run.exit_code, 0,
        "`type` on a readable file exits 0; stderr was {:?}",
        run.stderr.text
    );
    assert!(
        run.stdout.text.contains("read from inside the container"),
        "the file's real bytes must come back, got {:?} / stderr {:?}",
        run.stdout.text,
        run.stderr.text
    );
    assert_eq!(
        run.stdout.dropped_bytes, 0,
        "nothing this small may be dropped"
    );
}

/// The working directory is the root and nothing else: `RunParams` has no `cwd`
/// precisely so there is no second answer to this question.
#[test]
fn a_sandboxed_process_starts_in_its_root() {
    let dir = TempDir::new().expect("a temp dir");
    let sandbox = Sandbox::create(dir.path()).expect("the profile and the grant");

    let run = sandbox
        .run(
            &system32("cmd.exe"),
            &args(&["/c", "cd"]),
            16 * 1024,
            Duration::from_secs(30),
        )
        .expect("the launch succeeds");

    // `cd` with no argument prints the current directory. Compared against the
    // canonical root with the verbatim prefix stripped, which is the form the
    // child was given.
    let expected = dir
        .path()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string();
    assert_eq!(run.stdout.text.trim(), expected, "{run:?}");
}
