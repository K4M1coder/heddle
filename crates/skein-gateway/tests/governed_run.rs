//! The slice's headline claim, end to end: the **real** model client driving the
//! **real** governed loop into a **real** hash-chained Ledger (spec 012, User
//! Story 1) — and still with no Ollama anywhere, because the provider is a
//! `std::net::TcpListener` speaking HTTP/1.1.
//!
//! Everything upstream of the socket is product code: `NativeLoop`,
//! `LoopController`, `ToolGateway` and `Ledger` are the same types slices
//! 004–009 shipped, unchanged by this slice, so a break here is a break in the
//! wiring and not in a stand-in.

use skein_core::{
    Exit, Ledger, LoopBudget, LoopController, Message, NativeLoop, ProgressProbe, Redactor, Result,
    SkeinError, StepKind, ToolCall, ToolGateway, ToolOutcome, ToolPolicy, ToolTransport,
    WireExchange,
};
use skein_gateway::{LocalEndpoint, OpenAiCompatClient};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

/// A provider answering a fixed script, one connection per turn.
///
/// `connection: close` on every reply is what makes a multi-turn stub
/// deterministic: each turn is a fresh accept rather than a race against
/// `ureq`'s connection pool.
struct StubProvider {
    base_url: String,
    requests: Receiver<String>,
}

impl StubProvider {
    fn serving(bodies: Vec<String>) -> StubProvider {
        StubProvider::answering(bodies.into_iter().map(|body| (200, body)).collect())
    }

    /// One `(status, body)` per turn, so the provider-error path — which
    /// returns before the reply is ever parsed — is reachable from a test.
    fn answering(replies: Vec<(u16, String)>) -> StubProvider {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let addr = listener.local_addr().expect("the bound address");
        let (tx, requests) = mpsc::channel();
        std::thread::spawn(move || {
            for (status, body) in replies {
                let Ok((mut socket, _)) = listener.accept() else {
                    return;
                };
                let Some(seen) = read_request(&mut socket) else {
                    return;
                };
                if tx.send(seen).is_err() {
                    return;
                }
                let _ = socket.write_all(
                    format!(
                        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                );
                let _ = socket.flush();
            }
        });
        StubProvider {
            base_url: format!("http://{addr}/v1"),
            requests,
        }
    }

    /// The body of the next request the provider was sent, parsed.
    fn request_body(&self) -> serde_json::Value {
        serde_json::from_str(&self.raw_request_body()).expect("a JSON request body")
    }

    /// The same body, **unparsed and unnormalized**: a parse would launder
    /// exactly the divergence the wire-capture tests exist to detect.
    fn raw_request_body(&self) -> String {
        let raw = match self.requests.recv_timeout(Duration::from_secs(10)) {
            Ok(raw) => raw,
            Err(RecvTimeoutError::Timeout) => panic!("the loop sent no request within 10s"),
            Err(RecvTimeoutError::Disconnected) => panic!("the stub provider stopped early"),
        };
        let (_, body) = raw.split_once("\r\n\r\n").expect("a blank-line separator");
        body.to_string()
    }
}

fn read_request(socket: &mut TcpStream) -> Option<String> {
    let mut reader = BufReader::new(socket.try_clone().ok()?);
    let mut raw = String::new();
    let mut len = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            len = value.trim().parse().ok()?;
        }
        let blank = line == "\r\n" || line == "\n";
        raw.push_str(&line);
        if blank {
            break;
        }
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).ok()?;
    raw.push_str(&String::from_utf8_lossy(&body));
    Some(raw)
}

fn reply(content: &str, finish_reason: &str, total_tokens: u64) -> String {
    serde_json::json!({
        "choices": [{
            "message": {"role": "assistant", "content": content},
            "finish_reason": finish_reason
        }],
        "usage": {"total_tokens": total_tokens}
    })
    .to_string()
}

/// The collaborators a tool-less chat honestly has, mirroring what
/// `skein chat` supplies: no ground truth, and a transport no name can reach.
struct NoGroundTruth;

impl ProgressProbe for NoGroundTruth {
    fn observe(&mut self) -> bool {
        false
    }
}

struct NoTools;

impl ToolTransport for NoTools {
    fn call(&mut self, call: &ToolCall) -> Result<ToolOutcome> {
        Err(SkeinError::Tool(format!(
            "no tool server is configured: {} was not called",
            call.tool
        )))
    }
}

