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

use skein_connectors::{local_connector_with_run, FsRoot, RunAccess};
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

/// `ToolArgs::agent_policy(RunAccess::Allowed)`'s shape: allowed **and**
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
        RunAccess::Allowed,
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
