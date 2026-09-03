//! The hand-verification, made repeatable (spec 019 T13).
//!
//! The one thing no stub can prove: that a **real** local model, shown this
//! tool in this wire format, actually asks for it — and that a real process
//! runs and its real output comes back on the chain.
//!
//! `#[ignore]`d, so `cargo test --workspace` stays green on a machine with no
//! Ollama, and `.github/workflows/core.yml` runs without `--include-ignored` so
//! it never runs there either. This is `governed_fs_run.rs`'s
//! `a_live_model_calls_a_real_fs_tool` pattern, which exists so a
//! hand-verification is repeatable rather than a one-off. Run it by hand:
//!
//! ```text
//! $env:SKEIN_LIVE_MODEL = "qwen3:8b"
//! cargo test -p skein-connectors --test governed_proc_run -- --ignored --nocapture
//! ```
//!
//! Not every Ollama model supports tool calling. If a model ignores the `tools`
//! array this fails, and that is a model-selection finding rather than a code
//! defect.
#![cfg(windows)]

use skein_connectors::{local_connector_with_run, FsRoot, RunAccess, RunDirs};
use skein_core::{
    Ledger, LoopBudget, LoopController, Message, NativeLoop, ProgressProbe, Redactor, StepKind,
    ToolAccess, ToolGateway, ToolPolicy,
};
use skein_gateway::{LocalEndpoint, OpenAiCompatClient};
use std::time::Duration;
use tempfile::TempDir;

const FILE_CONTENTS: &str = "the line a real process has to print";

/// `wiring::NoGroundTruth`'s reasoning, copied for the reason
/// `governed_fs_run.rs` records: `skein-cli` has no `lib` target to share it
/// through.
struct NoGroundTruth;

impl ProgressProbe for NoGroundTruth {
    fn observe(&mut self) -> bool {
        false
    }
}

/// `ToolArgs::agent_policy`'s shape under `RunAccess::Allowed`: allowed **and**
/// approved, because `call_captured` consults the policy before the transport,
/// so a `Mutating` tool absent from `approved` never reaches one.
fn agent_policy() -> ToolPolicy {
    ToolPolicy::new(
        vec![
            ("fs_read".to_string(), ToolAccess::ReadOnly),
            ("fs_list".to_string(), ToolAccess::ReadOnly),
            ("proc_run".to_string(), ToolAccess::Mutating),
        ],
        vec!["proc_run".to_string()],
    )
}

