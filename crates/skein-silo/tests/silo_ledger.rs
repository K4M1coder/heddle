//! Acceptance tests for the durable silo-backed Ledger (spec 009).
//! Every assertion is against a real SQLite file under a real temporary
//! directory: the persistence claim is worth nothing against an in-memory
//! stand-in, because the thing being proved is that the chain outlives the
//! connection that wrote it.

use rusqlite::Connection;
use serde_json::json;
use skein_core::{
    Exit, Ledger, LoopBudget, LoopController, Message, ModelClient, NativeLoop, ProgressProbe,
    Redactor, Result, SkeinError, StepKind, ToolAccess, ToolCall, ToolGateway, ToolOutcome,
    ToolPolicy, ToolTransport, TurnRequest, TurnResponse,
};
use skein_silo::Silo;
use tempfile::TempDir;

// ---- doubles ----
// Copied, not shared: the ones in `skein-core`'s and `skein-mcp`'s test
// binaries are private to those binaries (slice 008's precedent).

/// A model whose every turn is decided in advance.
struct ScriptedModel {
    script: Vec<TurnResponse>,
    calls: usize,
}

impl ModelClient for ScriptedModel {
    fn turn(&mut self, _req: &TurnRequest) -> Result<TurnResponse> {
        let i = self.calls;
        self.calls += 1;
        Ok(self
            .script
            .get(i)
            .unwrap_or_else(|| panic!("script exhausted: the loop asked for turn {i}"))
            .clone())
    }
}

/// Ground truth the test controls directly — never the model.
struct StaticProbe(bool);

impl ProgressProbe for StaticProbe {
    fn observe(&mut self) -> bool {
        self.0
    }
}

struct CountingTransport {
    calls: usize,
    reply: String,
}

impl ToolTransport for CountingTransport {
    fn call(&mut self, _call: &ToolCall) -> Result<ToolOutcome> {
        self.calls += 1;
        Ok(ToolOutcome {
            content: self.reply.clone(),
        })
    }
}

fn reply(text: &str, final_output: bool, tool_calls: Vec<ToolCall>) -> TurnResponse {
    TurnResponse {
        message: Message::assistant_text(text),
        tokens_used: 10,
        final_output,
        tool_calls,
    }
}

fn kinds(led: &Ledger, run_id: &str) -> Vec<StepKind> {
    led.log(run_id).iter().map(|s| s.kind.clone()).collect()
}

/// A root directory inside a `TempDir`, so a test can also assert that nothing
/// was created *beside* the root.
fn root() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().expect("temp dir");
    let root = dir.path().join("silos");
    std::fs::create_dir(&root).expect("root");
    (dir, root)
}

// ---- persistence ----

#[test]
fn s1_ledger_survives_close_and_reopen() {
    let (_dir, root) = root();
    let silo = Silo::open(&root, "alpha").unwrap();

    let mut led = silo.ledger().unwrap();
    let written: Vec<String> = [
        (StepKind::LlmRequest, "the exact prompt"),
        (StepKind::LlmResponse, "the exact reply"),
        (StepKind::ToolCall, "{\"tool\":\"read_file\"}"),
        (StepKind::Exit, "FinalOutput"),
    ]
    .into_iter()
    .map(|(kind, payload)| led.append("run-1", kind, payload).unwrap())
    .collect();
    // The process boundary this test can simulate: the connection is closed.
    drop(led);

    let reopened = silo.ledger().unwrap();
    let log = reopened.log("run-1");
    assert_eq!(log.len(), 4, "every step came back");
    assert_eq!(
        log.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
        written,
        "same ids, so the chain is the one that was written"
    );
    assert_eq!(log[0].payload, "the exact prompt");
    assert_eq!(log[3].kind, StepKind::Exit);
    reopened
        .verify_chain("run-1")
        .expect("the reopened chain verifies");
}

#[test]
fn s2_reopened_chain_continues_rather_than_restarts() {
    let (_dir, root) = root();
    let silo = Silo::open(&root, "alpha").unwrap();

    let mut led = silo.ledger().unwrap();
    for (kind, payload) in [
        (StepKind::LlmRequest, "one"),
        (StepKind::LlmResponse, "two"),
        (StepKind::ToolCall, "three"),
    ] {
        led.append("run-1", kind, payload).unwrap();
    }
    let last = led.append("run-1", StepKind::Exit, "FinalOutput").unwrap();
    drop(led);

    let mut reopened = silo.ledger().unwrap();
    let fifth = reopened
        .append("run-1", StepKind::StateChange, "after the restart")
        .unwrap();

    let step = reopened.show(&fifth).unwrap();
    assert_eq!(step.seq, 4, "the chain continued rather than forking");
    assert_eq!(step.parent.as_deref(), Some(last.as_str()));
    reopened
        .verify_chain("run-1")
        .expect("the continued chain verifies");
    assert_eq!(reopened.log("run-1").len(), 5);
}

// ---- isolation (Constitution II/III) ----

