//! Pre-registered exit criteria for Spike 1, Option A (native loop).
//! Ground truth: observable assertions, no self-judgment.

use opt_a_native::{run_loop, Event, Exit, Verdict};
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Exact bodies the stub model returns (raw-capture comparisons need byte equality).
const TOOL_TURN_BODY: &str = r#"{"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"echo","arguments":"{\"x\":42}"}}]}}]}"#;
const FINAL_TURN_BODY: &str = r#"{"choices":[{"message":{"role":"assistant","content":"done"}}]}"#;

async fn stub_two_turn_model() -> MockServer {
    let server = MockServer::start().await;
    // First call → tool_calls; subsequent → final answer.
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(TOOL_TURN_BODY))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(FINAL_TURN_BODY))
        .mount(&server)
        .await;
    server
}

fn allow_all() -> Box<dyn Fn(&str, &serde_json::Value) -> Verdict + Send + Sync> {
    Box::new(|_, _| Verdict::Allow)
}

/// CRITERION 1 — exact model I/O capture, per turn.
#[tokio::test]
async fn criterion_1_captures_exact_model_io() {
    let server = stub_two_turn_model().await;
    let out = run_loop(&server.uri(), "hello", "run-c1", 5, allow_all(), CancellationToken::new()).await;
    assert_eq!(out.exit, Exit::FinalOutput);

    // Request side: first event is the EXACT payload we sent (verifiable JSON).
    match &out.events[0] {
        Event::LlmRequest { payload, .. } => {
            assert_eq!(payload["messages"][0], json!({"role":"user","content":"hello"}));
            assert_eq!(payload["model"], "spike-model");
            assert!(payload["tools"].is_array());
        }
        other => panic!("first event must be LlmRequest, got {other:?}"),
    }
    // Response side: raw body captured BYTE-EXACT (not a lossy parse).
    let raws: Vec<&str> = out.events.iter().filter_map(|e| match e {
        Event::LlmResponse { raw, .. } => Some(raw.as_str()),
        _ => None,
    }).collect();
    assert_eq!(raws, vec![TOOL_TURN_BODY, FINAL_TURN_BODY]);

    // Second turn's request must contain the tool result (loop actually fed back).
    let second_req = out.events.iter().find_map(|e| match e {
        Event::LlmRequest { seq, payload, .. } if *seq > 0 => Some(payload),
        _ => None,
    }).expect("second LlmRequest");
    let msgs = second_req["messages"].as_array().unwrap();
    assert!(msgs.iter().any(|m| m["role"] == "tool"), "tool result fed back into next turn");
}

/// CRITERION 2 — tool calls are intercepted BEFORE execution; policy can deny.
#[tokio::test]
async fn criterion_2_intercepts_tool_calls_before_execution() {
    // Allow path: Intercepted must strictly precede Executed.
    let server = stub_two_turn_model().await;
    let out = run_loop(&server.uri(), "go", "run-c2a", 5, allow_all(), CancellationToken::new()).await;
    let idx_intercept = out.events.iter().position(|e| matches!(e, Event::ToolIntercepted { .. })).unwrap();
    let idx_exec = out.events.iter().position(|e| matches!(e, Event::ToolExecuted { .. })).unwrap();
    assert!(idx_intercept < idx_exec, "mediation point sits before execution");

    // Deny path: policy blocks execution entirely.
    let server2 = stub_two_turn_model().await;
    let denials = Arc::new(Mutex::new(Vec::new()));
    let d = denials.clone();
    let deny_all = Box::new(move |name: &str, _args: &serde_json::Value| {
        d.lock().unwrap().push(name.to_string());
        Verdict::Deny
    });
    let out2 = run_loop(&server2.uri(), "go", "run-c2b", 5, deny_all, CancellationToken::new()).await;
    assert!(out2.events.iter().any(|e| matches!(e, Event::ToolDenied { .. })));
    assert!(!out2.events.iter().any(|e| matches!(e, Event::ToolExecuted { .. })), "denied tool never ran");
    assert_eq!(denials.lock().unwrap().as_slice(), &["echo".to_string()]);
}

/// CRITERION 3 — the harness terminates the loop mid-turn, without killing the process.
#[tokio::test]
async fn criterion_3_external_termination_mid_turn() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(FINAL_TURN_BODY)
                .set_delay(std::time::Duration::from_secs(10)), // model "hangs"
        )
        .mount(&server)
        .await;

    let cancel = CancellationToken::new();
    let c = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        c.cancel(); // external budget decision — not the model's
    });

    let started = std::time::Instant::now();
    let out = run_loop(&server.uri(), "hang", "run-c3", 5, allow_all(), cancel).await;
    assert_eq!(out.exit, Exit::Cancelled);
    assert!(started.elapsed() < std::time::Duration::from_secs(5), "did not wait for the model");
    assert!(matches!(out.events.last().unwrap(), Event::Terminated { reason, .. } if reason == "external-cancel"));
    // The process is alive to run this assertion — termination ≠ kill.
}

/// CRITERION 4 — every event of a run is correlated under one run-id, ordered by seq.
#[tokio::test]
async fn criterion_4_run_correlation() {
    let server = stub_two_turn_model().await;
    let out = run_loop(&server.uri(), "hello", "run-c4", 5, allow_all(), CancellationToken::new()).await;
    assert!(out.events.len() >= 6, "expected a full two-turn trace, got {}", out.events.len());
    assert!(out.events.iter().all(|e| e.run_id() == "run-c4"));
    let seqs: Vec<u32> = out.events.iter().map(|e| e.seq()).collect();
    let expected: Vec<u32> = (0..seqs.len() as u32).collect();
    assert_eq!(seqs, expected, "monotonic gap-free sequence (Ledger-ready)");
}
