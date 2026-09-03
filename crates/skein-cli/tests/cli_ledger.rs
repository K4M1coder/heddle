//! Acceptance tests for `skein ledger log|show|verify` (spec 011).
//!
//! Every test here **runs the real binary as a process** against a **real silo
//! on disk**. That is not ceremony: Constitution I says the CLI *is* the core's
//! authoritative client and the basis for E2E tests, so a unit test of an inner
//! formatter would prove nothing about the executable a person actually runs —
//! it would not exercise argument parsing, the exit code, the split between
//! stdout and stderr, or the fact that a binary is produced at all.
//!
//! Cargo sets `CARGO_BIN_EXE_skein` for integration tests of a package with a
//! `[[bin]]`, so reaching the binary needs no test-harness dependency.

use rusqlite::Connection;
use skein_core::{Step, StepKind};
use skein_silo::Silo;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

fn skein(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_skein"))
        .args(args)
        .output()
        .expect("the skein binary runs")
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("stdout is utf-8")
}

fn stderr(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).expect("stderr is utf-8")
}

/// A real silo holding two runs, returned with the steps as the chain built
/// them — so every expectation below is derived from the actual chain rather
/// than from a hard-coded hash.
fn seeded(id: &str) -> (TempDir, PathBuf, Vec<Step>) {
    let dir = TempDir::new().expect("a temp root");
    let root = dir.path().to_path_buf();
    let silo = Silo::open(&root, id).expect("a silo opens");
    let mut led = silo.ledger().expect("the silo's ledger opens");

    led.append("run-one", StepKind::LlmRequest, "ask the model")
        .unwrap();
    led.append("run-one", StepKind::LlmResponse, "the model replied")
        .unwrap();
    led.append("run-two", StepKind::ToolCall, r#"{"tool":"read_file"}"#)
        .unwrap();

    let steps: Vec<Step> = led
        .runs()
        .into_iter()
        .flat_map(|run| led.log(run).into_iter().cloned().collect::<Vec<_>>())
        .collect();
    (dir, root, steps)
}

fn root_arg(root: &Path) -> String {
    root.to_str().expect("a utf-8 temp path").to_string()
}

/// The four columns `ledger log` promises, spelled out here rather than
/// computed from the product's own formatter: a test that derives the expected
/// kind name the same way the code does could not catch the name drifting.
fn expected_line(step: &Step, kind: &str) -> String {
    format!("{}\t{}\t{}\t{}", step.run_id, step.seq, kind, step.id)
}

#[test]
fn e1_ledger_log_prints_every_step_in_the_silo() {
    let (_dir, root, steps) = seeded("alpha");

    let out = skein(&[
        "ledger",
        "log",
        "--root",
        &root_arg(&root),
        "--silo",
        "alpha",
    ]);

    assert!(out.status.success(), "{}", stderr(&out));
    let expected = format!(
        "{}\n{}\n{}\n",
        expected_line(&steps[0], "llm_request"),
        expected_line(&steps[1], "llm_response"),
        expected_line(&steps[2], "tool_call"),
    );
    assert_eq!(stdout(&out), expected);
}

#[test]
fn e2_ledger_log_filters_to_one_run() {
    let (_dir, root, steps) = seeded("alpha");

    let out = skein(&[
        "ledger",
        "log",
        "--root",
        &root_arg(&root),
        "--silo",
        "alpha",
        "--run",
        "run-two",
    ]);

    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        format!("{}\n", expected_line(&steps[2], "tool_call")),
        "the column set does not change with --run, so a script's field offsets never shift"
    );
}

