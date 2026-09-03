//! Acceptance tests for the governed Tool Gateway (spec 005).
//! Every property is driven through real wiring: a `ToolGateway` over a counting
//! transport double, writing into a real hash-chained `Ledger`.

use serde_json::json;
use skein_core::{
    replay_tool_calls, Ledger, Redactor, Result, SkeinError, StepKind, ToolAccess, ToolCall,
    ToolGateway, ToolOutcome, ToolPolicy, ToolTransport,
};

const SECRET: &str = "sk-SECRET-abc123";

/// A transport whose every reply is decided in advance. Counts calls so tests can
/// prove the governor never let one through — and never made one on replay.
struct CountingTransport {
    calls: usize,
    seen: Vec<ToolCall>,
    reply: String,
    fail: bool,
}

impl CountingTransport {
    fn new(reply: &str) -> Self {
        CountingTransport {
            calls: 0,
            seen: Vec::new(),
            reply: reply.to_string(),
            fail: false,
        }
    }

    fn failing() -> Self {
        CountingTransport {
            fail: true,
            ..CountingTransport::new("")
        }
    }
}

impl ToolTransport for CountingTransport {
    fn call(&mut self, call: &ToolCall) -> Result<ToolOutcome> {
        self.calls += 1;
        self.seen.push(call.clone());
        if self.fail {
            return Err(SkeinError::Tool("downstream exploded".into()));
        }
        Ok(ToolOutcome {
            content: self.reply.clone(),
        })
    }
}

fn gateway(transport: CountingTransport, approved: &[&str]) -> ToolGateway<CountingTransport> {
    ToolGateway::new(
        transport,
        ToolPolicy::new(
            vec![
                ("fs_write".into(), ToolAccess::Mutating),
                ("read_secret".into(), ToolAccess::ReadOnly),
                ("read_other".into(), ToolAccess::ReadOnly),
            ],
            approved.iter().map(|s| s.to_string()).collect(),
        ),
        Redactor::new(vec![SECRET.into()]),
    )
}

fn kinds(led: &Ledger, run_id: &str) -> Vec<StepKind> {
    led.log(run_id).iter().map(|s| s.kind.clone()).collect()
}

#[test]
fn denied_mutating_tool_never_reaches_the_transport() {
    let mut led = Ledger::new();
    let mut gw = gateway(CountingTransport::new("wrote file"), &[]);

    let err = gw
        .call("run-t1", &ToolCall::new("fs_write", json!({})), &mut led)
        .expect_err("a mutating tool without approval must be denied");

    assert!(
        matches!(err, SkeinError::ToolDenied { ref tool, .. } if tool == "fs_write"),
        "expected ToolDenied for fs_write, got {err:?}"
    );
    assert_eq!(gw.transport.calls, 0, "the transport must never be touched");
    assert_eq!(
        kinds(&led, "run-t1"),
        vec![StepKind::ToolCall, StepKind::Approval],
        "the attempt and the refusal are both on the record"
    );
    let approval = led.log("run-t1")[1].payload.clone();
    assert!(
        approval.contains("denied"),
        "approval step must record the refusal: {approval}"
    );
    led.verify_chain("run-t1").expect("chain verifies");
}

#[test]
fn approved_mutating_tool_executes_once() {
    let mut led = Ledger::new();
    let mut gw = gateway(CountingTransport::new("wrote file"), &["fs_write"]);

    let out = gw
        .call("run-t2", &ToolCall::new("fs_write", json!({})), &mut led)
        .expect("an approved mutating tool runs");

    assert_eq!(out.content, "wrote file");
    assert_eq!(gw.transport.calls, 1, "executed exactly once");
    assert_eq!(
        kinds(&led, "run-t2"),
        vec![StepKind::ToolCall, StepKind::Approval, StepKind::ToolResult]
    );
    led.verify_chain("run-t2").expect("chain verifies");
}

#[test]
fn secret_is_redacted_from_args_and_result_before_capture() {
    let mut led = Ledger::new();
    let mut gw = gateway(
        CountingTransport::new(&format!("config: api_key={SECRET}")),
        &[],
    );

    let call = ToolCall::new("read_secret", json!({ "token": SECRET }));
    let out = gw
        .call("run-t4", &call, &mut led)
        .expect("read_secret runs");

    assert!(
        out.content.contains(SECRET),
        "the raw secret must still reach the trusted caller"
    );
    assert_eq!(
        gw.transport.seen[0].args,
        json!({ "token": SECRET }),
        "the transport must receive the raw arguments, not the redacted ones"
    );

    // Scan every payload of the run, not only the ToolResult step: this is the
    // assertion that would catch a future step type leaking.
    let payloads: Vec<String> = led
        .log("run-t4")
        .iter()
        .map(|s| s.payload.clone())
        .collect();
    assert!(
        payloads.iter().all(|p| !p.contains(SECRET)),
        "no captured payload may contain the secret: {payloads:?}"
    );
    assert!(
        payloads.iter().any(|p| p.contains("***")),
        "the secret must be replaced by the redaction marker: {payloads:?}"
    );
}

#[test]
fn replay_reconstructs_results_without_a_transport() {
    let mut led = Ledger::new();
    let mut gw = gateway(CountingTransport::new(&format!("value={SECRET}")), &[]);

    gw.call("run-t5", &ToolCall::new("read_secret", json!({})), &mut led)
        .unwrap();
    gw.call("run-t5", &ToolCall::new("read_other", json!({})), &mut led)
        .unwrap();
    let calls_after_live = gw.transport.calls;
    assert_eq!(calls_after_live, 2);

    let replayed = replay_tool_calls(&led, "run-t5").expect("replay reads the record");

    assert_eq!(
        gw.transport.calls, calls_after_live,
        "replay must make no new downstream call"
    );
    assert_eq!(
        replayed.iter().map(|r| r.tool.as_str()).collect::<Vec<_>>(),
        vec!["read_secret", "read_other"],
        "results come back in order"
    );
    assert!(replayed
        .iter()
        .all(|r| r.content.contains("***") && !r.content.contains(SECRET)));
}

