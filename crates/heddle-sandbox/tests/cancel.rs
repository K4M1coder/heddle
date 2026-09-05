//! Stopping a launch that is still running (spec 027 FR-002 … FR-007).
//!
//! **Returning at all is the assertion, in every test here.** `Sandbox::run`
//! joins both reader threads before it returns, and those threads see EOF only
//! once every write end of the pipes is closed — which needs the whole process
//! tree dead, grandchildren included. A cancellation that killed the one
//! process the handle names and leaked its child would hang here rather than
//! fail, so the bound each test puts on elapsed wall clock is what turns that
//! into a failure.
//!
//! ## The command, and the three that were measured and rejected
//!
//! A cancellation test needs a sandboxed command that genuinely keeps running.
//! Inside an AppContainer carrying **zero** capability SIDs almost nothing does,
//! and each rejection below is a measurement rather than a guess — all four ran
//! under a 2 s bound in this very fixture:
//!
//! | candidate | measured |
//! |---|---|
//! | `waitfor /t 30 <signal>` | exits 1 in ~25 ms: *"cannot wait for the specified signal"* — it needs a named kernel object the token cannot create |
//! | `timeout /t 30` | exits 1 in ~29 ms: *"input redirection is not supported"* — every stream here is a pipe |
//! | `ping -n 30 127.0.0.1` | exits 1 in ~28 ms: *"cannot contact the IP driver"* — ICMP is capability-gated exactly as TCP is |
//! | `cmd /c cmd /c for /l …` | still running at 2 s, terminated by the bound |
//!
//! The last one is the only survivor, and it is the command
//! `the_job_object_kills_the_tree_when_the_clock_runs_out` already uses — so
//! cancellation and the timeout are proved to stop *the same* thing. It is a
//! **grandchild**: one `cmd.exe` launching another, because the bound under
//! test is the whole tree and not the one process the handle names.
#![cfg(windows)]

mod guard;

use heddle_sandbox::Sandbox;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// A grandchild that counts far past any bound in this file.
fn forever() -> Vec<String> {
    args(&[
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
    ])
}

/// The same loop, counted down to something that spans many poll slices and
/// then finishes on its own.
fn briefly() -> Vec<String> {
    args(&["/c", "for", "/l", "%i", "in", "(1,1,2000000)", "do", "@rem"])
}

/// Deliberately far longer than any of these tests should take, so a
/// cancellation that is merely slow reads as a failure rather than as a pass.
const GENEROUS: Duration = Duration::from_secs(20);

#[test]
fn a_flag_set_while_a_child_runs_kills_it_long_before_its_timeout() {
    let root = TempDir::new().expect("a temp root");
    let sandbox = Sandbox::create(root.path(), &[]).expect("the profile and the grant");
    let _pruned_sandbox = guard::PrunedOnDrop::of(&sandbox);

    let cancelled = Arc::new(AtomicBool::new(false));
    let canceller = cancelled.clone();
    // From another thread, because that is where it comes from in the product:
    // the ACP connection's dispatch task sets it while the loop thread is
    // blocked in here.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        canceller.store(true, Ordering::SeqCst);
    });

    let started = Instant::now();
    let refusal = sandbox
        .run(
            &system32("cmd.exe"),
            &forever(),
            16 * 1024,
            GENEROUS,
            &cancelled,
        )
        .expect_err("a cancelled run is refused, not reported as an exit code");
    let elapsed = started.elapsed();

    assert!(
        refusal.contains("cancelled"),
        "the refusal must name the cancellation so the model is not told it timed out: {refusal}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "the child must die on the flag and not on the {GENEROUS:?} clock; the call took {elapsed:?}"
    );
}

/// The control for the test above, and the one thing the sliced wait puts at
/// risk that nothing on `dev` covers: that a run nobody cancelled is still
/// refused *as a timeout*, at the right moment, in the timeout's own words.
///
/// `the_job_object_kills_the_tree_when_the_clock_runs_out` proves the tree
/// dies; it cannot notice a message that started saying "cancelled" for every
/// run, and it cannot notice a first `WAIT_TIMEOUT` mistaken for expiry —
/// which would refuse this in 50 ms rather than in 2 s.
#[test]
fn a_run_nobody_cancelled_still_times_out_and_says_so() {
    let root = TempDir::new().expect("a temp root");
    let sandbox = Sandbox::create(root.path(), &[]).expect("the profile and the grant");
    let _pruned_sandbox = guard::PrunedOnDrop::of(&sandbox);

    let never = AtomicBool::new(false);
    let started = Instant::now();
    let refusal = sandbox
        .run(
            &system32("cmd.exe"),
            &forever(),
            16 * 1024,
            Duration::from_secs(2),
            &never,
        )
        .expect_err("a loop far longer than the limit must be refused");
    let elapsed = started.elapsed();

    assert!(
        refusal.contains("2s limit") && refusal.contains("terminated"),
        "the timeout keeps its own words: {refusal}"
    );
    assert!(
        !refusal.contains("cancel"),
        "and must not claim a cancellation nobody asked for: {refusal}"
    );
    assert!(
        elapsed >= Duration::from_secs(2),
        "a sliced wait must not expire on its first slice; the call took {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "the tree must die with the job; the call took {elapsed:?}"
    );
}

/// The second control: slicing the wait must not change what an ordinary run
/// returns. Every other run in the suite is over in well under one slice, so
/// this is the only one that exercises the loop's **normal** exit — many
/// `WAIT_TIMEOUT`s, then a `WAIT_OBJECT_0`, then the status read after it.
///
/// That a nonzero status survives `GetExitCodeProcess` is already proved on
/// `dev` by `a_sandboxed_process_cannot_reach_the_network`'s `assert_ne!`; what
/// was never exercised is reaching that call through a loop.
#[test]
fn a_run_that_outlives_many_poll_slices_still_returns_its_real_exit_code() {
    let root = TempDir::new().expect("a temp root");
    let sandbox = Sandbox::create(root.path(), &[]).expect("the profile and the grant");
    let _pruned_sandbox = guard::PrunedOnDrop::of(&sandbox);

    let never = AtomicBool::new(false);
    let started = Instant::now();
    let run = sandbox
        .run(
            &system32("cmd.exe"),
            &briefly(),
            16 * 1024,
            GENEROUS,
            &never,
        )
        .expect("a run inside its budget is not refused");
    let elapsed = started.elapsed();

    assert_eq!(
        run.exit_code, 0,
        "the child's own status, read after the loop; stderr was {:?}",
        run.stderr.text
    );
    assert!(
        elapsed > Duration::from_millis(150),
        "this command must span several poll slices or it proves nothing; it took {elapsed:?}"
    );
}
