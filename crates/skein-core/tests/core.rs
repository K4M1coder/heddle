//! skein-core v0 acceptance tests (TDD, ground-truth assertions).

use skein_core::{
    Content, Exit, Ledger, LedgerStore, LoopBudget, LoopController, Message, Redactor, Result,
    Role, SecretProvider, SecretRef, SecretValue, SkeinError, Step, StepKind,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
    let a = led
        .append("run-a", StepKind::LlmRequest, "prompt exact")
        .unwrap();
    let b = led
        .append("run-a", StepKind::LlmResponse, "raw response")
        .unwrap();
    led.append("run-b", StepKind::LlmRequest, "other run")
        .unwrap();

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
    led.append("run-t", StepKind::LlmRequest, "original")
        .unwrap();
    led.append("run-t", StepKind::ToolCall, "call").unwrap();
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

// ---- ledger store seam ----

/// A `LedgerStore` double: records what it was handed, and can be made to fail
/// on demand. The recording is shared so the test can read it after the store
/// has been moved into the `Ledger`.
#[derive(Clone, Default)]
struct VecStore {
    steps: Arc<Mutex<Vec<Step>>>,
    failing: Arc<AtomicBool>,
}

impl VecStore {
    fn loaded_with(steps: Vec<Step>) -> Self {
        VecStore {
            steps: Arc::new(Mutex::new(steps)),
            failing: Arc::new(AtomicBool::new(false)),
        }
    }

    fn recorded(&self) -> Vec<Step> {
        self.steps.lock().unwrap().clone()
    }
}

impl LedgerStore for VecStore {
    fn append(&mut self, step: &Step) -> skein_core::Result<()> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(SkeinError::Storage("store is down".into()));
        }
        self.steps.lock().unwrap().push(step.clone());
        Ok(())
    }

    fn load(&self) -> skein_core::Result<Vec<Step>> {
        Ok(self.recorded())
    }
}

#[test]
fn ledger_open_replays_an_existing_store() {
    let first = VecStore::default();
    let mut led = Ledger::open(Box::new(first.clone())).unwrap();
    led.append("run-p", StepKind::LlmRequest, "one").unwrap();
    let second_id = led.append("run-p", StepKind::LlmResponse, "two").unwrap();
    drop(led);

    // A different Ledger over a store holding the same rows: the chain is the
    // one that was persisted, not a fresh one.
    let mut reopened = Ledger::open(Box::new(VecStore::loaded_with(first.recorded()))).unwrap();
    let log = reopened.log("run-p");
    assert_eq!(log.len(), 2, "both persisted steps are replayed");
    assert_eq!(log[1].id, second_id);
    reopened
        .verify_chain("run-p")
        .expect("replayed chain verifies");

    let third = reopened.append("run-p", StepKind::Exit, "done").unwrap();
    let step = reopened.show(&third).unwrap();
    assert_eq!(step.seq, 2, "the reopened chain continues");
    assert_eq!(step.parent.as_deref(), Some(second_id.as_str()));
    reopened
        .verify_chain("run-p")
        .expect("continued chain verifies");
}

#[test]
fn ledger_append_writes_through_to_the_store() {
    let store = VecStore::default();
    let mut led = Ledger::open(Box::new(store.clone())).unwrap();

    let id = led.append("run-w", StepKind::ToolCall, "payload").unwrap();

    let recorded = store.recorded();
    assert_eq!(recorded.len(), 1, "the append reached the store");
    assert_eq!(
        &recorded[0],
        led.show(&id).unwrap(),
        "same step, both sides"
    );
}

#[test]
fn ledger_append_failure_leaves_the_chain_unmoved() {
    let store = VecStore::default();
    store.failing.store(true, Ordering::SeqCst);
    let mut led = Ledger::open(Box::new(store.clone())).unwrap();

    let err = led
        .append("run-f", StepKind::LlmRequest, "lost")
        .expect_err("a store that cannot persist must not report success");
    assert!(format!("{err}").contains("storage"), "{err}");
    assert!(led.log("run-f").is_empty(), "the mirror never moved");
    assert!(store.recorded().is_empty());

    // The seq/parent derivation reads the mirror, so a healed store continues
    // from where the chain actually is — no silently skipped step.
    store.failing.store(false, Ordering::SeqCst);
    let id = led.append("run-f", StepKind::LlmRequest, "kept").unwrap();
    let step = led.show(&id).unwrap();
    assert_eq!(step.seq, 0, "the failed append consumed no sequence number");
    assert_eq!(step.parent, None);
    led.verify_chain("run-f").unwrap();
}

// ---- secrets (§7.13) ----

/// A provider double: the one reference it knows resolves, everything else is a
/// miss. Standing in for an OS credential store, which `skein-silo` tests for
/// real — here the point is the seam, not the backend.
struct FakeProvider {
    known: (SecretRef, String),
}

impl SecretProvider for FakeProvider {
    fn resolve(&self, r: &SecretRef) -> Result<SecretValue> {
        if r == &self.known.0 {
            Ok(SecretValue::new(self.known.1.clone()))
        } else {
            Err(SkeinError::Secret(format!("no such secret: {}", r.0)))
        }
    }

    fn requires_network(&self) -> bool {
        false
    }
}

#[test]
fn secret_value_never_prints_itself() {
    let v = SecretValue::new("hunter2");
    assert_eq!(v.expose(), "hunter2", "the one explicit way to read it");
    assert!(
        !format!("{:?}", v).contains("hunter2"),
        "a derived Debug on any struct holding one must not leak it"
    );
}

#[test]
fn redactor_resolves_from_a_provider() {
    let r = SecretRef("keychain://skein/test".into());
    let provider = FakeProvider {
        known: (r.clone(), "hunter2".into()),
    };

    let redactor =
        Redactor::resolve(&provider, std::slice::from_ref(&r)).expect("a known reference resolves");

    assert_eq!(redactor.redact("token=hunter2"), "token=***");
}

#[test]
fn redactor_resolve_propagates_a_provider_failure() {
    let provider = FakeProvider {
        known: (SecretRef("keychain://skein/test".into()), "hunter2".into()),
    };

    let err = Redactor::resolve(&provider, &[SecretRef("keychain://skein/absent".into())])
        .err()
        .expect("a misconfigured reference must fail loudly");

    assert!(
        matches!(err, SkeinError::Secret(_)),
        "a redactor that scrubs nothing is worse than no redactor: {err}"
    );
}