/// The collaborators every test here injects: the real client pointed at
/// `base_url`, no ground truth, and a transport no tool name can reach. Only
/// the redactor varies, and it is the same one on both seats because a run
/// configures one secret set.
fn chat_loop(
    base_url: &str,
    redactor: Redactor,
) -> NativeLoop<OpenAiCompatClient, NoGroundTruth, NoTools> {
    NativeLoop::new(
        OpenAiCompatClient::new(
            LocalEndpoint::parse(base_url).expect("a loopback base URL"),
            "llama3.1",
            Duration::from_secs(10),
        ),
        NoGroundTruth,
        ToolGateway::new(NoTools, ToolPolicy::new(vec![], vec![]), redactor.clone()),
        redactor,
    )
}

fn kinds(ledger: &Ledger, run_id: &str) -> Vec<StepKind> {
    ledger
        .log(run_id)
        .into_iter()
        .map(|s| s.kind.clone())
        .collect()
}

fn payload(ledger: &Ledger, run_id: &str, kind: StepKind) -> String {
    ledger
        .log(run_id)
        .into_iter()
        .find(|s| s.kind == kind)
        .unwrap_or_else(|| panic!("no {kind:?} step in run {run_id}"))
        .payload
        .clone()
}

#[test]
fn an_end_to_end_run_against_a_stub_provider_lands_on_the_chain() {
    // Two replies: the first is not final (the provider truncated it), so the
    // loop takes a second turn and the test can prove history was fed back.
    let provider = StubProvider::serving(vec![
        reply("thinking out lou", "length", 17),
        reply("the answer is 42", "stop", 25),
    ]);
    let mut loops = chat_loop(&provider.base_url, Redactor::new(vec![]));
    let mut ledger = Ledger::new();
    let mut controller = LoopController::new(LoopBudget::new(4, 1_000, 4));

    let run = loops
        .run(
            "run-e2e",
            Message::user_text("what is the answer?"),
            &mut ledger,
            &mut controller,
        )
        .expect("the run completes");

    assert_eq!(run.exit, Exit::FinalOutput);
    assert_eq!(
        run.final_message,
        Some(Message::assistant_text("the answer is 42"))
    );

    // Two iterations of the same four steps, then exactly one Exit.
    assert_eq!(
        kinds(&ledger, "run-e2e"),
        vec![
            StepKind::IterationBoundary,
            StepKind::LlmRequest,
            StepKind::WireExchange,
            StepKind::LlmResponse,
            StepKind::BudgetSpent,
            StepKind::IterationBoundary,
            StepKind::LlmRequest,
            StepKind::WireExchange,
            StepKind::LlmResponse,
            StepKind::BudgetSpent,
            StepKind::Exit,
        ]
    );

    // The metering on the chain is the provider's own, turn by turn — not a
    // constant, and not a zero.
    let spent: Vec<&str> = ledger
        .log("run-e2e")
        .into_iter()
        .filter(|s| s.kind == StepKind::BudgetSpent)
        .map(|s| s.payload.as_str())
        .collect();
    assert_eq!(spent, vec!["17", "25"]);
    assert_eq!(controller.tokens(), 42);
    assert_eq!(controller.iters(), 2);

    // The loop really fed history back: the second request carries the first
    // turn's assistant message. Read off the wire, not off the chain.
    let first = provider.request_body();
    assert_eq!(
        first["messages"],
        serde_json::json!([{"role": "user", "content": "what is the answer?"}])
    );
    let second = provider.request_body();
    assert_eq!(
        second["messages"],
        serde_json::json!([
            {"role": "user", "content": "what is the answer?"},
            {"role": "assistant", "content": "thinking out lou"},
        ])
    );

    assert!(ledger.verify_chain("run-e2e").is_ok());

    // The *translated* TurnResponse. Since spec 023 the chain also holds the
    // bytes it was translated from, one step earlier, so the two can now be
    // read against each other — which the tests below do.
    let recorded: serde_json::Value =
        serde_json::from_str(&payload(&ledger, "run-e2e", StepKind::LlmResponse))
            .expect("the LlmResponse payload is a serialized TurnResponse");
    assert_eq!(recorded["tokens_used"], 17);
    assert_eq!(recorded["final_output"], false);
}

