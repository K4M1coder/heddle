//! skein-core v0 acceptance tests (TDD, ground-truth assertions).

use skein_core::{Content, Exit, Ledger, LoopBudget, LoopController, Message, Role, StepKind};

// ---- content ----

#[test]
fn message_roundtrips_and_reads_text() {
    let m = Message::user_text("hello");
    assert_eq!(m.role, Role::User);
    assert_eq!(m.text(), "hello");
    let json = serde_json::to_string(&m).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(back, m);
    assert!(matches!(back.parts[0], Content::Text { .. }));
}

// ---- ledger (§4.11) ----

#[test]
fn ledger_appends_hash_chained_and_isolated_by_run() {
    let mut led = Ledger::new();
    let a = led.append("run-a", StepKind::LlmRequest, "prompt exact");
    let b = led.append("run-a", StepKind::LlmResponse, "raw response");
    led.append("run-b", StepKind::LlmRequest, "other run");

    let log = led.log("run-a");
    assert_eq!(log.len(), 2, "run isolation: only run-a steps");
    assert_eq!(log[0].parent, None);
    assert_eq!(log[1].parent.as_deref(), Some(a.as_str()));
    assert_eq!(log[1].id, b);
    // show returns exact payload (in/out captured, not just results).
    assert_eq!(led.show(&a).unwrap().payload, "prompt exact");
    assert_eq!(led.show(&b).unwrap().payload, "raw response");
    led.verify_chain("run-a").unwrap();
}

#[test]
fn ledger_detects_tampering() {
    let mut led = Ledger::new();
    led.append("run-t", StepKind::LlmRequest, "original");
    led.append("run-t", StepKind::ToolCall, "call");
    led.verify_chain("run-t").expect("intact chain verifies");

    // Mutate an earlier payload → the recomputed hash no longer matches.
    led.tamper_payload_for_test("run-t", 0, "forged");
    let err = led.verify_chain("run-t").unwrap_err();
    assert!(
        format!("{err}").contains("integrity"),
        "tampering is detected: {err}"
    );
}

// ---- loop controller (§4.14, Constitution VIII) ----

#[test]
fn loop_stops_on_iteration_budget_not_on_model_will() {
    let mut ctl = LoopController::new(LoopBudget::new(3, 1_000_000, 10));
    for _ in 0..3 {
        assert_eq!(ctl.should_exit(false), None, "keeps going within budget");
        ctl.record_iteration(10, true);
    }
    // Model would continue (final_output=false) but the engine stops it.
    assert_eq!(ctl.should_exit(false), Some(Exit::MaxIters));
}

#[test]
fn loop_stops_on_no_progress_ground_truth() {
    let mut ctl = LoopController::new(LoopBudget::new(100, 1_000_000, 2));
    ctl.record_iteration(5, false); // no external progress
    assert_eq!(ctl.should_exit(false), None);
    ctl.record_iteration(5, false); // second stale iteration hits the limit
    assert_eq!(ctl.should_exit(false), Some(Exit::NoProgress));
    // Progress resets staleness.
    let mut ctl2 = LoopController::new(LoopBudget::new(100, 1_000_000, 2));
    ctl2.record_iteration(5, false);
    ctl2.record_iteration(5, true);
    ctl2.record_iteration(5, false);
    assert_eq!(
        ctl2.should_exit(false),
        None,
        "progress cleared the stale counter"
    );
}

#[test]
fn loop_honors_final_output_and_token_budget() {
    let mut ctl = LoopController::new(LoopBudget::new(100, 50, 10));
    assert_eq!(
        ctl.should_exit(true),
        Some(Exit::FinalOutput),
        "allowed model stop"
    );
    ctl.record_iteration(60, true);
    assert_eq!(ctl.should_exit(false), Some(Exit::MaxTokens));
}