#[test]
#[ignore = "needs a real tool-capable local provider; set SKEIN_LIVE_MODEL to run"]
fn a_live_model_calls_a_real_proc_run() {
    let Some(model_name) = std::env::var_os("SKEIN_LIVE_MODEL") else {
        eprintln!("SKEIN_LIVE_MODEL is unset; skipping the live model tool-call test");
        return;
    };
    let model_name = model_name.to_string_lossy().to_string();
    let base_url = std::env::var("SKEIN_MODEL_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());

    let dir = TempDir::new().expect("a temp dir");
    std::fs::write(dir.path().join("notes.txt"), FILE_CONTENTS).expect("a file in the root");
    let connector = local_connector_with_run(
        FsRoot::new(dir.path()).expect("a canonicalizable root"),
        RunAccess::Allowed(RunDirs::none()),
    )
    .expect("the sandbox builds and the embedded server starts");

    let redactor = Redactor::new(Vec::new());
    let mut loops = NativeLoop::new(
        OpenAiCompatClient::new(
            LocalEndpoint::parse(&base_url).expect("a loopback base URL"),
            &model_name,
            Duration::from_secs(120),
        ),
        NoGroundTruth,
        ToolGateway::new(connector, agent_policy(), redactor.clone()),
        redactor,
    );
    let mut ledger = Ledger::new();
    let mut controller = LoopController::new(LoopBudget::new(4, 1_000_000, 4));

    let run = loops
        .run(
            "run-live",
            Message::user_text(
                "Using the proc_run tool, run the command `cmd.exe` with the arguments \
                 [\"/c\", \"type\", \"notes.txt\"] and tell me exactly what it printed.",
            ),
            &mut ledger,
            &mut controller,
        )
        .unwrap_or_else(|e| panic!("{base_url} did not complete a run for {model_name:?}: {e}"));

    for step in ledger.log("run-live") {
        eprintln!("{:>20}  {}", format!("{:?}", step.kind), step.payload);
    }
    eprintln!("exit = {:?}\nanswer = {:?}", run.exit, run.final_message);

    let results: Vec<String> = ledger
        .log("run-live")
        .iter()
        .filter(|s| s.kind == StepKind::ToolResult)
        .map(|s| s.payload.clone())
        .collect();
    assert!(
        results.iter().any(|p| p.contains("proc_run")),
        "the model was told it can run a process and did not ask; if it cannot call tools that is \
         a model-selection finding, not a defect: {:?}",
        ledger.log("run-live")
    );
    // The bytes, not the exit code: a launcher that reported success without
    // running anything would pass an exit-code assertion.
    assert!(
        results.iter().any(|p| p.contains(FILE_CONTENTS)),
        "a real process's real output must have reached the chain: {results:?}"
    );
    ledger
        .verify_chain("run-live")
        .expect("a live run's chain verifies");
}

/// The run-dir allowlist's own hand-verification, made repeatable (spec 020
/// T10).
///
/// The one thing no hermetic test can prove: that a real local model, shown a
/// `proc_run` whose description names a real toolchain directory, asks for the
/// binary in it — and that the binary really runs inside the AppContainer and
/// its real output reaches the chain.
///
/// **`$SKEIN_LIVE_RUN_DIR` grants that directory a read-and-execute ACE that
/// outlives this test**, which is why it is opt-in by an environment variable
/// rather than defaulted to anything. Name a directory you own: a directory
/// owned by SYSTEM needs an elevated shell and fails with
/// `ERROR_ACCESS_DENIED`.
///
/// With `$SKEIN_LIVE_SILO_ROOT` set the chain is persisted to a real silo, so
/// `skein ledger log` and `skein ledger show` can read the evidence back **in a
/// second process** — which is the form slice 019's live section records. Unset,
/// it falls back to an in-memory `Ledger` so nothing about
/// `cargo test --workspace` changes.
///
/// ```text
/// $env:SKEIN_LIVE_MODEL     = "qwen3:8b"
/// $env:SKEIN_LIVE_RUN_DIR   = "D:\Users\you\.rustup\toolchains\1.97-x86_64-pc-windows-msvc\bin"
/// $env:SKEIN_LIVE_SILO_ROOT = "$env:TEMP\skein-live-020"
/// cargo test -p skein-connectors --test governed_proc_run -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs a real local provider and a real toolchain directory; set SKEIN_LIVE_MODEL and \
            SKEIN_LIVE_RUN_DIR to run"]
fn a_live_model_runs_a_real_toolchain_binary() {
    let (Some(model_name), Some(run_dir)) = (
        std::env::var_os("SKEIN_LIVE_MODEL"),
        std::env::var_os("SKEIN_LIVE_RUN_DIR"),
    ) else {
        eprintln!(
            "SKEIN_LIVE_MODEL or SKEIN_LIVE_RUN_DIR is unset; skipping the live run-dir test"
        );
        return;
    };
    let model_name = model_name.to_string_lossy().to_string();
    let base_url = std::env::var("SKEIN_MODEL_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());

    let dir = TempDir::new().expect("a temp dir");
    let run_dirs = RunDirs::new(&[std::path::PathBuf::from(&run_dir)])
        .unwrap_or_else(|e| panic!("{run_dir:?} is not a usable run directory: {e}"));
    eprintln!("granting and searching {:?}", run_dirs.paths());
    let connector = local_connector_with_run(
        FsRoot::new(dir.path()).expect("a canonicalizable root"),
        RunAccess::Allowed(run_dirs),
    )
    .unwrap_or_else(|e| panic!("the sandbox must build over {run_dir:?}: {e}"));

    let redactor = Redactor::new(Vec::new());
    let mut loops = NativeLoop::new(
        OpenAiCompatClient::new(
            LocalEndpoint::parse(&base_url).expect("a loopback base URL"),
            &model_name,
            Duration::from_secs(120),
        ),
        NoGroundTruth,
        ToolGateway::new(connector, agent_policy(), redactor.clone()),
        redactor,
    );
    // A real silo when one is named, so the chain survives this process and
    // `skein ledger show` can be the evidence rather than this test's stdout.
    let mut ledger = match std::env::var_os("SKEIN_LIVE_SILO_ROOT") {
        Some(root) => skein_silo::Silo::open(std::path::Path::new(&root), "live020")
            .and_then(|silo| silo.ledger())
            .expect("a silo at $SKEIN_LIVE_SILO_ROOT"),
        None => Ledger::new(),
    };
    let mut controller = LoopController::new(LoopBudget::new(4, 1_000_000, 4));

    let run = loops
        .run(
            "run-live-020",
            Message::user_text(
                "Using the proc_run tool, run the command `cargo` with the arguments \
                 [\"--version\"] and tell me exactly what it printed.",
            ),
            &mut ledger,
            &mut controller,
        )
        .unwrap_or_else(|e| panic!("{base_url} did not complete a run for {model_name:?}: {e}"));

    for step in ledger.log("run-live-020") {
        eprintln!("{:>20}  {}", format!("{:?}", step.kind), step.payload);
    }
    eprintln!("exit = {:?}\nanswer = {:?}", run.exit, run.final_message);

    let results: Vec<String> = ledger
        .log("run-live-020")
        .iter()
        .filter(|s| s.kind == StepKind::ToolResult)
        .map(|s| s.payload.clone())
        .collect();
    assert!(
        results.iter().any(|p| p.contains("proc_run")),
        "the model was told which directory it can reach and did not ask; if it cannot call tools \
         that is a model-selection finding, not a defect: {:?}",
        ledger.log("run-live-020")
    );
    // `exit 0` and not merely the tool's name: a resolution that found nothing
    // would also land a `ToolResult`, carrying the refusal instead of output.
    assert!(
        results.iter().any(|p| p.contains("exit 0")),
        "a binary in the named run directory must really have run: {results:?}"
    );
    ledger
        .verify_chain("run-live-020")
        .expect("a live run's chain verifies");
}
