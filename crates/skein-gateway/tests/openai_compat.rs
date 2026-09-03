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

use skein_core::{Content, Message, ModelClient, Role, TurnRequest};
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
