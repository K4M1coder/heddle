//! Acceptance tests for the OpenAI-compatible model client (spec 012).
//!
//! Every wire test here drives the real client against a **real socket**, served
//! by the `std::net::TcpListener` stub below. That is deliberate: the slice's
//! headline claim is that the bytes on the wire are the OpenAI chat-completions
//! contract, and only a socket can show you the literal bytes. An HTTP-mocking
//! crate would assert an *intent* — "a POST matching this matcher arrived" —
//! and would cost a `tokio` runtime in the test suite for a client that is
//! synchronous by design.
//!
//! No test here needs a running Ollama. The one that does is `#[ignore]`d.

use skein_core::{Content, Message, ModelClient, Role, SkeinError, TurnRequest};
use skein_gateway::{LocalEndpoint, OpenAiCompatClient};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

/// Long enough that a slow CI runner never trips it, short enough that a client
/// which silently sends nothing fails as a failure rather than as a hang.
const OBSERVE_TIMEOUT: Duration = Duration::from_secs(10);

/// One canned reply, written by the stub exactly as spelled here.
struct Reply {
    status: &'static str,
    body: String,
    /// Held before answering, to drive the client's global timeout.
    stall: Option<Duration>,
}

impl Reply {
    fn ok(body: impl Into<String>) -> Reply {
        Reply {
            status: "200 OK",
            body: body.into(),
            stall: None,
        }
    }
}

/// A provider that answers `replies` in order and reports the exact request
/// bytes it was sent.
///
/// The server thread asserts nothing: it reads, reports and answers. Every
/// expectation lives in the test body, so a failure names the test rather than a
/// worker thread — and if the thread dies, its sender drops and
/// [`Stub::request`] fails with a message saying so instead of hanging.
struct Stub {
    base_url: String,
    requests: Receiver<String>,
}

impl Stub {
    fn serving(replies: Vec<Reply>) -> Stub {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let addr = listener.local_addr().expect("the bound address");
        let (tx, requests) = mpsc::channel();
        std::thread::spawn(move || {
            for reply in replies {
                let Ok((mut socket, _)) = listener.accept() else {
                    return;
                };
                let Some(seen) = read_request(&mut socket) else {
                    return;
                };
                if tx.send(seen).is_err() {
                    return;
                }
                if let Some(stall) = reply.stall {
                    std::thread::sleep(stall);
                }
                // `connection: close` makes each turn a fresh accept, so a
                // multi-turn test counts connections deterministically instead
                // of racing ureq's pool.
                let _ = socket.write_all(
                    format!(
                        "HTTP/1.1 {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        reply.status,
                        reply.body.len(),
                        reply.body
                    )
                    .as_bytes(),
                );
                let _ = socket.flush();
            }
        });
        Stub {
            base_url: format!("http://{addr}/v1"),
            requests,
        }
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The next request's raw bytes, as text with `\r` stripped so an assertion
    /// failure is readable.
    fn request(&self) -> String {
        match self.requests.recv_timeout(OBSERVE_TIMEOUT) {
            Ok(raw) => raw.replace('\r', ""),
            Err(RecvTimeoutError::Timeout) => {
                panic!("the client sent no request within {OBSERVE_TIMEOUT:?}")
            }
            Err(RecvTimeoutError::Disconnected) => {
                panic!("the stub server stopped before a request arrived")
            }
        }
    }

    /// The body of the next request, parsed. Used where a test asserts one field
    /// rather than the whole byte string.
    fn request_body(&self) -> serde_json::Value {
        let raw = self.request();
        let (_, body) = raw
            .split_once("\n\n")
            .expect("headers and body separated by a blank line");
        serde_json::from_str(body).expect("a JSON request body")
    }
}

/// Reads one HTTP/1.1 request: the request line, the headers, and exactly
/// `content-length` body bytes.
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

/// A response shaped the way Ollama's OpenAI-compatible endpoint shapes one.
fn provider_reply(content: &str, finish_reason: &str, total_tokens: u64) -> String {
    serde_json::json!({
        "id": "chatcmpl-1",
        "object": "chat.completion",
        "created": 1_756_000_000_u64,
        "model": "llama3.1",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": content},
            "finish_reason": finish_reason
        }],
        "usage": {"prompt_tokens": 11, "completion_tokens": 7, "total_tokens": total_tokens}
    })
    .to_string()
}

