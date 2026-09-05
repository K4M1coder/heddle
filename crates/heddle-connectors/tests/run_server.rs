//! `proc_run`, exercised as the server's own method (spec 019 SC-008).
//!
//! `fs_server.rs`'s level and for its recorded reason: this is where a
//! tool-level refusal is still an `Err(String)`, before rmcp turns it into a
//! `CallToolResult { is_error: true }`. The end-to-end proof that it really
//! arrives that way is `cli_acp_agent.rs`, and the containment gates live in
//! `heddle-sandbox`'s own `tests/escape.rs` where the positive controls are.
#![cfg(windows)]

mod guard;

use heddle_connectors::{
    EmbeddedServer, FsRoot, RunAccess, RunDirs, RunParams, RUN_OUTPUT_BYTE_CAP, RUN_TIMEOUT,
};
use heddle_sandbox::ARG_COUNT_CAP;
use rmcp::handler::server::wrapper::Parameters;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

struct Fixture {
    server: EmbeddedServer,
    /// The same flag the server holds. Every test but the cancellation one
    /// leaves it alone, which is what makes each of them a control for the
    /// claim that an uncancelled run is unchanged.
    cancelled: Arc<AtomicBool>,
    /// Between the server and the directories on purpose: it drops with the
    /// server's sandbox already gone and the root still on disk, so the revoke
    /// it performs is a real one.
    _pruned: guard::PrunedOnDrop,
    /// Declared **last**, for the reason `fs_root.rs`'s fixture records: the
    /// server's root holds an open directory handle, and fields drop in
    /// declaration order.
    _dir: TempDir,
    _toolbin: TempDir,
}

fn system32(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").expect("Windows names its own root"))
        .join("System32")
        .join(name)
}

/// The root is a **subdirectory**, so there is somewhere real for a containment
/// refusal to point at: `fs_server.rs`'s own fixture shape, for its own reason.
/// A path to a file that does not exist is refused too, but on the wrong
/// grounds — "not found" rather than "outside the root" — and a test that
/// accepted either would not notice if containment stopped working.
fn fixture() -> Fixture {
    built(false)
}

/// [`fixture`] with the toolchain directory actually **named**. The two differ
/// in nothing else, which is what makes the pair of resolution tests below a
/// comparison rather than two unrelated runs.
fn fixture_with_a_named_run_dir() -> Fixture {
    built(true)
}

