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
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let addr = listener.local_addr().expect("the bound address");
        let (tx, requests) = mpsc::channel();
        std::thread::spawn(move || {
            for body in bodies {
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
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
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
        let raw = match self.requests.recv_timeout(Duration::from_secs(10)) {
            Ok(raw) => raw.replace('\r', ""),
            Err(RecvTimeoutError::Timeout) => panic!("the loop sent no request within 10s"),
            Err(RecvTimeoutError::Disconnected) => panic!("the stub provider stopped early"),
        };
        let (_, body) = raw.split_once("\n\n").expect("a blank-line separator");
        serde_json::from_str(body).expect("a JSON request body")
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
    let mut loops = NativeLoop::new(
        OpenAiCompatClient::new(
            LocalEndpoint::parse(provider.base_url.as_str()).expect("a loopback base URL"),
            "llama3.1",
            Duration::from_secs(10),
        ),
        NoGroundTruth,
        ToolGateway::new(
            NoTools,
            ToolPolicy::new(vec![], vec![]),
            Redactor::new(vec![]),
        ),
        Redactor::new(vec![]),
    );
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
            StepKind::LlmResponse,
            StepKind::BudgetSpent,
            StepKind::IterationBoundary,
            StepKind::LlmRequest,
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

    // The chain holds the *translated* TurnRequest/TurnResponse, not the
    // provider's raw wire bytes — the gap spec 012 states plainly and defers.
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
    let mut loops = NativeLoop::new(
        OpenAiCompatClient::new(
            LocalEndpoint::parse(&dead).expect("a loopback base URL"),
            "llama3.1",
            Duration::from_secs(10),
        ),
        NoGroundTruth,
        ToolGateway::new(
            NoTools,
            ToolPolicy::new(vec![], vec![]),
            Redactor::new(vec![]),
        ),
        Redactor::new(vec![]),
    );
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
    assert_eq!(
        kinds(&ledger, "run-dead"),
        vec![StepKind::IterationBoundary, StepKind::LlmRequest]
    );
    assert!(payload(&ledger, "run-dead", StepKind::LlmRequest).contains("anyone home?"));
    assert!(ledger.verify_chain("run-dead").is_ok());
}