#[test]
fn e3_ledger_show_prints_the_stored_step_verbatim() {
    let (_dir, root, steps) = seeded("alpha");
    let step = &steps[1];

    let out = skein(&[
        "ledger",
        "show",
        "--root",
        &root_arg(&root),
        "--silo",
        "alpha",
        &step.id,
    ]);

    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        format!(
            "id\t{}\nparent\t{}\nrun\t{}\nseq\t{}\nkind\tllm_response\npayload\n{}\n",
            step.id,
            step.parent
                .as_deref()
                .expect("the second step has a parent"),
            step.run_id,
            step.seq,
            step.payload,
        )
    );
}

#[test]
fn e4_ledger_show_of_an_unknown_id_fails_loudly() {
    let (_dir, root, _steps) = seeded("alpha");

    let out = skein(&[
        "ledger",
        "show",
        "--root",
        &root_arg(&root),
        "--silo",
        "alpha",
        "0000000000000000000000000000000000000000000000000000000000000000",
    ]);

    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("not found"), "{}", stderr(&out));
    assert!(
        stdout(&out).is_empty(),
        "a reader must never get a partial record: {}",
        stdout(&out)
    );
}

#[test]
fn e5_ledger_verify_passes_on_an_intact_chain() {
    let (_dir, root, _steps) = seeded("alpha");

    let out = skein(&[
        "ledger",
        "verify",
        "--root",
        &root_arg(&root),
        "--silo",
        "alpha",
    ]);

    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "run-one\tok\t2 steps\nrun-two\tok\t1 steps\n");
}

#[test]
fn e6_ledger_verify_fails_on_a_forged_row() {
    let (_dir, root, steps) = seeded("alpha");

    // A local writer with the file can always drop a trigger; that is why the
    // chain is hashed. Same technique as `silo_ledger.rs::s6` — the point here
    // is that the break is visible *from the CLI*.
    let ledger_path = Silo::open(&root, "alpha").unwrap().ledger_path();
    let raw = Connection::open(&ledger_path).unwrap();
    raw.execute_batch("DROP TRIGGER ledger_step_no_update")
        .unwrap();
    raw.execute(
        "UPDATE ledger_step SET payload = 'forged' WHERE id = ?1",
        [&steps[0].id],
    )
    .unwrap();
    drop(raw);

    let out = skein(&[
        "ledger",
        "verify",
        "--root",
        &root_arg(&root),
        "--silo",
        "alpha",
    ]);

    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("ledger integrity broken at seq 0"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn e7_a_missing_ledger_is_an_error_not_an_empty_log() {
    let (_dir, root, _steps) = seeded("alpha");

    let out = skein(&[
        "ledger",
        "log",
        "--root",
        &root_arg(&root),
        "--silo",
        "tpyo",
    ]);

    assert_eq!(
        out.status.code(),
        Some(1),
        "a typo'd silo must not look like a silo with no history"
    );
    assert!(stderr(&out).contains("not found"), "{}", stderr(&out));
    assert!(
        !root.join("tpyo").join("ledger.sqlite3").exists(),
        "an inspection command must not create the thing it inspects"
    );
}

/// The alternative to `--root` on every invocation. Kept in the same test as
/// its own precedence so there is no second silo fixture for one assertion.
#[test]
fn e10_the_silo_root_falls_back_to_skein_root_and_is_required() {
    let (_dir, root, steps) = seeded("alpha");

    let from_env = Command::new(env!("CARGO_BIN_EXE_skein"))
        .args(["ledger", "log", "--silo", "alpha"])
        .env("SKEIN_ROOT", &root)
        .output()
        .expect("the skein binary runs");
    assert!(from_env.status.success(), "{}", stderr(&from_env));
    assert!(stdout(&from_env).contains(&steps[0].id));

    let with_neither = Command::new(env!("CARGO_BIN_EXE_skein"))
        .args(["ledger", "log", "--silo", "alpha"])
        .env_remove("SKEIN_ROOT")
        .output()
        .expect("the skein binary runs");
    assert_eq!(with_neither.status.code(), Some(1));
    assert!(
        stderr(&with_neither).contains("no silo root"),
        "{}",
        stderr(&with_neither)
    );
}
