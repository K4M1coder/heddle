//! Acceptance tests for the governed Tool Gateway (spec 005).
//! Every property is driven through real wiring: a `ToolGateway` over a counting
//! transport double, writing into a real hash-chained `Ledger`.

use serde_json::json;
use skein_core::{
    replay_tool_calls, Ledger, Redactor, Result, SkeinError, StepKind, ToolAccess, ToolCall,
    ToolGateway, ToolOutcome, ToolPolicy, ToolSpec, ToolTransport,
};

const SECRET: &str = "sk-SECRET-abc123";

/// A transport whose every reply is decided in advance. Counts calls so tests can
/// prove the governor never let one through — and never made one on replay.
struct CountingTransport {
    calls: usize,
    seen: Vec<ToolCall>,
    reply: String,
    fail: bool,
    catalogue: Vec<ToolSpec>,
}

impl CountingTransport {
    fn new(reply: &str) -> Self {
        CountingTransport {
            calls: 0,
            seen: Vec::new(),
            reply: reply.to_string(),
            fail: false,
            catalogue: Vec::new(),
        }
    }

    fn failing() -> Self {
        CountingTransport {
            fail: true,
            ..CountingTransport::new("")
        }
    }

    fn offering(names: &[&str]) -> Self {
        CountingTransport {
            catalogue: names.iter().map(|name| spec(name)).collect(),
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

    fn list(&mut self) -> Result<Vec<ToolSpec>> {
        if self.fail {
            return Err(SkeinError::Tool("downstream exploded".into()));
        }
        Ok(self.catalogue.clone())
    }
}

/// A catalogue entry as a server would derive it: a distinct schema per tool, so
/// an assertion on the advertised specs proves the server's own document
/// travelled rather than a name the gateway could have reconstructed from the
/// policy.
fn spec(name: &str) -> ToolSpec {
    ToolSpec::new(
        name,
        format!("what {name} does"),
        json!({"type": "object", "properties": {name: {"type": "string"}}}),
    )
}

/// The nine `impl ToolTransport` sites in the tree that predate advertisement
/// all look like this one: `call` and nothing else. What they offer a model is
/// the property under test.
struct UnlistedTransport;

impl ToolTransport for UnlistedTransport {
    fn call(&mut self, _call: &ToolCall) -> Result<ToolOutcome> {
        panic!("this test never calls a tool")
    }
}

#[test]
fn a_transport_that_does_not_override_list_offers_nothing() {
    // The inverse of `NativeLoop::new`'s required `Redactor`: there the silent
    // default would have been the unsafe one, so it was refused; here it is the
    // safe one. Inheriting this body is deny-by-default.
    assert_eq!(
        UnlistedTransport.list().expect("the default body succeeds"),
        Vec::new()
    );
}

fn policy(approved: &[&str]) -> ToolPolicy {
    ToolPolicy::new(
        vec![
            ("fs_write".into(), ToolAccess::Mutating),
            ("read_secret".into(), ToolAccess::ReadOnly),
            ("read_other".into(), ToolAccess::ReadOnly),
        ],
        approved.iter().map(|s| s.to_string()).collect(),
    )
}

fn gateway(transport: CountingTransport, approved: &[&str]) -> ToolGateway<CountingTransport> {
    ToolGateway::new(
        transport,
        policy(approved),
        Redactor::new(vec![SECRET.into()]),
    )
}

/// `gateway`, with the redacted material chosen by the caller: the redaction
/// tests turn on which *forms* of a secret are found, so the secret itself is
/// the variable.
fn gateway_scrubbing(transport: CountingTransport, secret: &str) -> ToolGateway<CountingTransport> {
    ToolGateway::new(transport, policy(&[]), Redactor::new(vec![secret.into()]))
}

#[test]
fn advertisement_is_the_allowlist_intersected_with_the_catalogue_in_allowlist_order() {
    // Wider than the policy, in a different order, and missing one allowlisted
    // name — the three ways a catalogue and an allowlist can disagree.
    let mut gw = gateway(
        CountingTransport::offering(&["read_other", "banned_tool", "read_secret"]),
        &[],
    );

    let advertised = gw.advertise().expect("the catalogue is readable");

    assert_eq!(
        advertised,
        vec![spec("read_secret"), spec("read_other")],
        "allowlist order, not catalogue order: the operator's list is the authority"
    );
    assert!(
        !advertised.iter().any(|s| s.name == "fs_write"),
        "an allowlisted tool the server does not offer is absent, never fabricated \
         from the policy: {advertised:?}"
    );
}

#[test]
fn an_unapproved_mutating_tool_is_still_advertised() {
    // Deliberate, and pinned so nobody "tightens" it later. `call_captured`
    // consults the policy *before* the transport, so a mutating tool withheld at
    // advertisement would never reach the ACP permission prompt — which is the
    // only path to a human. It is advertised, and refused at call time with a
    // reason the model is told.
    let mut led = Ledger::new();
    let mut gw = gateway(CountingTransport::offering(&["fs_write"]), &[]);

    assert_eq!(
        gw.advertise().expect("the catalogue is readable"),
        vec![spec("fs_write")]
    );

    let err = gw
        .call("run-t11", &ToolCall::new("fs_write", json!({})), &mut led)
        .expect_err("advertised is not approved");
    assert!(
        matches!(err, SkeinError::ToolDenied { ref reason, .. } if reason.contains("approval")),
        "the refusal names its reason, and the model is told it: {err:?}"
    );
    assert_eq!(gw.transport.calls, 0);
}

#[test]
fn a_catalogue_that_cannot_be_read_is_an_error_not_an_empty_advertisement() {
    let mut gw = gateway(CountingTransport::failing(), &[]);

    let err = gw
        .advertise()
        .expect_err("an unreadable inventory leaves the run's capabilities unknown");
    assert!(matches!(err, SkeinError::Tool(_)), "{err:?}");
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

/// The three characters `serde_json` escapes inside a string, in one secret. A
/// tool result is already-serialized JSON, so this is the form the secret is
/// actually on it in — and `Redactor::redact_wire` derives its escaped needle
/// from `Value::String`, so one secret carrying all three proves the derivation
/// rather than one character of it.
const AWKWARD: &str = "pa\"ss\\wo\nrd";

/// A `CallToolResult` as `skein-mcp` hands one over: the whole result,
/// serialized (`skein-mcp/src/lib.rs`). `text` is the decoded string the tool
/// produced, so a secret is as written here and escaped by this serialization.
fn wire_result(text: &str) -> String {
    serde_json::to_string(&json!({
        "content": [{"type": "text", "text": text}],
        "isError": false,
    }))
    .expect("a result body serializes")
}

/// The decoded text inside a serialized result body — the only form in which a
/// secret appears as written, and what a replay consumer actually reads.
///
/// Every assertion about an awkward secret goes through here, in **both**
/// directions, because a `contains` over the serialized form is meaningless in
/// either. On the raw outcome the secret is escaped once, so the literal needle
/// misses a secret that is present; on the `ToolResult` step payload it is
/// escaped twice — `content` is itself serialized JSON inside a serialized
/// `CapturedResult` — so neither the literal nor a singly-escaped needle
/// matches, and the naive assertion is green while the secret is in plain sight.
/// Parsing back down to the string is what makes either assertion real, and it
/// proves the body is still parseable at the same time.
fn body_text(body: &str) -> String {
    let parsed: serde_json::Value =
        serde_json::from_str(body).expect("a result body is still the JSON it was");
    parsed["content"][0]["text"]
        .as_str()
        .expect("scrubbing leaves the body's shape intact")
        .to_string()
}

fn captured_text(led: &Ledger, run_id: &str) -> String {
    body_text(&replay_tool_calls(led, run_id).expect("the run replays")[0].content)
}

#[test]
fn an_awkward_secret_is_redacted_from_a_wire_shaped_result() {
    let mut led = Ledger::new();
    let text = format!("api_key={AWKWARD}\nendpoint=localhost");
    let mut gw = gateway_scrubbing(CountingTransport::new(&wire_result(&text)), AWKWARD);

    let call = ToolCall::new("read_secret", json!({ "token": AWKWARD }));
    let out = gw
        .call("run-t12", &call, &mut led)
        .expect("read_secret runs");

    assert_eq!(
        captured_text(&led, "run-t12"),
        "api_key=***\nendpoint=localhost",
        "a secret containing a quote, a backslash or a newline is on an \
         already-serialized result in escaped form, so finding it needs the wire premise"
    );
    assert_eq!(
        body_text(&out.content),
        text,
        "the raw secret must still reach the trusted caller"
    );
    assert_eq!(
        gw.transport.seen[0].args,
        json!({ "token": AWKWARD }),
        "the transport must receive the raw arguments, not the redacted ones"
    );
    led.verify_chain("run-t12").expect("chain verifies");
}

#[test]
fn a_secret_with_nothing_to_escape_is_captured_byte_for_byte_as_before() {
    // The escaped needle is added only when it differs from the literal one, so
    // every run that was already scrubbed correctly must be untouched by the
    // wire premise. Asserted on the whole `content`, not on its decoded text:
    // byte-identical is the claim.
    let mut led = Ledger::new();
    let reply = wire_result(&format!("api_key={SECRET}"));
    let mut gw = gateway_scrubbing(CountingTransport::new(&reply), SECRET);

    gw.call(
        "run-t13",
        &ToolCall::new("read_secret", json!({})),
        &mut led,
    )
    .expect("read_secret runs");

    assert_eq!(
        replay_tool_calls(&led, "run-t13").expect("the run replays")[0].content,
        wire_result("api_key=***")
    );
}

#[test]
fn an_awkward_secret_in_the_arguments_is_still_scrubbed_by_the_literal_needle() {
    // The other half of the same function, and deliberately *not* changed with
    // it. `redact_call` scrubs the `Value` and `call_captured` serializes
    // afterwards, so here the needle really is the secret as written. Pinned so
    // a later edit making the two lines "consistent" fails rather than passes.
    let mut led = Ledger::new();
    let mut gw = gateway_scrubbing(CountingTransport::new(&wire_result("nothing")), AWKWARD);

    gw.call(
        "run-t14",
        &ToolCall::new("read_secret", json!({ "token": AWKWARD })),
        &mut led,
    )
    .expect("read_secret runs");

    let attempt: serde_json::Value = serde_json::from_str(&led.log("run-t14")[0].payload)
        .expect("the ToolCall step is a captured call");
    assert_eq!(attempt["args"], json!({ "token": "***" }));
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

#[test]
fn a_secret_in_a_tool_call_id_is_redacted_from_the_attempt() {
    let mut led = Ledger::new();
    let mut gw = gateway(CountingTransport::new("contents"), &["read_secret"]);

    // The id is not ours whenever a provider supplies one: `OpenAiCompatClient`
    // forwards the provider's id verbatim if it is non-empty, so a compromised
    // endpoint can echo an operator secret into this field exactly as it can
    // into the name or the arguments.
    let id = format!("call_{SECRET}");
    gw.call(
        "run-t12",
        &ToolCall::with_id(&id, "read_secret", json!({})),
        &mut led,
    )
    .expect("read_secret is allowlisted and runs");

    assert_eq!(
        gw.transport.seen[0].id, id,
        "the transport must receive the raw id, not the redacted one"
    );

    let payloads: Vec<String> = led
        .log("run-t12")
        .iter()
        .map(|s| s.payload.clone())
        .collect();
    assert!(
        payloads.iter().all(|p| !p.contains(SECRET)),
        "no captured payload may contain the secret: {payloads:?}"
    );
    assert!(
        payloads[0].contains("call_***"),
        "the attempt names the call by its scrubbed id: {}",
        payloads[0]
    );
}