#[test]
fn a_provider_failure_ends_the_run_with_the_request_already_on_the_chain() {
    // Nothing is listening, so the very first turn fails. The request is
    // appended before the call, so the chain records what was asked even though
    // nothing answered — the property `NativeLoop`'s own comment claims.
    let dead = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("the bound address").port();
        drop(listener);
        format!("http://127.0.0.1:{port}/v1")
    };
    let mut loops = chat_loop(&dead, Redactor::new(vec![]));
    let mut ledger = Ledger::new();
    let mut controller = LoopController::new(LoopBudget::new(4, 1_000, 4));

    let error = loops
        .run(
            "run-dead",
            Message::user_text("anyone home?"),
            &mut ledger,
            &mut controller,
        )
        .expect_err("no provider is listening");

    assert!(matches!(error, SkeinError::Model(_)), "got {error:?}");
    // No `WireExchange`: nothing was sent and nothing answered, and a chain
    // that claimed bytes crossed here would be lying about the one fact it
    // exists to hold.
    assert_eq!(
        kinds(&ledger, "run-dead"),
        vec![StepKind::IterationBoundary, StepKind::LlmRequest]
    );
    assert!(payload(&ledger, "run-dead", StepKind::LlmRequest).contains("anyone home?"));
    assert!(ledger.verify_chain("run-dead").is_ok());
}

/// The one exchange of a single-turn run, as it was recorded.
fn exchange(ledger: &Ledger, run_id: &str) -> WireExchange {
    serde_json::from_str(&payload(ledger, run_id, StepKind::WireExchange))
        .expect("the WireExchange payload is a serialized WireExchange")
}

#[test]
fn the_chain_records_the_literal_bytes_of_the_exchange() {
    let answer = reply("the answer is 42", "stop", 25);
    let provider = StubProvider::serving(vec![answer.clone()]);
    let mut loops = chat_loop(&provider.base_url, Redactor::new(vec![]));
    let mut ledger = Ledger::new();
    let mut controller = LoopController::new(LoopBudget::new(4, 1_000, 4));

    loops
        .run(
            "run-wire",
            Message::user_text("what is the answer?"),
            &mut ledger,
            &mut controller,
        )
        .expect("the run completes");

    let recorded = exchange(&ledger, "run-wire");
    // Byte equality against what the socket on the other end actually read and
    // wrote, in both directions. Not a parse, not a field-by-field comparison:
    // either the chain holds the transmitted buffer or it holds a re-derivation
    // of it, and only the first is what this slice claims.
    assert_eq!(recorded.request, provider.raw_request_body());
    assert_eq!(recorded.response, answer);
    assert_eq!(recorded.status, 200);
    assert!(
        recorded.url.ends_with("/chat/completions"),
        "got {}",
        recorded.url
    );

    assert!(ledger.verify_chain("run-wire").is_ok());
}

/// A secret carrying a `"`. Serialized into the request body it becomes
/// `pa\"ss-w0rd`, so the literal needle is *absent* from the wire text and a
/// redactor that only looks for the literal form leaks it in cleartext.
const QUOTED_SECRET: &str = "pa\"ss-w0rd";

/// The same secret as it appears *inside* a serialized JSON body — the only
/// form that is ever on the wire, and the one a literal-needle scrub misses.
const ESCAPED_SECRET: &str = r#"pa\"ss-w0rd"#;

/// Every payload of a run, so a leak anywhere is a failure and not just a leak
/// in the step the test happens to name.
fn payloads(ledger: &Ledger, run_id: &str) -> Vec<String> {
    ledger
        .log(run_id)
        .into_iter()
        .map(|s| s.payload.clone())
        .collect()
}

#[test]
fn a_quote_bearing_secret_is_scrubbed_from_the_exchange_it_escaped_into() {
    let provider = StubProvider::serving(vec![reply("understood", "stop", 11)]);
    let mut loops = chat_loop(
        &provider.base_url,
        Redactor::new(vec![QUOTED_SECRET.into()]),
    );
    let mut ledger = Ledger::new();
    let mut controller = LoopController::new(LoopBudget::new(4, 1_000, 4));

    loops
        .run(
            "run-quoted",
            Message::user_text(format!("the password is {QUOTED_SECRET}")),
            &mut ledger,
            &mut controller,
        )
        .expect("the run completes");

    // The control that makes the assertion below mean something: the model was
    // sent the truth. Only the record is scrubbed, and the escaped form really
    // is what crossed — so a test that passed by accident would fail here.
    let sent = provider.raw_request_body();
    assert!(
        sent.contains(ESCAPED_SECRET),
        "the provider must be sent the real secret, got {sent}"
    );
    assert!(!sent.contains("***"));

    for payload in payloads(&ledger, "run-quoted") {
        assert!(
            !payload.contains(QUOTED_SECRET),
            "a payload carries the secret literally: {payload}"
        );
    }
    // Asserted on the *parsed* exchange rather than on the step's payload text.
    // The payload escapes the wire body a second time, so a `contains` over it
    // searches for a thrice-escaped needle and passes while the secret sits
    // there in plain sight — which is how this test was first written, and why
    // it is not written that way now.
    let recorded = exchange(&ledger, "run-quoted");
    assert!(
        !recorded.request.contains(ESCAPED_SECRET),
        "the escaped secret reached the chain: {}",
        recorded.request
    );
    assert!(
        !recorded.request.contains(QUOTED_SECRET),
        "the literal secret reached the chain: {}",
        recorded.request
    );
    assert!(recorded.request.contains("***"), "got {}", recorded.request);
    assert!(ledger.verify_chain("run-quoted").is_ok());
}