#[test]
fn s3_a_write_in_one_silo_is_invisible_from_another() {
    let (_dir, root) = root();
    let alpha = Silo::open(&root, "alpha").unwrap();
    let beta = Silo::open(&root, "beta").unwrap();

    let mut alpha_led = alpha.ledger().unwrap();
    let alpha_id = alpha_led
        .append("run-x", StepKind::LlmRequest, "alpha's secret business")
        .unwrap();
    drop(alpha_led);

    let beta_led = beta.ledger().unwrap();
    assert!(
        beta_led.log("run-x").is_empty(),
        "beta cannot see alpha's run"
    );
    assert!(
        matches!(beta_led.show(&alpha_id), Err(SkeinError::NotFound(_))),
        "beta cannot resolve alpha's step id"
    );
    drop(beta_led);

    // …and the traffic does not flow the other way either.
    let mut beta_led = beta.ledger().unwrap();
    beta_led
        .append("run-x", StepKind::LlmRequest, "beta's own business")
        .unwrap();
    drop(beta_led);

    let alpha_led = alpha.ledger().unwrap();
    let log = alpha_led.log("run-x");
    assert_eq!(log.len(), 1, "alpha still holds only its own step");
    assert_eq!(log[0].payload, "alpha's secret business");

    // The isolation is a property of the storage shape: separate files, so a
    // cross-silo read has no handle to reach through.
    assert_ne!(alpha.ledger_path(), beta.ledger_path());
    assert!(alpha.ledger_path().is_file());
    assert!(beta.ledger_path().is_file());
}

#[test]
fn s4_silo_id_cannot_escape_the_root() {
    let (dir, root) = root();
    let outside = dir.path().join("outside");
    std::fs::create_dir(&outside).unwrap();

    for id in ["../outside", "..", ".", "", "a/b", "a\\b", "alpha/../.."] {
        let err = Silo::open(&root, id)
            .err()
            .unwrap_or_else(|| panic!("{id:?} must be refused"));
        assert!(
            matches!(err, SkeinError::Storage(_)),
            "{id:?} refused as storage error, got {err}"
        );
    }

    assert_eq!(
        std::fs::read_dir(&root).unwrap().count(),
        0,
        "a refused id creates nothing inside the root"
    );
    assert_eq!(
        std::fs::read_dir(&outside).unwrap().count(),
        0,
        "and nothing outside it"
    );
}

// ---- append-only, enforced by the engine (Constitution V) ----

#[test]
fn s5_the_store_refuses_update_and_delete() {
    let (_dir, root) = root();
    let silo = Silo::open(&root, "alpha").unwrap();
    let mut led = silo.ledger().unwrap();
    led.append("run-1", StepKind::LlmRequest, "original")
        .unwrap();
    drop(led);

    let raw = Connection::open(silo.ledger_path()).unwrap();
    for sql in [
        "UPDATE ledger_step SET payload = 'forged'",
        "DELETE FROM ledger_step",
    ] {
        let err = raw
            .execute(sql, [])
            .err()
            .unwrap_or_else(|| panic!("{sql} must be refused"));
        assert!(
            format!("{err}").contains("ledger is append-only"),
            "{sql}: {err}"
        );
    }
    drop(raw);

    let reopened = silo.ledger().unwrap();
    assert_eq!(reopened.log("run-1")[0].payload, "original");
}

#[test]
fn s6_row_level_tampering_is_detected_on_reopen() {
    let (_dir, root) = root();
    let silo = Silo::open(&root, "alpha").unwrap();
    let mut led = silo.ledger().unwrap();
    led.append("run-1", StepKind::LlmRequest, "original")
        .unwrap();
    led.append("run-1", StepKind::LlmResponse, "reply").unwrap();
    drop(led);

    // A local writer with the file can always drop a trigger. That is exactly
    // why the chain is hashed: this is tamper-evidence, not tamper-proofing.
    let raw = Connection::open(silo.ledger_path()).unwrap();
    raw.execute_batch("DROP TRIGGER ledger_step_no_update")
        .unwrap();
    raw.execute(
        "UPDATE ledger_step SET payload = 'forged' WHERE seq = 0",
        [],
    )
    .unwrap();
    drop(raw);

    let reopened = silo.ledger().unwrap();
    assert_eq!(reopened.log("run-1")[0].payload, "forged");
    let err = reopened
        .verify_chain("run-1")
        .expect_err("a forged row breaks the chain");
    assert!(
        matches!(err, SkeinError::LedgerIntegrity { .. }),
        "got {err}"
    );
}

// ---- the whole governed loop, unchanged, against a durable chain ----

#[test]
fn s7_a_full_governed_run_persists_and_reverifies() {
    let (_dir, root) = root();
    let silo = Silo::open(&root, "alpha").unwrap();

    let mut engine = NativeLoop::new(
        ScriptedModel {
            script: vec![
                reply(
                    "checking",
                    false,
                    vec![ToolCall::new("read_file", json!({"path": "a.txt"}))],
                ),
                reply("done", true, Vec::new()),
            ],
            calls: 0,
        },
        StaticProbe(true),
        ToolGateway::new(
            CountingTransport {
                calls: 0,
                reply: "file contents".into(),
            },
            ToolPolicy::new(vec![("read_file".into(), ToolAccess::ReadOnly)], Vec::new()),
            Redactor::new(Vec::new()),
        ),
    );

    let mut led = silo.ledger().unwrap();
    let mut ctl = LoopController::new(LoopBudget::new(10, 1_000, 3));
    let run = engine
        .run("run-1", Message::user_text("go"), &mut led, &mut ctl)
        .unwrap();
    assert_eq!(run.exit, Exit::FinalOutput);
    assert_eq!(engine.gateway.transport.calls, 1);
    let live = kinds(&led, "run-1");
    drop(led);

    let reopened = silo.ledger().unwrap();
    assert_eq!(
        kinds(&reopened, "run-1"),
        live,
        "the durable chain is the chain the run wrote, step for step"
    );
    assert!(live.contains(&StepKind::ToolResult));
    reopened
        .verify_chain("run-1")
        .expect("the reopened governed run verifies");
}
