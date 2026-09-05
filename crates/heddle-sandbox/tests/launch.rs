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

mod guard;

use heddle_sandbox::Sandbox;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use tempfile::TempDir;

fn system32(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").expect("Windows names its own root"))
        .join("System32")
        .join(name)
}

/// Nothing in this file cancels anything; the launcher needs a flag to
/// read, and one that is never set is what "no cancel channel" looks
/// like — the same thing `heddle chat` passes.
fn uncancelled() -> AtomicBool {
    AtomicBool::new(false)
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
    let sandbox =
        Sandbox::create(dir.path(), &[]).expect("the profile is created and the root granted");
    let _pruned_sandbox = guard::PrunedOnDrop::of(&sandbox);

    let run = sandbox
        .run(
            &system32("cmd.exe"),
            &args(&["/c", "type", "hello.txt"]),
            16 * 1024,
            Duration::from_secs(30),
            &uncancelled(),
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

/// Past the cap the reader keeps reading, and this is the only test that makes
/// it prove that.
///
/// The cap is not a stopping point, it is a *keeping* point — `drain` reads to
/// EOF and discards past the cap. A reader that stopped at it instead leaves a
/// child writing into a pipe nobody is emptying, and the child is then at the
/// mercy of whether the read end is still open: held open it blocks in
/// `WriteFile` until the wall clock kills it, and closed — which is what a
/// reader thread returning early actually does, since the `File` drops with it
/// — it dies on a broken pipe. Measured against a `break`-at-cap edit, this
/// test sees the second: exit code 1 and `cmd.exe` reporting a write to a
/// nonexistent pipe.
///
/// So the child emits four times the cap, and the wall clock is short so the
/// first shape is a ten-second failure rather than a hang. Every other test in
/// this file produces output far under the cap and could not tell the
/// difference.
///
/// `text.len() + dropped_bytes == EMITTED` is the assertion that says *read to
/// EOF* rather than merely *read something*: the count the model is shown has
/// to be the real one, and it can only be real if every byte was accounted for.
#[test]
fn a_stream_past_the_cap_is_drained_to_the_end_and_the_drop_is_counted() {
    const CAP: usize = 16 * 1024;
    const EMITTED: usize = 4 * CAP;

    let dir = TempDir::new().expect("a temp dir");
    // ASCII, so `String::from_utf8_lossy` is byte-for-byte and `text.len()` is
    // a byte count that can be compared against the cap.
    std::fs::write(dir.path().join("big.txt"), "x".repeat(EMITTED)).expect("a file over the cap");
    let sandbox = Sandbox::create(dir.path(), &[]).expect("the profile and the grant");
    let _pruned_sandbox = guard::PrunedOnDrop::of(&sandbox);

    let run = sandbox
        .run(
            &system32("cmd.exe"),
            &args(&["/c", "type", "big.txt"]),
            CAP,
            Duration::from_secs(10),
            &uncancelled(),
        )
        .expect("a child that overruns its pipe must still be waited out, not deadlocked");

    assert_eq!(
        run.exit_code, 0,
        "the child ran to completion; stderr was {:?}",
        run.stderr.text
    );
    assert_eq!(
        run.stdout.text.len(),
        CAP,
        "the kept bytes stop at the cap exactly"
    );
    assert_eq!(
        run.stdout.text.len() + run.stdout.dropped_bytes,
        EMITTED,
        "and every byte the child wrote is either kept or counted as dropped"
    );
    assert_eq!(
        run.stderr.dropped_bytes, 0,
        "the other stream is untouched by this: {:?}",
        run.stderr.text
    );
}

/// The working directory is the root and nothing else: `RunParams` has no `cwd`
/// precisely so there is no second answer to this question.
#[test]
fn a_sandboxed_process_starts_in_its_root() {
    let dir = TempDir::new().expect("a temp dir");
    let sandbox = Sandbox::create(dir.path(), &[]).expect("the profile and the grant");
    let _pruned_sandbox = guard::PrunedOnDrop::of(&sandbox);

    let run = sandbox
        .run(
            &system32("cmd.exe"),
            &args(&["/c", "cd"]),
            16 * 1024,
            Duration::from_secs(30),
            &uncancelled(),
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

/// A file inside the root that Windows will not execute: the most
/// model-reachable failure there is, and it must be a refusal rather than a
/// hang or a leak.
#[test]
fn a_file_that_is_not_a_program_is_refused_rather_than_launched() {
    let dir = TempDir::new().expect("a temp dir");
    let not_a_program = dir.path().join("notes.txt");
    std::fs::write(&not_a_program, "plain text, not a PE image").expect("a file in the root");
    let sandbox = Sandbox::create(dir.path(), &[]).expect("the profile and the grant");
    let _pruned_sandbox = guard::PrunedOnDrop::of(&sandbox);

    let refusal = sandbox
        .run(
            &not_a_program,
            &args(&[]),
            16 * 1024,
            Duration::from_secs(30),
            &uncancelled(),
        )
        .expect_err("a text file is not launchable");

    assert!(
        refusal.contains("notes.txt") && refusal.contains("could not be launched"),
        "the refusal must name the file and say what failed: {refusal}"
    );
}

/// What a named run directory actually buys, measured rather than assumed
/// (spec 020 SC-002).
///
/// Two claims, and they are deliberately separated because only one of them is
/// attributable to the grant.
///
/// The launch itself is **not**. `Sandbox::run` issues `CreateProcessW` from
/// *this* process, whose token is the ordinary user, so the image file is
/// opened under the parent's rights and the AppContainer's DACL never enters
/// into it — measured: the same launch succeeds against a directory that was
/// never granted, and against a real `cargo.exe` under `%USERPROFILE%` too. It
/// is asserted here as a smoke test of the tool's own path and nothing more.
///
/// What the grant does buy is everything the **child** does for itself, and the
/// ungranted controls in this test are what prove it. Without an ACE naming the
/// AppContainer SID a child reading a file out of the run directory is refused
/// with *access denied*, and — the one that matters most — a child cannot even
/// **find** a binary there through its own `PATH`: it reports the name as not
/// recognised, because it cannot enumerate the directory. Granted, both work.
/// That pair is the whole justification for writing an ACE at all, and it is
/// the case a real toolchain lives in: the rustup shim re-executing the real
/// cargo, a linter invoking a helper, a compiler reading a library beside its
/// own binary.
///
/// The binary is a copy of `cmd.exe` renamed to `toolchain.exe` deliberately:
/// no such name exists in System32, so the resolution tests built on this later
/// cannot succeed for the wrong reason.
#[test]
fn a_binary_in_an_allowlisted_run_dir_executes_and_its_stdout_comes_back() {
    const MARKER: &str = "launched-from-the-run-dir";
    const DATA: &str = "bytes-beside-the-toolchain";
    let root = TempDir::new().expect("a temp root");
    let toolbin = TempDir::new().expect("a temp run directory");
    let tool = toolbin.path().join("toolchain.exe");
    std::fs::copy(system32("cmd.exe"), &tool).expect("a real PE image in the run directory");
    std::fs::write(toolbin.path().join("data.txt"), DATA).expect("a file beside it");
    // The verbatim prefix `TempDir` can carry is not a form `cmd.exe` accepts.
    let data = toolbin
        .path()
        .join("data.txt")
        .to_string_lossy()
        .replace(r"\\?\", "");

    // The controls first, ungranted: the child can neither read out of that
    // directory nor find a binary in it through its own `PATH`.
    let ungranted = Sandbox::create(root.path(), &[]).expect("the profile and the root's grant");
    let _pruned_ungranted = guard::PrunedOnDrop::of(&ungranted);
    let refused = ungranted
        .run(
            &system32("cmd.exe"),
            &args(&["/c", "type", &data]),
            16 * 1024,
            Duration::from_secs(30),
            &uncancelled(),
        )
        .expect("the launch itself succeeds; it is the read that must fail");
    assert!(
        !refused.stdout.text.contains(DATA),
        "without the grant the child must not read the run directory, or the assertion below \
         proves nothing: {refused:?}"
    );
    let unfound = ungranted
        .run(
            &system32("cmd.exe"),
            &args(&["/c", "toolchain.exe", "/c", "echo", MARKER]),
            16 * 1024,
            Duration::from_secs(30),
            &uncancelled(),
        )
        .expect("the outer launch succeeds; it is the inner one that must fail");
    assert!(
        !unfound.stdout.text.contains(MARKER),
        "and an ungranted directory is not even on the child's PATH to be found: {unfound:?}"
    );

    let sandbox = Sandbox::create(root.path(), &[toolbin.path().to_path_buf()])
        .expect("the profile, the root's grant and the run directory's");
    let _pruned_sandbox = guard::PrunedOnDrop::of(&sandbox);

    let read = sandbox
        .run(
            &system32("cmd.exe"),
            &args(&["/c", "type", &data]),
            16 * 1024,
            Duration::from_secs(30),
            &uncancelled(),
        )
        .expect("the launch succeeds");
    assert!(
        read.stdout.text.contains(DATA),
        "the grant must let the child read beside its toolchain, got {:?} / stderr {:?}",
        read.stdout.text,
        read.stderr.text
    );

    // The one that justifies writing an ACE at all: a child finding and
    // launching a sibling through the `PATH` the run directory put it on.
    let sibling = sandbox
        .run(
            &system32("cmd.exe"),
            &args(&["/c", "toolchain.exe", "/c", "echo", MARKER]),
            16 * 1024,
            Duration::from_secs(30),
            &uncancelled(),
        )
        .expect("the outer launch succeeds");
    assert!(
        sibling.stdout.text.contains(MARKER),
        "the child must find its toolchain on its own PATH and run it, got {:?} / stderr {:?}",
        sibling.stdout.text,
        sibling.stderr.text
    );

    let run = sandbox
        .run(
            &tool,
            &args(&["/c", "echo", MARKER]),
            16 * 1024,
            Duration::from_secs(30),
            &uncancelled(),
        )
        .expect("a binary in a named run directory launches inside the container");
    assert_eq!(
        run.exit_code, 0,
        "the tool's own launch path must reach it; stderr was {:?}",
        run.stderr.text
    );
    assert!(
        run.stdout.text.contains(MARKER),
        "and its real bytes must come back, got {:?} / stderr {:?}",
        run.stdout.text,
        run.stderr.text
    );
}
