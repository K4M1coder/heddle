//! `proc_run`, exercised as the server's own method (spec 019 SC-008).
//!
//! `fs_server.rs`'s level and for its recorded reason: this is where a
//! tool-level refusal is still an `Err(String)`, before rmcp turns it into a
//! `CallToolResult { is_error: true }`. The end-to-end proof that it really
//! arrives that way is `cli_acp_agent.rs`, and the containment gates live in
//! `skein-sandbox`'s own `tests/escape.rs` where the positive controls are.
#![cfg(windows)]

use rmcp::handler::server::wrapper::Parameters;
use skein_connectors::{EmbeddedServer, FsRoot, RunAccess, RunParams, RUN_OUTPUT_BYTE_CAP};
use skein_sandbox::ARG_COUNT_CAP;
use tempfile::TempDir;

struct Fixture {
    _dir: TempDir,
    server: EmbeddedServer,
}

/// The root is a **subdirectory**, so there is somewhere real for a containment
/// refusal to point at: `fs_server.rs`'s own fixture shape, for its own reason.
/// A path to a file that does not exist is refused too, but on the wrong
/// grounds — "not found" rather than "outside the root" — and a test that
/// accepted either would not notice if containment stopped working.
fn fixture() -> Fixture {
    let dir = TempDir::new().expect("a temp dir");
    let root_path = dir.path().join("root");
    std::fs::create_dir(&root_path).expect("the root is created");
    std::fs::write(root_path.join("seed.txt"), "seeded bytes").expect("a file in the root");
    std::fs::write(dir.path().join("outside.exe"), "not yours").expect("a file outside the root");
    let root = FsRoot::new(&root_path).expect("a canonicalizable root");
    Fixture {
        server: EmbeddedServer::with_run(root, RunAccess::Allowed)
            .expect("the sandbox is built once, here"),
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