fn built(name_the_run_dir: bool) -> Fixture {
    let dir = TempDir::new().expect("a temp dir");
    let root_path = dir.path().join("root");
    std::fs::create_dir(&root_path).expect("the root is created");
    std::fs::write(root_path.join("seed.txt"), "seeded bytes").expect("a file in the root");
    std::fs::write(dir.path().join("outside.exe"), "not yours").expect("a file outside the root");
    let root = FsRoot::new(&root_path).expect("a canonicalizable root");
    let toolbin = TempDir::new().expect("a temp run directory");
    // A copy of `cmd.exe` under a name System32 does not have, so a resolution
    // test cannot pass for the wrong reason.
    std::fs::copy(system32("cmd.exe"), toolbin.path().join("toolchain.exe"))
        .expect("a real PE image in the run directory");
    let run_dirs = if name_the_run_dir {
        RunDirs::new(&[toolbin.path().to_path_buf()]).expect("a real directory")
    } else {
        RunDirs::none()
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    Fixture {
        server: EmbeddedServer::with_run(root, RunAccess::Allowed(run_dirs), cancelled.clone())
            .expect("the sandbox is built once, here"),
        cancelled,
        _pruned: guard::PrunedOnDrop::of_root(&root_path),
        _toolbin: toolbin,
        _dir: dir,
    }
}

fn run(server: &EmbeddedServer, command: &str, args: &[&str]) -> Result<String, String> {
    server.proc_run(Parameters(RunParams {
        command: command.to_string(),
        args: args.iter().map(|a| a.to_string()).collect(),
    }))
}

#[test]
fn a_successful_run_reports_its_exit_code_and_both_streams() {
    let fixture = fixture();

    let report = run(&fixture.server, "cmd.exe", &["/c", "type", "seed.txt"])
        .expect("a readable file inside the root is not a refusal");

    assert!(report.starts_with("exit 0\n"), "{report}");
    assert!(report.contains("--- stdout ---"), "{report}");
    assert!(report.contains("--- stderr ---"), "{report}");
    assert!(report.contains("seeded bytes"), "{report}");
}

/// The process ran; the result is true; the model needs the output. An `Err`
/// would discard both, which is why a nonzero exit is an `Ok`.
#[test]
fn a_nonzero_exit_is_an_ok_and_not_an_error() {
    let fixture = fixture();

    let report = run(&fixture.server, "cmd.exe", &["/c", "exit", "3"])
        .expect("a failing process is a successful tool call");

    assert!(report.starts_with("exit 3\n"), "{report}");
}

#[test]
fn output_over_the_cap_is_truncated_and_the_drop_is_labelled() {
    let fixture = fixture();
    let filler = "x".repeat(200);
    let script = format!("for /l %i in (1,1,200) do @echo {filler}");

    let report = run(&fixture.server, "cmd.exe", &["/c", &script])
        .expect("a chatty process is not a refusal");

    // Truncated **and labelled**, following `git_status`'s reasoning and not
    // `fs_read`'s: the process has already run and cannot be un-run, and there
    // is no smaller call for the model to make instead.
    assert!(
        report.contains("bytes of stdout not shown"),
        "the drop must be labelled, not silent: {report}"
    );
    let kept = report
        .split("--- stdout ---\n")
        .nth(1)
        .expect("a stdout section")
        .len();
    assert!(
        kept < RUN_OUTPUT_BYTE_CAP + 1024,
        "the kept text must respect the {RUN_OUTPUT_BYTE_CAP}-byte cap, got {kept}"
    );
}

#[test]
fn too_many_arguments_is_a_named_refusal() {
    let fixture = fixture();
    let many: Vec<String> = (0..=ARG_COUNT_CAP).map(|i| i.to_string()).collect();
    let borrowed: Vec<&str> = many.iter().map(String::as_str).collect();

    let refusal =
        run(&fixture.server, "cmd.exe", &borrowed).expect_err("one over the cap must be refused");

    assert!(
        refusal.contains(&ARG_COUNT_CAP.to_string()),
        "the refusal must name the cap: {refusal}"
    );
}

#[test]
fn a_command_naming_a_path_outside_the_root_is_a_named_refusal() {
    let fixture = fixture();

    // The file really is there, so the refusal is about containment and not
    // about the file being missing.
    let refusal = run(&fixture.server, r"..\outside.exe", &[])
        .expect_err("a path out of the root is refused");

    assert!(
        refusal.contains("outside the root"),
        "the refusal must say the root is the boundary: {refusal}"
    );
}

#[test]
fn an_absolute_command_is_refused_even_when_it_exists() {
    let fixture = fixture();

    // The one executable this slice is certain exists, named the one way the
    // tool does not accept. A model that has learned `C:\Windows\...` from its
    // training data must be told the rule, not silently obeyed.
    let refusal = run(
        &fixture.server,
        r"C:\Windows\System32\cmd.exe",
        &["/c", "exit", "0"],
    )
    .expect_err("an absolute path is refused however real it is");

    assert!(
        refusal.contains("absolute") || refusal.contains("outside the root"),
        "the refusal must name the rule: {refusal}"
    );
}

/// A refusal that names **both** places it looked, and says `PATH` is not one
/// of them: the model cannot otherwise tell "not installed" from "not reachable
/// from here", and this tool deliberately does not search `%PATH%`.
#[test]
fn a_command_that_resolves_nowhere_names_both_places_it_looked() {
    let fixture = fixture();

    let refusal = run(&fixture.server, "definitely-not-a-real-binary", &[])
        .expect_err("an unresolvable command must be refused");

    assert!(
        refusal.contains("definitely-not-a-real-binary.exe") && refusal.contains("System32"),
        "the refusal must name what it looked for and where: {refusal}"
    );
    assert!(
        refusal.contains("PATH"),
        "and it must say `PATH` is deliberately not searched: {refusal}"
    );
}

/// The rule at the tool's own level (spec 020 SC-004): a bare name, no
/// extension, resolved inside a directory the operator named.
#[test]
fn a_bare_name_in_an_allowlisted_run_dir_resolves_and_runs() {
    let fixture = fixture_with_a_named_run_dir();

    let report = run(&fixture.server, "toolchain", &["/c", "type", "seed.txt"])
        .expect("a bare name in a named run directory is not a refusal");

    assert!(
        report.starts_with(
            "exit 0
"
        ),
        "{report}"
    );
    // The root's file, read by a binary reached through the run directory: the
    // two halves of the configuration have to work at once or this says
    // nothing.
    assert!(report.contains("seeded bytes"), "{report}");
}

/// The same bare name with that directory **not** named (spec 020 SC-005).
///
/// The allowlist has not become a de facto `%PATH%`: a directory that exists,
/// holds the binary, and was simply not named is not searched. And the refusal
/// enumerates every place it really looked, because a model cannot otherwise
/// tell "not installed" from "not named at launch".
#[test]
fn a_bare_name_in_a_directory_that_was_not_named_names_every_place_it_looked() {
    let unnamed = fixture();
    let named = fixture_with_a_named_run_dir();

    let refusal = run(&unnamed.server, "toolchain", &["/c", "exit", "0"])
        .expect_err("a directory that was not named must not be searched");
    assert!(
        refusal.contains("toolchain.exe") && refusal.contains("System32"),
        "the refusal must name what it looked for and where: {refusal}"
    );
    assert!(
        refusal.contains("PATH"),
        "and it must still say `PATH` is deliberately not searched: {refusal}"
    );

    // The control, and the reason this test builds two fixtures: the same call
    // against the same directory succeeds once it is named, so the refusal
    // above is about the allowlist and not about the binary being unusable.
    run(&named.server, "toolchain", &["/c", "exit", "0"])
        .expect("naming the directory is the only difference");

    // And a refusal from a configuration that *has* run directories has to name
    // them, or an operator cannot tell a mistyped `--run-dir` from a missing
    // binary.
    let enumerated = run(&named.server, "definitely-not-a-real-binary", &[])
        .expect_err("an unresolvable command is still refused");
    // Canonicalized the same way `RunDirs::new` canonicalizes it: on at
    // least one real Windows CI runner, a `TempDir`'s raw path and its
    // canonical form differ (an 8.3 short-name component resolves to its
    // long form), so comparing against the raw path is flaky by environment
    // rather than by behavior.
    let toolbin = std::fs::canonicalize(named._toolbin.path())
        .expect("the fixture's own directory canonicalizes")
        .to_string_lossy()
        .replace(r"\\?\", "");
    assert!(
        enumerated.contains(&toolbin),
        "the refusal must name every directory that was searched, including {toolbin}: {enumerated}"
    );
}

/// D5's append-not-prepend order, proven by a run that can only succeed if
/// System32 was searched first (spec 020 SC-006).
///
/// The shadowing file is deliberately **not** a program: if resolution reached
/// it, `CreateProcessW` would refuse the launch and this would fail loudly
/// rather than quietly running the right thing for the wrong reason.
#[test]
fn system32_still_wins_over_a_run_dir_that_shadows_it() {
    let fixture = fixture_with_a_named_run_dir();
    std::fs::write(
        fixture._toolbin.path().join("cmd.exe"),
        "plain text, not a PE image",
    )
    .expect("a shadowing file in the run directory");

    let report = run(&fixture.server, "cmd.exe", &["/c", "type", "seed.txt"])
        .expect("System32 is searched before any run directory");

    assert!(
        report.starts_with(
            "exit 0
"
        ),
        "{report}"
    );
    assert!(report.contains("seeded bytes"), "{report}");
}

/// A grandchild that outlives `RUN_TIMEOUT` many times over — the one command
/// `heddle-sandbox`'s `tests/cancel.rs` measured as surviving an AppContainer
/// with zero capability SIDs.
fn forever() -> Vec<&'static str> {
    vec![
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
    ]
}

/// The flag reaches the child, through rmcp's `&self` handler and the `Arc` the
/// server was built with.
///
/// The elapsed bound is the assertion that matters. Without the flag reaching
/// the sandbox this still ends in an `Err` — thirty seconds later, saying it
/// timed out — so a test that only checked for a refusal would pass on a
/// server wired to nothing.
#[test]
fn a_flag_set_while_proc_run_is_executing_ends_it_with_a_named_refusal() {
    let fixture = fixture();
    let canceller = fixture.cancelled.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        canceller.store(true, Ordering::SeqCst);
    });

    let started = Instant::now();
    let refusal = run(&fixture.server, "cmd.exe", &forever())
        .expect_err("a cancelled run is a tool error, not a report of an exit code");
    let elapsed = started.elapsed();

    assert!(
        refusal.contains("cancelled"),
        "the model must be told which of the two bounds stopped it: {refusal}"
    );
    assert!(
        elapsed < RUN_TIMEOUT / 2,
        "the flag and not the clock must have ended it; the call took {elapsed:?}"
    );
}
