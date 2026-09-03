//! The three containment gates (spec 019 SC-003, SC-004, SC-005).
//!
//! Each one's ground truth is an **effect that did not happen** — a file that
//! is not on disk, a connection the listener never accepted, a call that
//! returned instead of hanging — and each has its **unsandboxed positive
//! control in the same test**. The control is not padding: without it, a
//! mistyped `copy` or an unreachable port makes the test pass for the wrong
//! reason and the gate silently stops guarding anything.
//!
//! This is `governed_fs_run.rs`'s recorded discipline — *"its absence on disk
//! is the ground truth that nothing downstream ran. Not a counter in the
//! server: an effect the server would have had"* — applied to a process.
#![cfg(windows)]

use skein_sandbox::Sandbox;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn system32(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").expect("Windows names its own root"))
        .join("System32")
        .join(name)
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| v.to_string()).collect()
}

/// The identical argv, outside the sandbox. `std::process::Command` and not a
/// second `Sandbox::run`, so the control shares none of the machinery under
/// test.
fn unsandboxed(exe: &Path, argv: &[String], cwd: &Path) -> std::process::Output {
    std::process::Command::new(exe)
        .args(argv)
        .current_dir(cwd)
        .output()
        .expect("the control launches")
}

#[test]
fn a_sandboxed_process_cannot_write_outside_its_root() {
    let root = TempDir::new().expect("a temp root");
    let outside = TempDir::new().expect("a sibling directory the sandbox never hears about");
    std::fs::write(root.path().join("seed.txt"), "the payload").expect("a file to copy");
    let escaped = outside.path().join("escaped.txt");
    let argv = args(&[
        "/c",
        "copy",
        "seed.txt",
        &escaped.to_string_lossy().replace(r"\\?\", ""),
    ]);

    // The control first: if this argv does not create the file *outside* a
    // sandbox, the assertion below proves nothing about the sandbox.
    let control = unsandboxed(&system32("cmd.exe"), &argv, root.path());
    assert!(
        escaped.exists(),
        "the control must create the file, or the test below is vacuous: {}{}",
        String::from_utf8_lossy(&control.stdout),
        String::from_utf8_lossy(&control.stderr)
    );
    std::fs::remove_file(&escaped).expect("the control's file is removed before the real run");

    let sandbox = Sandbox::create(root.path(), &[]).expect("the profile and the grant");
    let run = sandbox
        .run(
            &system32("cmd.exe"),
            &argv,
            16 * 1024,
            Duration::from_secs(30),
        )
        .expect("the launch itself succeeds; it is the copy that must fail");

    // Constitution VI, proven by an effect rather than by a counter.
    assert!(
        !escaped.exists(),
        "a sandboxed process must not write outside its root; it wrote {}",
        escaped.display()
    );
    assert_ne!(
        run.exit_code, 0,
        "and it must say so: {} / {}",
        run.stdout.text, run.stderr.text
    );
}

#[test]
fn a_sandboxed_process_cannot_reach_the_network() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let port = listener.local_addr().expect("its address").port();
    let accepted = Arc::new(AtomicUsize::new(0));
    let counted = accepted.clone();
    // Detached deliberately: this thread must survive both runs, and the
    // accepted count is read rather than joined on.
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            if stream.is_err() {
                break;
            }
            counted.fetch_add(1, Ordering::SeqCst);
        }
    });

    let root = TempDir::new().expect("a temp root");
    // `--max-time 3` is load-bearing, not a convenience: a blocked AppContainer
    // loopback connect **times out** rather than failing fast, so without it
    // the child would sit until the sandbox's own wall clock ran out and the
    // test would read as a hang.
    let argv = args(&[
        "--max-time",
        "3",
        "--silent",
        &format!("http://127.0.0.1:{port}/"),
    ]);
    let curl = system32("curl.exe");
    assert!(
        curl.exists(),
        "curl.exe ships with Windows 10 1803+ and is this test's client"
    );

    // The control: the identical argv unsandboxed does reach the listener.
    unsandboxed(&curl, &argv, root.path());
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        1,
        "the control must connect, or the assertion below is vacuous"
    );

    let sandbox = Sandbox::create(root.path(), &[]).expect("the profile and the grant");
    let run = sandbox
        .run(&curl, &argv, 16 * 1024, Duration::from_secs(30))
        .expect("the launch itself succeeds; it is the connection that must fail");

    // The accepted count is the ground truth, exactly as the absent file is
    // above. Loopback is blocked for an AppContainer by a WFP filter matching
    // the `IsLoopback` condition, independently of the three capability SIDs
    // this profile also does not have.
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        1,
        "a sandboxed process must not reach the network; the listener accepted a second connection"
    );
    assert_ne!(
        run.exit_code, 0,
        "and curl must report the failure: {} / {}",
        run.stdout.text, run.stderr.text
    );
}

