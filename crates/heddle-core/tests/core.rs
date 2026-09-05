//! heddle-core v0 acceptance tests (TDD, ground-truth assertions).

use heddle_core::{
    Content, Exit, HeddleError, Ledger, LedgerStore, LoopBudget, LoopController, Message, Redactor,
    Result, Role, SecretProvider, SecretRef, SecretValue, Step, StepKind, ToolSpec, TurnRequest,
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
    fn append(&mut self, step: &Step) -> heddle_core::Result<()> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(HeddleError::Storage("store is down".into()));
        }
        self.steps.lock().unwrap().push(step.clone());
        Ok(())
    }

    fn load(&self) -> heddle_core::Result<Vec<Step>> {
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
/// miss. Standing in for an OS credential store, which `heddle-silo` tests for
/// real — here the point is the seam, not the backend.
struct FakeProvider {
    known: (SecretRef, String),
}

impl SecretProvider for FakeProvider {
    fn resolve(&self, r: &SecretRef) -> Result<SecretValue> {
        if r == &self.known.0 {
            Ok(SecretValue::new(self.known.1.clone()))
        } else {
            Err(HeddleError::Secret(format!("no such secret: {}", r.0)))
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
    let r = SecretRef("keychain://heddle/test".into());
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
        known: (SecretRef("keychain://heddle/test".into()), "hunter2".into()),
    };

    let err = Redactor::resolve(&provider, &[SecretRef("keychain://heddle/absent".into())])
        .err()
        .expect("a misconfigured reference must fail loudly");

    assert!(
        matches!(err, HeddleError::Secret(_)),
        "a redactor that scrubs nothing is worse than no redactor: {err}"
    );
}

/// A secret carrying a quote and a newline: serialized first, both are
/// JSON-escaped, so a string-level replace on the serialized payload would never
/// see this needle.
const AWKWARD: &str = "a\"b\nc";

#[test]
fn redact_json_scrubs_the_strings_and_keeps_the_shape() {
    let redactor = Redactor::new(vec!["hunter2".into()]);
    let awkward = Redactor::new(vec![AWKWARD.into()]);
    let value = serde_json::json!({
        "token": "hunter2",
        "nested": {"hunter2": ["prefix hunter2 suffix", 7, true, null]},
        "awkward": AWKWARD,
    });

    let scrubbed = redactor.redact_json(&value).expect("a Value serializes");
    let back: serde_json::Value =
        serde_json::from_str(&scrubbed).expect("the payload still parses");

    assert_eq!(back["token"], serde_json::json!("***"));
    assert_eq!(
        back["nested"]["***"],
        serde_json::json!(["prefix *** suffix", 7, true, null]),
        "keys are scrubbed too, and non-string scalars survive unchanged: {back}"
    );
    assert!(!scrubbed.contains("hunter2"), "{scrubbed}");

    let escaped = awkward.redact_json(&value).expect("a Value serializes");
    let back: serde_json::Value = serde_json::from_str(&escaped).expect("the payload still parses");
    assert_eq!(
        back["awkward"],
        serde_json::json!("***"),
        "a secret containing a quote and a newline is scrubbed before it is escaped: {escaped}"
    );
}

#[test]
fn a_cloned_redactor_scrubs_what_the_original_scrubs() {
    let original = Redactor::new(vec!["hunter2".into()]);
    let copy = original.clone();

    assert_eq!(copy.redact("token=hunter2"), "token=***");
    assert_eq!(
        original.redact("token=hunter2"),
        copy.redact("token=hunter2"),
        "one run configures one secret set, however many collaborators hold it"
    );
}

// ---- tool advertisement (§4.3) ----

#[test]
fn tool_spec_roundtrips_through_a_ledger_payload() {
    let spec = ToolSpec::new(
        "fs_read",
        "Read a UTF-8 text file under the configured root.",
        serde_json::json!({
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"],
        }),
    );
    let mut led = Ledger::new();
    led.append(
        "run-spec",
        StepKind::LlmRequest,
        serde_json::to_string(&vec![spec.clone()]).unwrap(),
    )
    .unwrap();

    let back: Vec<ToolSpec> = serde_json::from_str(&led.log("run-spec")[0].payload).unwrap();
    assert_eq!(back, vec![spec]);
    // The schema is the server's document, carried opaquely: a payload that lost
    // `required` would still deserialize into a `ToolSpec`, so the assertion is
    // on the schema itself rather than on the wrapper.
    assert_eq!(back[0].parameters["required"], serde_json::json!(["path"]));
}

#[test]
fn a_turn_request_advertising_nothing_serializes_no_tools_key() {
    // The property that keeps this slice invisible to every existing chain and
    // every existing wire assertion: a run with no tools puts the same bytes in
    // the Ledger it put there before `tools` existed.
    let bare = TurnRequest {
        run_id: "run-bare".into(),
        messages: vec![Message::user_text("hello")],
        tools: Vec::new(),
    };
    assert_eq!(
        serde_json::to_string(&bare).unwrap(),
        r#"{"run_id":"run-bare","messages":[{"role":"user","parts":[{"type":"text","text":"hello"}]}]}"#
    );

    // And the other direction, so a chain written before the field existed still
    // replays: a payload with no `tools` key deserializes to an empty list.
    let back: TurnRequest = serde_json::from_str(&serde_json::to_string(&bare).unwrap()).unwrap();
    assert_eq!(back, bare);
}

// ---- ledger run enumeration ----

#[test]
fn ledger_runs_lists_run_ids_in_first_append_order() {
    let mut led = Ledger::new();
    led.append("run-b", StepKind::LlmRequest, "first").unwrap();
    led.append("run-a", StepKind::LlmRequest, "second").unwrap();
    led.append("run-b", StepKind::LlmResponse, "third").unwrap();

    assert_eq!(
        led.runs(),
        vec!["run-b", "run-a"],
        "a run is listed once, at the position of its first append"
    );
}

// ---- an engine-stopped run has its own name ----

#[test]
fn an_unfinished_run_names_the_run_and_the_exit_that_stopped_it() {
    // A budget exit is not a provider failure: `HeddleError::Model` would print
    // `model provider:` for a decision no provider made, and `Storage` is a
    // lie. A client needs to say which run the engine stopped, and why.
    let err = HeddleError::Unfinished {
        run_id: "chat-1756000000000-4242".into(),
        exit: format!("{:?}", Exit::MaxIters),
    };

    assert_eq!(
        err.to_string(),
        "run chat-1756000000000-4242 ended without a final answer: MaxIters"
    );
}

// ---- a protocol adapter's transport has its own name ----

#[test]
fn a_protocol_failure_names_the_adapter_rather_than_the_model_or_the_tool() {
    // The ACP or MCP connection itself failing is neither a provider decision
    // nor a tool effect. `HeddleError::Model` would blame a model that was never
    // reached, and `HeddleError::Tool` — which is what the ACP permission
    // transport legitimately uses — would print `tool transport:` for a broken
    // stdio pipe that carried no tool call at all.
    let err = HeddleError::Protocol("acp stdio: connection reset".into());

    assert_eq!(err.to_string(), "protocol: acp stdio: connection reset");
}