fn client(base_url: &str, model: &str) -> OpenAiCompatClient {
    OpenAiCompatClient::new(
        LocalEndpoint::parse(base_url).expect("a loopback base URL"),
        model,
        Duration::from_secs(10),
    )
}

fn ask(messages: Vec<Message>) -> TurnRequest {
    TurnRequest {
        run_id: "run-1".into(),
        messages,
    }
}

#[test]
fn turn_sends_an_openai_chat_completions_request() {
    let stub = Stub::serving(vec![Reply::ok(provider_reply("hello back", "stop", 18))]);
    let mut model = client(stub.base_url(), "llama3.1");

    model
        .turn(&ask(vec![Message::user_text("hello")]))
        .expect("the stub answers");

    let seen = stub.request();
    let (headers, body) = seen.split_once("\n\n").expect("a blank-line separator");
    assert!(
        headers.starts_with("POST /v1/chat/completions HTTP/1.1\n"),
        "request line, in:\n{headers}"
    );
    assert!(
        headers
            .lines()
            .any(|l| l.eq_ignore_ascii_case("content-type: application/json")),
        "content-type header, in:\n{headers}"
    );
    // Byte-exact, not field-by-field: the request body is a serialized struct,
    // so its key order is ours and a reordering is a wire change worth catching.
    assert_eq!(
        body,
        r#"{"model":"llama3.1","messages":[{"role":"user","content":"hello"}],"stream":false}"#
    );
}

#[test]
fn a_conversation_history_is_sent_in_order() {
    let stub = Stub::serving(vec![Reply::ok(provider_reply("ok", "stop", 4))]);
    let mut model = client(stub.base_url(), "llama3.1");

    model
        .turn(&ask(vec![
            Message {
                role: Role::System,
                parts: vec![Content::Text {
                    text: "be terse".into(),
                }],
            },
            Message::user_text("first"),
            Message::assistant_text("second"),
            Message::user_text("third"),
        ]))
        .expect("the stub answers");

    assert_eq!(
        stub.request_body()["messages"],
        serde_json::json!([
            {"role": "system", "content": "be terse"},
            {"role": "user", "content": "first"},
            {"role": "assistant", "content": "second"},
            {"role": "user", "content": "third"},
        ])
    );
}

/// The message a refused base URL must carry, so a test asserts the refusal's
/// reason and not merely that something failed.
fn refusal(base_url: &str) -> String {
    match LocalEndpoint::parse(base_url) {
        Ok(endpoint) => panic!("{base_url} was accepted as {:?}", endpoint.base_url()),
        Err(SkeinError::Model(message)) => message,
        Err(other) => panic!("expected SkeinError::Model, got {other:?}"),
    }
}

#[test]
fn loopback_base_urls_are_accepted() {
    for base_url in [
        "http://127.0.0.1:11434/v1",
        "http://[::1]:11434/v1",
        "http://localhost:11434/v1",
    ] {
        let endpoint = LocalEndpoint::parse(base_url)
            .unwrap_or_else(|e| panic!("{base_url} should be accepted: {e}"));
        assert_eq!(endpoint.base_url(), base_url);
    }
    // A trailing slash is normalised, so the path is not doubled.
    assert_eq!(
        LocalEndpoint::parse("http://127.0.0.1:11434/v1/")
            .expect("accepted")
            .base_url(),
        "http://127.0.0.1:11434/v1"
    );
}

#[test]
fn a_non_loopback_base_url_is_refused() {
    // A host name other than `localhost` is refused *without* being resolved:
    // resolving it would itself be egress, since the name would leave this
    // machine in a DNS query. There is no socket and no DNS lookup to observe
    // here precisely because the refusal happens first.
    let named = refusal("http://ollama.example.com/v1");
    assert!(
        named.contains("without being resolved"),
        "a foreign name must be refused unresolved, got: {named}"
    );

    // A private-LAN literal is a valid IP that is not loopback (ADR-0002 D4
    // allows loopback, not the wider LAN).
    let lan = refusal("http://192.168.1.10:11434/v1");
    assert!(
        lan.contains("192.168.1.10") && lan.contains("not a loopback address"),
        "a LAN literal must be refused by address, got: {lan}"
    );
}