#[test]
fn a_plain_secret_is_scrubbed_from_the_exchange() {
    const SECRET: &str = "hunter2token";
    let provider = StubProvider::serving(vec![reply("understood", "stop", 11)]);
    let mut loops = chat_loop(&provider.base_url, Redactor::new(vec![SECRET.into()]));
    let mut ledger = Ledger::new();
    let mut controller = LoopController::new(LoopBudget::new(4, 1_000, 4));

    loops
        .run(
            "run-plain",
            Message::user_text(format!("the password is {SECRET}")),
            &mut ledger,
            &mut controller,
        )
        .expect("the run completes");

    for payload in payloads(&ledger, "run-plain") {
        assert!(!payload.contains(SECRET), "{payload}");
    }
    let recorded = exchange(&ledger, "run-plain");
    assert!(recorded.request.contains("***"), "got {}", recorded.request);
    assert!(ledger.verify_chain("run-plain").is_ok());
}

#[test]
fn an_unparseable_reply_still_leaves_the_bytes_that_caused_it_on_the_chain() {
    // A body no `ChatResponse` can make sense of: the run fails, and this step
    // is the only place in the product that can say why.
    const GIBBERISH: &str = r#"{"error":{"message":"model not loaded"}}"#;
    let provider = StubProvider::serving(vec![GIBBERISH.to_string()]);
    let mut loops = chat_loop(&provider.base_url, Redactor::new(vec![]));
    let mut ledger = Ledger::new();
    let mut controller = LoopController::new(LoopBudget::new(4, 1_000, 4));

    let error = loops
        .run(
            "run-garbage",
            Message::user_text("what is the answer?"),
            &mut ledger,
            &mut controller,
        )
        .expect_err("the reply cannot be parsed");

    assert!(matches!(error, SkeinError::Model(_)), "got {error:?}");
    assert_eq!(
        kinds(&ledger, "run-garbage"),
        vec![
            StepKind::IterationBoundary,
            StepKind::LlmRequest,
            StepKind::WireExchange,
        ]
    );
    let recorded = exchange(&ledger, "run-garbage");
    assert_eq!(recorded.response, GIBBERISH);
    assert_eq!(recorded.status, 200);
    assert!(ledger.verify_chain("run-garbage").is_ok());
}

#[test]
fn a_provider_error_status_still_leaves_the_bytes_that_caused_it_on_the_chain() {
    const REFUSAL: &str = r#"{"error":{"message":"model \"llama3.1\" is not loaded"}}"#;
    let provider = StubProvider::answering(vec![(500, REFUSAL.to_string())]);
    let mut loops = chat_loop(&provider.base_url, Redactor::new(vec![]));
    let mut ledger = Ledger::new();
    let mut controller = LoopController::new(LoopBudget::new(4, 1_000, 4));

    let error = loops
        .run(
            "run-500",
            Message::user_text("what is the answer?"),
            &mut ledger,
            &mut controller,
        )
        .expect_err("the provider refused");

    assert!(matches!(error, SkeinError::Model(_)), "got {error:?}");
    assert_eq!(
        kinds(&ledger, "run-500"),
        vec![
            StepKind::IterationBoundary,
            StepKind::LlmRequest,
            StepKind::WireExchange,
        ]
    );
    let recorded = exchange(&ledger, "run-500");
    assert_eq!(recorded.status, 500);
    assert_eq!(recorded.response, REFUSAL);
    assert!(ledger.verify_chain("run-500").is_ok());
}