#[test]
fn the_job_object_kills_the_tree_when_the_clock_runs_out() {
    let root = TempDir::new().expect("a temp root");
    let sandbox = Sandbox::create(root.path(), &[]).expect("the profile and the grant");

    let started = Instant::now();
    // A **grandchild** — one `cmd.exe` launching another — because the bound
    // under test is the whole tree and not the one process the handle names.
    //
    // A counting loop and not `ping -n 60`, which is what the plan named:
    // `ping.exe` inside an AppContainer fails immediately with *"cannot contact
    // the IP driver"* — measured — because ICMP is capability-gated exactly as
    // TCP is. That is more evidence for the gate above and useless as a
    // stopwatch. `timeout.exe` is out for a different reason: it refuses to run
    // with redirected input, and every stream here is a pipe.
    let refusal = sandbox
        .run(
            &system32("cmd.exe"),
            &args(&[
                "/c",
                "cmd.exe",
                "/c",
                "for",
                "/l",
                "%i",
                "in",
                "(1,1,2000000000)",
                "do",
                "@rem",
            ]),
            16 * 1024,
            Duration::from_secs(2),
        )
        .expect_err("a loop far longer than the limit must be refused");
    let elapsed = started.elapsed();

    assert!(
        refusal.contains("2s limit") && refusal.contains("terminated"),
        "the refusal must name the limit so the model can plan around it: {refusal}"
    );
    // **That this test completes at all is the real assertion.** A leaked
    // descendant would still hold the pipes' write ends, the reader joins would
    // never return, and this would hang rather than fail. The bound turns that
    // into a failure instead of a merely slow pass.
    assert!(
        elapsed < Duration::from_secs(10),
        "the tree must die with the job; the call took {elapsed:?}"
    );
}

/// Narrowness as an **effect**, not as an intent (spec 020 SC-003).
///
/// `profile.rs` reads the mask back off the run directory's own security
/// descriptor and finds no write bit. This proves the same claim the other way
/// round, from outside the DACL model entirely: a real `copy` into that
/// directory leaves no file. Two independent proofs of one claim is this file's
/// recorded discipline, and it is what would catch a mask that looked narrow
/// and behaved wide.
///
/// The control is the **same copy into the fs-root**, which does land — so a
/// mistyped `copy` cannot make this pass for the wrong reason, and the test
/// pins the asymmetry rather than merely the refusal.
#[test]
fn a_sandboxed_process_cannot_write_into_a_run_dir() {
    let root = TempDir::new().expect("a temp root");
    let toolbin = TempDir::new().expect("a temp run directory");
    std::fs::write(root.path().join("seed.txt"), "the payload").expect("a file to copy");
    let escaped = toolbin.path().join("escaped.txt");
    let sandbox = Sandbox::create(root.path(), &[toolbin.path().to_path_buf()])
        .expect("the profile, the root's grant and the run directory's");

    // The control first, into the writable root: if this does not land, the
    // assertion below proves nothing about the run directory's narrower mask.
    let landed = sandbox
        .run(
            &system32("cmd.exe"),
            &args(&["/c", "copy", "seed.txt", "copied.txt"]),
            16 * 1024,
            Duration::from_secs(30),
        )
        .expect("the launch succeeds");
    assert!(
        root.path().join("copied.txt").exists(),
        "the control must land inside the root, or the assertion below is vacuous: {} / {}",
        landed.stdout.text,
        landed.stderr.text
    );

    let run = sandbox
        .run(
            &system32("cmd.exe"),
            &args(&[
                "/c",
                "copy",
                "seed.txt",
                &escaped.to_string_lossy().replace(r"\?\", ""),
            ]),
            16 * 1024,
            Duration::from_secs(30),
        )
        .expect("the launch itself succeeds; it is the copy that must fail");

    assert!(
        !escaped.exists(),
        "a run directory is for reaching an executable, not for writing to; it wrote {}",
        escaped.display()
    );
    assert_ne!(
        run.exit_code, 0,
        "and it must say so: {} / {}",
        run.stdout.text, run.stderr.text
    );
}