#[test]
fn an_https_base_url_is_refused() {
    // Refused on the scheme, before any socket exists. `ureq` is compiled with
    // no TLS backend, so `ureq::Error::TlsRequired` ("TLS required, but
    // transport is unsecured") is the hard floor underneath this check if it
    // were ever removed — the guard and the build are two independent locks.
    let message = refusal("https://api.openai.com/v1");
    assert!(
        message.contains("\"https\"") && message.contains("no TLS backend is compiled in"),
        "https must be refused on the scheme, got: {message}"
    );
}

/// One turn against a stub, expecting the client to refuse. Returns the
/// `SkeinError::Model` message, so a test asserts *why* it refused.
fn turn_error(stub: &Stub, prompt: &str) -> String {
    let mut model = client(stub.base_url(), "llama3.1");
    match model.turn(&ask(vec![Message::user_text(prompt)])) {
        Ok(response) => panic!("expected a refusal, got {response:?}"),
        Err(SkeinError::Model(message)) => message,
        Err(other) => panic!("expected SkeinError::Model, got {other:?}"),
    }
}

#[test]
fn turn_parses_a_realistic_response_into_a_turn_response() {
    let stub = Stub::serving(vec![Reply::ok(provider_reply(
        "42 is the answer.",
        "stop",
        23,
    ))]);
    let mut model = client(stub.base_url(), "llama3.1");

    let response = model
        .turn(&ask(vec![Message::user_text("what is the answer?")]))
        .expect("the stub answers");

    assert_eq!(
        response.message,
        Message::assistant_text("42 is the answer.")
    );
    assert!(
        response.final_output,
        "finish_reason 'stop' with no tool call"
    );
    assert!(response.tool_calls.is_empty());
    // The provider's own number, not a constant: `provider_reply`'s
    // prompt_tokens + completion_tokens is 18, so a client that summed instead
    // of reading total_tokens would not produce 23.
    assert_eq!(response.tokens_used, 23);
}

#[test]
fn tokens_used_falls_back_to_prompt_plus_completion_when_total_is_absent() {
    let stub = Stub::serving(vec![Reply::ok(
        serde_json::json!({
            "choices": [{
                "message": {"role": "assistant", "content": "ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 31, "completion_tokens": 11}
        })
        .to_string(),
    )]);
    let mut model = client(stub.base_url(), "llama3.1");

    let response = model
        .turn(&ask(vec![Message::user_text("count me")]))
        .expect("the stub answers");

    assert_eq!(response.tokens_used, 42);
}

#[test]
fn a_response_without_usage_is_refused_rather_than_metered_as_zero() {
    // The guard Constitution VIII rests on. `LoopController::should_exit` stops
    // on `tokens >= max_tokens`, so metering an unmetered turn as 0 would
    // disable the token budget while looking like it worked. Refusing loudly is
    // this project's established answer to "I cannot honestly produce this
    // value".
    let stub = Stub::serving(vec![Reply::ok(
        serde_json::json!({
            "choices": [{
                "message": {"role": "assistant", "content": "unmetered"},
                "finish_reason": "stop"
            }]
        })
        .to_string(),
    )]);

    let message = turn_error(&stub, "how much did that cost?");
    assert!(
        message.contains("without token metering") && message.contains("token budget"),
        "the refusal must name the missing metering, got: {message}"
    );

    // And a `usage` that is present but half-filled is refused too: a sum needs
    // both halves.
    let partial = Stub::serving(vec![Reply::ok(
        serde_json::json!({
            "choices": [{
                "message": {"role": "assistant", "content": "half-metered"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 9}
        })
        .to_string(),
    )]);
    assert!(
        turn_error(&partial, "and now?").contains("without token metering"),
        "a half-filled usage object is not metering"
    );
}