#[test]
fn transport_error_leaves_the_chain_verifiable() {
    let mut led = Ledger::new();
    let mut gw = gateway(CountingTransport::failing(), &[]);

    let err = gw
        .call("run-t6", &ToolCall::new("read_secret", json!({})), &mut led)
        .expect_err("a failing transport propagates");

    assert!(
        matches!(err, SkeinError::Tool(_)),
        "expected a transport error, got {err:?}"
    );
    assert_eq!(
        kinds(&led, "run-t6"),
        vec![StepKind::ToolCall, StepKind::Approval],
        "no ToolResult may be fabricated for a call that produced none"
    );
    led.verify_chain("run-t6").expect("chain verifies");
}

#[test]
fn governed_calls_extend_one_hash_chain() {
    let mut led = Ledger::new();
    let mut gw = gateway(CountingTransport::new("ok"), &["fs_write"]);

    led.append(
        "run-t7",
        StepKind::LlmResponse,
        "the model asked for a tool",
    )
    .unwrap();
    gw.call("run-t7", &ToolCall::new("fs_write", json!({})), &mut led)
        .unwrap();
    led.append("run-t7", StepKind::LlmResponse, "the model saw the result")
        .unwrap();
    gw.call("run-t7", &ToolCall::new("read_secret", json!({})), &mut led)
        .unwrap();

    led.verify_chain("run-t7")
        .expect("the gateway writes into the one chain, not a parallel log");
    let seqs: Vec<u64> = led.log("run-t7").iter().map(|s| s.seq).collect();
    assert_eq!(seqs, (0..seqs.len() as u64).collect::<Vec<_>>());
}

#[test]
fn a_secret_in_a_tool_name_is_redacted_from_the_attempt_and_the_approval() {
    let mut led = Ledger::new();
    let mut gw = gateway(CountingTransport::new("pwned"), &[]);

    // The tool name is model-chosen text, so it carries an echoed secret exactly
    // as the arguments do.
    let name = format!("read_{SECRET}");
    let err = gw
        .call("run-t11", &ToolCall::new(&name, json!({})), &mut led)
        .expect_err("a tool nobody allowlisted must be denied");

    assert!(
        matches!(err, SkeinError::ToolDenied { ref tool, .. } if tool == &name),
        "the trusted caller is still told which name was refused, raw: {err:?}"
    );
    assert_eq!(gw.transport.calls, 0, "the transport must never be touched");

    let payloads: Vec<String> = led
        .log("run-t11")
        .iter()
        .map(|s| s.payload.clone())
        .collect();
    assert!(
        payloads.iter().all(|p| !p.contains(SECRET)),
        "no captured payload may contain the secret: {payloads:?}"
    );

    let approval = led.log("run-t11")[1].payload.clone();
    assert!(
        approval.contains("read_***") && approval.contains("denied"),
        "the policy decided on the raw name and the record holds the scrubbed one: {approval}"
    );
}

// ---- deny-by-default for tool identity (spec 007) ----

#[test]
fn unlisted_tool_is_denied_even_though_it_is_not_mutating() {
    let mut led = Ledger::new();
    let mut gw = gateway(CountingTransport::new("pwned"), &[]);

    let err = gw
        .call("run-t8", &ToolCall::new("shell_exec", json!({})), &mut led)
        .expect_err("a tool nobody allowlisted must be denied, mutating or not");

    assert!(
        matches!(err, SkeinError::ToolDenied { ref tool, .. } if tool == "shell_exec"),
        "expected ToolDenied for shell_exec, got {err:?}"
    );
    assert_eq!(gw.transport.calls, 0, "the transport must never be touched");
    assert_eq!(
        kinds(&led, "run-t8"),
        vec![StepKind::ToolCall, StepKind::Approval],
        "the attempt and the refusal are both on the record"
    );
    let approval = led.log("run-t8")[1].payload.clone();
    assert!(
        approval.contains("denied"),
        "approval step must record the refusal: {approval}"
    );
    led.verify_chain("run-t8").expect("chain verifies");
}

#[test]
fn allowlisted_read_only_tool_runs_without_approval() {
    let mut led = Ledger::new();
    let mut gw = gateway(CountingTransport::new("contents"), &[]);

    let out = gw
        .call("run-t9", &ToolCall::new("read_other", json!({})), &mut led)
        .expect("an allowlisted read-only tool needs no approval");

    assert_eq!(out.content, "contents");
    assert_eq!(gw.transport.calls, 1, "executed exactly once");
    assert_eq!(
        kinds(&led, "run-t9"),
        vec![StepKind::ToolCall, StepKind::Approval, StepKind::ToolResult]
    );
    let approval = led.log("run-t9")[1].payload.clone();
    assert!(
        approval.contains("allowed"),
        "approval step must record the permission: {approval}"
    );
}

#[test]
fn approval_alone_does_not_admit_a_tool_missing_from_the_allowlist() {
    let mut led = Ledger::new();
    let mut gw = gateway(CountingTransport::new("pwned"), &["shell_exec"]);

    let err = gw
        .call("run-t10", &ToolCall::new("shell_exec", json!({})), &mut led)
        .expect_err("identity is checked before, and independently of, approval");

    assert!(
        matches!(err, SkeinError::ToolDenied { ref tool, .. } if tool == "shell_exec"),
        "expected ToolDenied for shell_exec, got {err:?}"
    );
    assert_eq!(gw.transport.calls, 0, "the transport must never be touched");
}
