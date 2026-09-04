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

use skein_core::{
    Content, Message, ModelClient, Role, SkeinError, TextSink, ToolSpec, TurnRequest,
};
use skein_gateway::{LocalEndpoint, OpenAiCompatClient};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Long enough that a slow CI runner never trips it, short enough that a client
/// which silently sends nothing fails as a failure rather than as a hang.
const OBSERVE_TIMEOUT: Duration = Duration::from_secs(10);

/// One canned reply, written by the stub exactly as spelled here.
struct Reply {
    status: &'static str,
    content_type: &'static str,
    body: String,
    /// Held before answering, to drive the client's global timeout.
    stall: Option<Duration>,
}

impl Reply {
    /// A successful answer. Its content type is `text/event-stream` even for the
    /// bodies below that are not streams at all: a provider whose 200 carries
    /// something else entirely is exactly the case those tests exist for, and
    /// the client's behaviour must not depend on a header it does not read.
    fn ok(body: impl Into<String>) -> Reply {
        Reply {
            status: "200 OK",
            content_type: "text/event-stream",
            body: body.into(),
            stall: None,
        }
    }

    /// A refusal, which the provider sends as a plain JSON body under a non-2xx
    /// status — never as a stream. Measured against the real provider.
    fn status(status: &'static str, body: impl Into<String>) -> Reply {
        Reply {
            status,
            content_type: "application/json",
            body: body.into(),
            stall: None,
        }
    }

    /// Accepts the request, then holds it — the shape a client must time out
    /// against rather than block the run on.
    fn stalled(stall: Duration) -> Reply {
        Reply {
            status: "200 OK",
            content_type: "text/event-stream",
            body: String::new(),
            stall: Some(stall),
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
                        "HTTP/1.1 {}\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        reply.status,
                        reply.content_type,
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

/// SSE framing as the real provider writes it: one `data: {json}` per event,
/// each closed by a blank line, and `data: [DONE]` last.
///
/// The separator is a bare `\n\n` and not CRLF. That is measured, not assumed —
/// `cat -A` against the live endpoint shows every line ending `$`, never `^M$` —
/// and it matters, because the client's capture keeps its terminators.
fn sse(events: Vec<serde_json::Value>) -> String {
    let mut raw = String::new();
    for event in events {
        raw.push_str(&format!("data: {event}\n\n"));
    }
    raw.push_str("data: [DONE]\n\n");
    raw
}

/// One content delta, the shape the provider sends for each fragment of an
/// answer.
fn delta(content: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "chatcmpl-1",
        "object": "chat.completion.chunk",
        "created": 1_756_000_000_u64,
        "model": "llama3.1",
        "choices": [{"index": 0, "delta": {"role": "assistant", "content": content}}]
    })
}

/// The event that closes the choice. Its delta is empty; `finish_reason` is the
/// only thing it carries.
fn finish(reason: &str) -> serde_json::Value {
    serde_json::json!({
        "choices": [{"index": 0, "delta": {}, "finish_reason": reason}]
    })
}

/// The metering event `stream_options.include_usage` buys: `choices` is empty
/// and `usage` is the whole point. It arrives immediately before `[DONE]`.
fn usage(total_tokens: u64) -> serde_json::Value {
    serde_json::json!({
        "choices": [],
        "usage": {"prompt_tokens": 11, "completion_tokens": 7, "total_tokens": total_tokens}
    })
}

/// A whole answer in one delta, framed the way the provider frames it. The
/// shorthand most tests here want, since they assert something other than how
/// the answer was split.
fn provider_reply(content: &str, finish_reason: &str, total_tokens: u64) -> String {
    sse(vec![
        delta(content),
        finish(finish_reason),
        usage(total_tokens),
    ])
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
        tools: Vec::new(),
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
        r#"{"model":"llama3.1","messages":[{"role":"user","content":"hello"}],"stream":true,"stream_options":{"include_usage":true}}"#
    );
}

#[test]
fn advertised_tools_are_sent_in_openais_function_shape() {
    let stub = Stub::serving(vec![Reply::ok(provider_reply("ok", "stop", 4))]);
    let mut model = client(stub.base_url(), "llama3.1");

    model
        .turn(&TurnRequest {
            run_id: "run-1".into(),
            messages: vec![Message::user_text("hello")],
            tools: vec![ToolSpec::new(
                "fs_read",
                "Read a UTF-8 text file.",
                // The schema's keys are spelled in alphabetical order, and that
                // is load-bearing rather than tidy. `agent-client-protocol`
                // enables `serde_json/preserve_order`, and Cargo unifies
                // features per build graph — so `Map` is an insertion-ordered
                // `IndexMap` under `cargo test --workspace` and a sorted
                // `BTreeMap` under `cargo test -p skein-gateway`. Writing the
                // keys sorted makes the two orders identical, so the byte-exact
                // assertion below holds under either resolution. The envelope
                // keys around it are struct fields, so their order is ours.
                serde_json::json!({
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"],
                    "type": "object",
                }),
            )],
        })
        .expect("the stub answers");

    let seen = stub.request();
    let (_, body) = seen.split_once("\n\n").expect("a blank-line separator");
    // Byte-exact for the same reason the no-tools request is: these bytes are a
    // provider contract, and `strict` is *absent* on purpose — it is an OpenAI
    // structured-outputs extension, and sending an unrecognised key to a local
    // provider buys nothing. An assertion on fields could not catch its return.
    assert_eq!(
        body,
        r#"{"model":"llama3.1","messages":[{"role":"user","content":"hello"}],"stream":true,"stream_options":{"include_usage":true},"tools":[{"type":"function","function":{"name":"fs_read","description":"Read a UTF-8 text file.","parameters":{"properties":{"path":{"type":"string"}},"required":["path"],"type":"object"}}}]}"#
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
                tool_calls: Vec::new(),
                tool_call_id: None,
            },
            Message::user_text("first"),
            Message::assistant_text("second"),
            Message::user_text("third"),
            Message::assistant_text("").with_tool_calls(vec![skein_core::ToolCall::with_id(
                "call_1",
                "fs_read",
                serde_json::json!({"path": "alpha"}),
            )]),
            Message::tool_result("call_1", "alpha holds 7"),
        ]))
        .expect("the stub answers");

    assert_eq!(
        stub.request_body()["messages"],
        serde_json::json!([
            {"role": "system", "content": "be terse"},
            {"role": "user", "content": "first"},
            {"role": "assistant", "content": "second"},
            {"role": "user", "content": "third"},
            // `arguments` is a JSON *string* holding JSON, not an object. That
            // is the wire format, and a test comparing parsed values is the
            // only kind that can tell the two apart.
            {"role": "assistant", "content": "", "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {"name": "fs_read", "arguments": "{\"path\":\"alpha\"}"}
            }]},
            {"role": "tool", "content": "alpha holds 7", "tool_call_id": "call_1"},
        ])
    );
}

#[test]
fn every_wire_tool_message_answers_exactly_one_earlier_echoed_call() {
    // Slice 015's stated objection to this shape — "a dangling call id" — as an
    // assertion rather than a promise. It reads the serialized body, so it
    // covers the translation and not only the core's bookkeeping.
    let stub = Stub::serving(vec![Reply::ok(provider_reply("ok", "stop", 4))]);
    let mut model = client(stub.base_url(), "llama3.1");

    model
        .turn(&ask(vec![
            Message::user_text("read both"),
            Message::assistant_text("").with_tool_calls(vec![
                skein_core::ToolCall::with_id(
                    "call_a",
                    "fs_read",
                    serde_json::json!({"path": "alpha"}),
                ),
                skein_core::ToolCall::with_id(
                    "call_b",
                    "fs_read",
                    serde_json::json!({"path": "beta"}),
                ),
            ]),
            Message::tool_result("call_a", "7"),
            Message::tool_result("call_b", "19"),
        ]))
        .expect("the stub answers");

    let body = stub.request_body();
    let messages = body["messages"].as_array().expect("a messages array");
    let mut echoed: Vec<&str> = Vec::new();
    let mut answered: Vec<&str> = Vec::new();

    for (i, message) in messages.iter().enumerate() {
        if message["role"] == "tool" {
            let id = message["tool_call_id"]
                .as_str()
                .unwrap_or_else(|| panic!("messages[{i}] is a tool message with no id: {body}"));
            assert!(
                echoed.contains(&id),
                "messages[{i}] answers {id:?}, which nothing earlier asked for: {body}"
            );
            answered.push(id);
        }
        for call in message["tool_calls"].as_array().into_iter().flatten() {
            echoed.push(call["id"].as_str().expect("an echoed call carries an id"));
        }
    }

    assert_eq!(
        echoed, answered,
        "every echoed id is answered exactly once, in order: {body}"
    );
    assert!(!echoed.is_empty(), "the fixture must echo something");
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
fn a_streamed_answer_is_accumulated_across_its_deltas() {
    // The slice's premise as an assertion: the provider produces the answer in
    // pieces, and one `TurnResponse` comes out the other side. The pieces are
    // spelled so that a client concatenating them in the wrong order, or
    // dropping one, produces something visibly different rather than something
    // merely shorter.
    let stub = Stub::serving(vec![Reply::ok(sse(vec![
        delta("The "),
        delta("answer "),
        delta("is "),
        delta("42."),
        finish("stop"),
        usage(61),
    ]))]);
    let mut model = client(stub.base_url(), "llama3.1");

    let response = model
        .turn(&ask(vec![Message::user_text("what is the answer?")]))
        .expect("the stub answers");

    assert_eq!(
        response.message,
        Message::assistant_text("The answer is 42.")
    );
    // From the stream's own metering event, which exists only because the
    // request asked for `stream_options.include_usage`. The deltas carry no
    // count at all, so this number cannot have come from anywhere else.
    assert_eq!(response.tokens_used, 61);
    assert!(response.final_output);
    assert!(response.tool_calls.is_empty());
}

/// Two complete tool calls in **one** delta, each with its own `index` — the
/// shape the real provider was measured sending on all three models tried. The
/// `plan.md` §0.2(4) measurement that contradicted the slice's own premise.
fn whole_tool_calls() -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "index": 0,
            "delta": {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {"index": 0, "id": "call_43lp106j", "type": "function", "function": {
                        "name": "fs_read", "arguments": "{\"path\":\"alpha\"}"}},
                    {"index": 1, "id": "call_jtd04izj", "type": "function", "function": {
                        "name": "fs_write", "arguments": "{\"path\":\"beta\",\"text\":\"hi\"}"}}
                ]
            }
        }]
    })
}

/// What both shapes below must produce, spelled once so the two tests cannot
/// drift into agreeing about something weaker than equality.
fn expected_calls() -> Vec<skein_core::ToolCall> {
    vec![
        skein_core::ToolCall::with_id(
            "call_43lp106j",
            "fs_read",
            serde_json::json!({"path": "alpha"}),
        ),
        skein_core::ToolCall::with_id(
            "call_jtd04izj",
            "fs_write",
            serde_json::json!({"path": "beta", "text": "hi"}),
        ),
    ]
}

#[test]
fn tool_calls_arriving_whole_in_one_delta_are_translated() {
    let stub = Stub::serving(vec![Reply::ok(sse(vec![
        whole_tool_calls(),
        finish("tool_calls"),
        usage(61),
    ]))]);
    let mut model = client(stub.base_url(), "llama3.1");

    let response = model
        .turn(&ask(vec![Message::user_text("read alpha then write beta")]))
        .expect("the stub answers");

    assert_eq!(response.tool_calls, expected_calls());
    assert!(!response.final_output, "a tool request is not an answer");
    assert_eq!(response.message, Message::assistant_text(""));
}

#[test]
fn tool_calls_arriving_one_whole_call_per_delta_are_translated() {
    // The shape actually observed while implementing this slice, against
    // `qwen3.8:27b`: each call complete within its own event, but the two calls
    // in **separate** events with distinct `index` values — between the two
    // shapes either side of it, and covered by neither on its own. An
    // accumulator that replaced its call list per event instead of merging into
    // it would keep only the second call and pass every other test here.
    let one = |call: serde_json::Value| serde_json::json!({"choices": [{"index": 0, "delta": {"tool_calls": [call]}, "finish_reason": null}]});
    let stub = Stub::serving(vec![Reply::ok(sse(vec![
        one(
            serde_json::json!({"id": "call_43lp106j", "index": 0, "type": "function",
             "function": {"name": "fs_read", "arguments": "{\"path\":\"alpha\"}"}}),
        ),
        one(
            serde_json::json!({"id": "call_jtd04izj", "index": 1, "type": "function",
             "function": {"name": "fs_write", "arguments": "{\"path\":\"beta\",\"text\":\"hi\"}"}}),
        ),
        finish("tool_calls"),
        usage(61),
    ]))]);
    let mut model = client(stub.base_url(), "llama3.1");

    let response = model
        .turn(&ask(vec![Message::user_text("read alpha then write beta")]))
        .expect("the stub answers");

    assert_eq!(response.tool_calls, expected_calls());
    assert!(!response.final_output);
}

#[test]
fn tool_calls_fragmented_across_deltas_accumulate_to_the_same_calls() {
    // The shape the `index` keying exists for. The provider this slice targets
    // never sends it — but `skein-cli`'s wiring documents a LiteLLM sidecar as
    // a supported deployment, and LiteLLM proxying a real cloud model does
    // fragment `arguments` across events. The name is split too, because
    // nothing on the wire promises it arrives in one piece either.
    let fragment = |calls: serde_json::Value| serde_json::json!({"choices": [{"index": 0, "delta": {"tool_calls": calls}}]});
    let stub = Stub::serving(vec![Reply::ok(sse(vec![
        fragment(serde_json::json!([
            {"index": 0, "id": "call_43lp106j", "type": "function",
             "function": {"name": "fs_", "arguments": ""}},
        ])),
        fragment(serde_json::json!([
            {"index": 0, "function": {"name": "read", "arguments": "{\"path\":"}},
        ])),
        fragment(serde_json::json!([
            {"index": 0, "function": {"arguments": "\"alpha\"}"}},
            {"index": 1, "id": "call_jtd04izj", "type": "function",
             "function": {"name": "fs_write", "arguments": "{\"path\""}},
        ])),
        fragment(serde_json::json!([
            {"index": 1, "function": {"arguments": ":\"beta\",\"text\""}},
        ])),
        fragment(serde_json::json!([
            {"index": 1, "function": {"arguments": ":\"hi\"}"}},
        ])),
        finish("tool_calls"),
        usage(61),
    ]))]);
    let mut model = client(stub.base_url(), "llama3.1");

    let response = model
        .turn(&ask(vec![Message::user_text("read alpha then write beta")]))
        .expect("the stub answers");

    // Identical to the whole-arrival case, including the order, which comes
    // from the `index` and not from the order the fragments happened to arrive
    // in — index 1 is opened before index 0 is finished, above.
    assert_eq!(response.tool_calls, expected_calls());
    assert!(!response.final_output);
}

#[test]
fn a_tool_call_with_no_argument_fragments_is_read_as_an_empty_object() {
    // Non-streamed, a no-argument call arrives as `"arguments":"{}"` and parses.
    // Streamed, it accumulates to `""`, which `serde_json` rejects. Without this
    // equivalence streaming would introduce a failure the non-streamed path did
    // not have, which is the one thing the parity invariant forbids.
    let stub = Stub::serving(vec![Reply::ok(sse(vec![
        serde_json::json!({"choices": [{"index": 0, "delta": {"tool_calls": [
            {"index": 0, "id": "call_1", "type": "function",
             "function": {"name": "git_status"}}
        ]}}]}),
        finish("tool_calls"),
        usage(7),
    ]))]);
    let mut model = client(stub.base_url(), "llama3.1");

    let response = model
        .turn(&ask(vec![Message::user_text("what changed?")]))
        .expect("the stub answers");

    assert_eq!(
        response.tool_calls,
        vec![skein_core::ToolCall::with_id(
            "call_1",
            "git_status",
            serde_json::json!({})
        )]
    );
}

#[test]
fn a_reasoning_delta_never_reaches_the_message() {
    // The real provider sends `reasoning` on a reasoning model, with `content`
    // empty on those same events — and it sends it **in both modes**, so the
    // non-streamed `ChoiceMessage` was already discarding it. Absorbing it now
    // would put text into `TurnResponse.message` that the non-streamed path
    // never had, and would show an editor a chain of thought that appears
    // nowhere in the Ledger.
    let thinking = |text: &str| {
        serde_json::json!({
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": "", "reasoning": text}}]
        })
    };
    let stub = Stub::serving(vec![Reply::ok(sse(vec![
        thinking("The user wants a number. "),
        thinking("I should not say this part out loud."),
        delta("42"),
        finish("stop"),
        usage(61),
    ]))]);
    let mut model = client(stub.base_url(), "llama3.1");

    let response = model
        .turn(&ask(vec![Message::user_text("what is the answer?")]))
        .expect("the stub answers");

    assert_eq!(response.message, Message::assistant_text("42"));
}

#[test]
fn tokens_used_falls_back_to_prompt_plus_completion_when_total_is_absent() {
    let stub = Stub::serving(vec![Reply::ok(sse(vec![
        delta("ok"),
        finish("stop"),
        serde_json::json!({
            "choices": [],
            "usage": {"prompt_tokens": 31, "completion_tokens": 11}
        }),
    ]))]);
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
    //
    // Streaming is why this test is now load-bearing rather than defensive: the
    // real provider sends **no usage object at all** under a bare `stream: true`
    // and only sends one because the request asks for `stream_options`. A
    // provider that ignored that field would land exactly here.
    let stub = Stub::serving(vec![Reply::ok(sse(vec![
        delta("unmetered"),
        finish("stop"),
    ]))]);

    let message = turn_error(&stub, "how much did that cost?");
    assert!(
        message.contains("without token metering") && message.contains("token budget"),
        "the refusal must name the missing metering, got: {message}"
    );

    // And a `usage` that is present but half-filled is refused too: a sum needs
    // both halves.
    let partial = Stub::serving(vec![Reply::ok(sse(vec![
        delta("half-metered"),
        finish("stop"),
        serde_json::json!({"choices": [], "usage": {"prompt_tokens": 9}}),
    ]))]);
    assert!(
        turn_error(&partial, "and now?").contains("without token metering"),
        "a half-filled usage object is not metering"
    );
}

/// A loopback URL nothing is listening on: bind a kernel-assigned port to learn
/// a number that is certainly free, then drop the listener.
fn dead_loopback_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("the bound address").port();
    drop(listener);
    format!("http://127.0.0.1:{port}/v1")
}

#[test]
fn finish_reason_length_is_not_a_final_answer() {
    // `"length"` means the provider truncated the model mid-thought. Treating
    // it as a completed answer would let a truncation launder itself past
    // LoopController, which Constitution VIII(a) reserves to the engine.
    let stub = Stub::serving(vec![Reply::ok(provider_reply(
        "the first half of a thoug",
        "length",
        99,
    ))]);
    let mut model = client(stub.base_url(), "llama3.1");

    let response = model
        .turn(&ask(vec![Message::user_text("explain everything")]))
        .expect("the stub answers");

    assert!(
        !response.final_output,
        "a truncated answer is not a final answer"
    );
    // The turn is still metered: the tokens were really spent.
    assert_eq!(response.tokens_used, 99);
}

#[test]
fn tool_calls_are_translated_and_are_not_a_final_answer() {
    // This *request* advertises no tools, so a provider should not send these. It
    // translates them anyway: the chain records the TurnResponse and not the
    // raw body, so silently dropping a model intent would weaken Constitution
    // V. `content` is null on a tool-calling turn, which must not be a parse
    // failure.
    let stub = Stub::serving(vec![Reply::ok(sse(vec![
        serde_json::json!({
            "choices": [{
                "index": 0,
                "delta": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": "{\"path\":\"README.md\"}"
                        }
                    }]
                }
            }]
        }),
        finish("tool_calls"),
        serde_json::json!({"choices": [], "usage": {"total_tokens": 12}}),
    ]))]);
    let mut model = client(stub.base_url(), "llama3.1");

    let response = model
        .turn(&ask(vec![Message::user_text("read the readme")]))
        .expect("the stub answers");

    assert_eq!(
        response.tool_calls,
        vec![skein_core::ToolCall::with_id(
            "call_1",
            "read_file",
            serde_json::json!({"path": "README.md"})
        )],
        "the provider's own id is carried, not discarded: it is what the next
         request's tool message answers"
    );
    assert!(!response.final_output, "a tool request is not an answer");
    assert_eq!(response.message, Message::assistant_text(""));
}

#[test]
fn tool_calls_without_a_provider_id_are_given_positional_ones() {
    // Ollama supplies ids; the OpenAI-compat ecosystem does not guarantee them.
    // Normalizing here means every `ToolCall` leaving this crate has a non-empty
    // id, so the loop that echoes them needs no fallback of its own. Under
    // streaming the synthesized id is the delta's own `index`, which is what
    // keeps two id-less calls distinct.
    let stub = Stub::serving(vec![Reply::ok(sse(vec![
        serde_json::json!({
            "choices": [{
                "index": 0,
                "delta": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [
                        {"index": 0, "type": "function", "function": {
                            "name": "fs_read", "arguments": "{\"path\":\"alpha\"}"}},
                        {"index": 1, "type": "function", "function": {
                            "name": "fs_read", "arguments": "{\"path\":\"beta\"}"}}
                    ]
                }
            }]
        }),
        finish("tool_calls"),
        serde_json::json!({"choices": [], "usage": {"total_tokens": 12}}),
    ]))]);
    let mut model = client(stub.base_url(), "llama3.1");

    let response = model
        .turn(&ask(vec![Message::user_text("read both")]))
        .expect("the stub answers");

    assert_eq!(
        response
            .tool_calls
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        vec!["call_0", "call_1"],
        "distinct and non-empty, so the two answers cannot be confused"
    );
}

#[test]
fn an_unreachable_provider_fails_with_a_message_naming_the_endpoint() {
    let base_url = dead_loopback_url();
    let mut model = client(&base_url, "llama3.1");

    let message = match model.turn(&ask(vec![Message::user_text("anyone home?")])) {
        Ok(response) => panic!("expected a refusal, got {response:?}"),
        Err(SkeinError::Model(message)) => message,
        Err(other) => panic!("expected SkeinError::Model, got {other:?}"),
    };

    // The shape, not the OS's connection-refused wording, which differs across
    // platforms.
    assert!(
        message.contains(&base_url) && message.contains("is a local provider listening"),
        "the operator must be told which endpoint and what to check, got: {message}"
    );
}

#[test]
fn a_provider_error_status_carries_the_providers_own_message() {
    // Ollama's own 404 shape. `http_status_as_error(false)` is what lets this
    // reach the operator instead of being flattened into a status code.
    let stub = Stub::serving(vec![Reply::status(
        "404 Not Found",
        r#"{"error":{"message":"model \"nope\" not found, try pulling it first","type":"api_error"}}"#,
    )]);

    let message = turn_error(&stub, "use a model I do not have");
    assert!(
        message.contains("returned 404") && message.contains(r#"model \"nope\" not found"#),
        "the provider's own message must survive, got: {message}"
    );
    assert!(
        message.contains(stub.base_url()),
        "the endpoint must be named, got: {message}"
    );
}

#[test]
fn an_over_long_provider_error_is_cut_on_a_character_and_not_a_byte() {
    // A provider error long enough to be truncated, in a language whose
    // characters are not one byte each — a self-hosted OpenAI-compatible server
    // answering in French or Japanese is the ordinary case, not an exotic one.
    //
    // The single ASCII character in front is what gives this test teeth: it
    // makes every character in the body start at an *odd* byte offset, so the
    // cap's own number is strictly inside a character. Without it the body's
    // two-byte characters would land a byte slice on a boundary by parity, and
    // a truncation that indexed bytes instead of characters would pass.
    let body = format!("x{}", "é".repeat(410));
    let stub = Stub::serving(vec![Reply::status("502 Bad Gateway", body.clone())]);

    let message = turn_error(&stub, "ask something a proxy will refuse");

    assert!(
        message.contains("returned 502"),
        "the status must survive truncation, got: {message}"
    );
    assert!(
        message.ends_with('…'),
        "a cut body must say it was cut, got: {message}"
    );
    // Counted on the body's own character rather than on the whole message,
    // which also carries the endpoint prefix and so is not shorter than the
    // body it truncates.
    let survived = message.matches('é').count();
    assert!(
        survived > 0,
        "what survives the cut must still be the provider's own words, got: {message}"
    );
    assert!(
        survived < body.matches('é').count(),
        "the body must actually be shortened, but all {survived} characters survived"
    );
}

#[test]
fn an_unrecognised_response_body_is_refused() {
    // A 200 that is not an event stream at all — the shape an interposing proxy
    // produces. It must be refused *showing the body*, and not by falling
    // through to the metering refusal, which would tell the operator nothing
    // about what actually answered.
    let not_a_stream = Stub::serving(vec![Reply::ok("<html>upstream proxy says no</html>")]);
    let message = turn_error(&not_a_stream, "what is this");
    assert!(
        message.contains("unrecognised chat-completions response")
            && message.contains("upstream proxy says no"),
        "the body must be shown so the operator can see what answered, got: {message}"
    );

    // A well-framed stream whose events never carry a choice is equally
    // unusable, and must not become an empty answer. The metering event alone
    // is exactly that stream: `choices` is empty on it by design, so a client
    // that accepted it would answer every prompt with "".
    let no_choices = Stub::serving(vec![Reply::ok(sse(vec![usage(1)]))]);
    assert!(
        turn_error(&no_choices, "and this").contains("no choices[0]"),
        "a stream that never carried a choice is not an answer"
    );

    // And a `data:` payload that is framed but not JSON is refused with its own
    // bytes shown, rather than silently skipped as an unknown line would be.
    let garbled = Stub::serving(vec![Reply::ok("data: not-json-at-all\n\ndata: [DONE]\n\n")]);
    assert!(
        turn_error(&garbled, "and this").contains("not-json-at-all"),
        "an unparseable event must show itself"
    );
}

// ---------------------------------------------------------------------------
// Mid-stream cancellation (spec 026).
// ---------------------------------------------------------------------------

/// Records every delta and leaves `wants_more` defaulted. The default is what a
/// caller with nothing to cancel gets, and what every sink written before this
/// slice gets, so this sink must read the stream to its end.
struct RecordingSink(Arc<Mutex<Vec<String>>>);

impl TextSink for RecordingSink {
    fn on_text(&mut self, delta: &str) {
        self.0.lock().unwrap().push(delta.to_string());
    }
}

/// Records, and stops wanting text once `stop_after` deltas have arrived — the
/// gateway-side shape of a client pressing stop. `seen` is the ground truth for
/// what the read delivered before it ended.
struct StoppingSink {
    seen: Arc<Mutex<Vec<String>>>,
    stop_after: usize,
}

impl TextSink for StoppingSink {
    fn on_text(&mut self, delta: &str) {
        self.seen.lock().unwrap().push(delta.to_string());
    }

    fn wants_more(&self) -> bool {
        self.seen.lock().unwrap().len() < self.stop_after
    }
}

/// Four deltas spelled so that stopping after two is visibly different from
/// stopping after three, and a `[DONE]` that only an uncancelled read reaches.
fn four_delta_answer() -> String {
    sse(vec![
        delta("The "),
        delta("answer "),
        delta("is "),
        delta("42."),
        finish("stop"),
        usage(61),
    ])
}

/// One turn against a stub with `sink` installed, returning the refusal message
/// and the exchange the turn captured — the two things every test below asserts
/// on, and both of which exist only because the read stopped short.
fn cancelled_turn(stub: &Stub, sink: Box<dyn TextSink>) -> (String, skein_core::WireExchange) {
    let mut model = client(stub.base_url(), "llama3.1");
    model.set_text_sink(sink);
    let message = match model.turn(&ask(vec![Message::user_text("what is the answer?")])) {
        Ok(response) => panic!("expected a cancellation, got {response:?}"),
        Err(SkeinError::Model(message)) => message,
        Err(other) => panic!("expected SkeinError::Model, got {other:?}"),
    };
    let exchange = model
        .take_wire_exchange()
        .expect("a cancelled turn still reached a socket and still captured what arrived");
    (message, exchange)
}

#[test]
fn a_sink_that_stops_wanting_text_ends_the_read_mid_stream() {
    let stub = Stub::serving(vec![Reply::ok(four_delta_answer())]);
    let seen = Arc::new(Mutex::new(Vec::new()));

    let (message, _) = cancelled_turn(
        &stub,
        Box::new(StoppingSink {
            seen: seen.clone(),
            stop_after: 2,
        }),
    );

    // Equality, not a length bound: a read that stopped one line late would
    // deliver "is " as well, and that is precisely the off-by-one worth
    // catching.
    assert_eq!(
        *seen.lock().unwrap(),
        vec!["The ", "answer "],
        "the read must stop at the delta the sink stopped wanting more after"
    );
    assert!(
        message.contains("cancelled") && message.contains(stub.base_url()),
        "the refusal must name the cancellation and the endpoint, got: {message}"
    );
}

#[test]
fn a_cancelled_turns_capture_holds_what_arrived_and_no_done() {
    // Spec 026 FR-005: the bytes that arrived are the evidence of the
    // cancellation, and no field beside them says so.
    let stub = Stub::serving(vec![Reply::ok(four_delta_answer())]);
    let seen = Arc::new(Mutex::new(Vec::new()));

    let (_, exchange) = cancelled_turn(
        &stub,
        Box::new(StoppingSink {
            seen,
            stop_after: 2,
        }),
    );

    assert_eq!(exchange.status, 200);
    assert!(
        exchange.streamed,
        "a cancelled read is still a read of an event stream"
    );
    assert!(
        exchange.response.contains("The ") && exchange.response.contains("answer "),
        "the capture must hold the bytes that did arrive, got: {:?}",
        exchange.response
    );
    assert!(
        !exchange.response.contains("42."),
        "the capture must not hold bytes the read never absorbed, got: {:?}",
        exchange.response
    );
    assert!(
        !exchange.response.contains("[DONE]"),
        "the missing terminator is what makes the capture itself the evidence, got: {:?}",
        exchange.response
    );
}

#[test]
fn a_sink_that_stops_before_the_first_event_is_reported_as_cancelled_not_as_an_empty_stream() {
    // Spec 026 FR-004. `an_unrecognised_response_body_is_refused` pins the
    // "no SSE events" diagnostic, which was written for an interposing proxy's
    // page. A cancellation landing before the first event leaves exactly zero
    // events, so without the fault being raised first this turn would blame a
    // proxy for the operator's own stop button.
    let stub = Stub::serving(vec![Reply::ok(four_delta_answer())]);
    let seen = Arc::new(Mutex::new(Vec::new()));

    let (message, exchange) = cancelled_turn(
        &stub,
        Box::new(StoppingSink {
            seen: seen.clone(),
            stop_after: 0,
        }),
    );

    assert!(seen.lock().unwrap().is_empty(), "no delta was ever wanted");
    assert!(
        message.contains("cancelled"),
        "the refusal must name the cancellation, got: {message}"
    );
    assert!(
        !message.contains("unrecognised"),
        "a cancellation must not be reported as an unrecognised response, got: {message}"
    );
    assert!(
        exchange.response.is_empty(),
        "a read cancelled before its first line read nothing, got: {:?}",
        exchange.response
    );
}

#[test]
fn a_sink_that_does_not_override_wants_more_reads_the_whole_stream() {
    // Spec 026 FR-001: the default is `true`, so this slice is invisible to
    // every sink written before it.
    let stub = Stub::serving(vec![Reply::ok(four_delta_answer())]);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut model = client(stub.base_url(), "llama3.1");
    model.set_text_sink(Box::new(RecordingSink(seen.clone())));

    let response = model
        .turn(&ask(vec![Message::user_text("what is the answer?")]))
        .expect("the stub answers");

    assert_eq!(*seen.lock().unwrap(), vec!["The ", "answer ", "is ", "42."]);
    assert_eq!(
        response.message,
        Message::assistant_text("The answer is 42.")
    );
    assert!(model
        .take_wire_exchange()
        .expect("the turn reached a socket")
        .response
        .contains("[DONE]"));
}

#[test]
fn a_hanging_provider_times_out_rather_than_blocking_the_run() {
    let stub = Stub::serving(vec![Reply::stalled(Duration::from_secs(30))]);
    let mut model = OpenAiCompatClient::new(
        LocalEndpoint::parse(stub.base_url()).expect("a loopback base URL"),
        "llama3.1",
        Duration::from_millis(300),
    );

    let started = std::time::Instant::now();
    let message = match model.turn(&ask(vec![Message::user_text("take your time")])) {
        Ok(response) => panic!("expected a timeout, got {response:?}"),
        Err(SkeinError::Model(message)) => message,
        Err(other) => panic!("expected SkeinError::Model, got {other:?}"),
    };

    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the client must give up on its own budget, not on the server's"
    );
    assert!(
        message.contains("timeout") && message.contains(stub.base_url()),
        "the timeout must name the endpoint, got: {message}"
    );
}

/// A sink that stops after `stop_after` deltas and timestamps every one, so the
/// live test below can report how long the provider kept writing after the read
/// stopped wanting it.
struct TimingSink {
    seen: Arc<Mutex<Vec<(String, std::time::Instant)>>>,
    stop_after: usize,
}

impl TextSink for TimingSink {
    fn on_text(&mut self, delta: &str) {
        self.seen
            .lock()
            .unwrap()
            .push((delta.to_string(), std::time::Instant::now()));
    }

    fn wants_more(&self) -> bool {
        self.seen.lock().unwrap().len() < self.stop_after
    }
}

/// What a stub cannot prove about cancellation: that abandoning a **real**
/// provider's event stream mid-answer actually ends the turn quickly, leaves a
/// capture without its terminator, and leaves the client able to make the next
/// request — the last being the observable half of ureq keeping a half-read
/// connection out of its pool (spec 026 FR-006).
///
/// Run it the same way as [`a_live_local_provider_answers`]:
///
/// ```text
/// $env:SKEIN_LIVE_MODEL = "gemma4:latest"
/// cargo test -p skein-gateway --test openai_compat -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs a real local provider; set SKEIN_LIVE_MODEL to run"]
fn a_live_local_provider_stops_when_the_sink_stops_wanting_text() {
    let Some(model_name) = std::env::var_os("SKEIN_LIVE_MODEL") else {
        eprintln!("SKEIN_LIVE_MODEL is unset; skipping the live provider test");
        return;
    };
    let model_name = model_name.to_string_lossy().to_string();
    let base_url = std::env::var("SKEIN_MODEL_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());

    const STOP_AFTER: usize = 8;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut model = OpenAiCompatClient::new(
        LocalEndpoint::parse(&base_url).expect("a loopback base URL"),
        &model_name,
        Duration::from_secs(120),
    );
    model.set_text_sink(Box::new(TimingSink {
        seen: seen.clone(),
        stop_after: STOP_AFTER,
    }));

    // Long enough that the provider is certainly still writing when the sink
    // stops wanting it — the whole point is to cancel an answer in progress.
    let started = std::time::Instant::now();
    let error = model
        .turn(&ask(vec![Message::user_text(
            "Count from 1 to 300. One number per line. No other words.",
        )]))
        .expect_err("the sink stopped wanting text, so the turn must not succeed");
    let ended = std::time::Instant::now();

    // Copied out rather than held: the sink is still installed, and holding its
    // lock across the follow-up turn below would deadlock this thread against
    // its own `on_text`.
    let seen: Vec<(String, std::time::Instant)> = seen.lock().unwrap().clone();
    let exchange = model
        .take_wire_exchange()
        .expect("a cancelled turn still captured what arrived");
    let last_delta = seen.last().expect("the provider wrote something").1;
    eprintln!(
        "live cancel {model_name} @ {base_url}
  deltas    = {} (stopped after {STOP_AFTER})
           text      = {:?}
  turn took = {:?}, of which {:?} after the last delta
  capture   =          {} bytes, ends {:?}",
        seen.len(),
        seen.iter().map(|(d, _)| d.as_str()).collect::<String>(),
        ended - started,
        ended - last_delta,
        exchange.response.len(),
        &exchange.response[exchange.response.len().saturating_sub(60)..],
    );

    assert_eq!(
        seen.len(),
        STOP_AFTER,
        "the read must stop at the delta the sink stopped wanting more after"
    );
    assert!(
        format!("{error}").contains("cancelled"),
        "the refusal must name the cancellation, got: {error}"
    );
    assert!(
        !exchange.response.contains("[DONE]"),
        "a cancelled capture cannot hold the terminator the read never reached"
    );
    // The provider was still writing; a read that only stopped at the natural
    // end of a 300-line answer would take far longer than one more line.
    assert!(
        ended - last_delta < Duration::from_secs(5),
        "the turn must end at the next line, not at the end of the answer; took {:?}",
        ended - last_delta
    );

    // The client is still usable: the abandoned connection was not handed back
    // to the pool for the next request to inherit. A fresh sink, because the one
    // above has stopped wanting text for good.
    model.set_text_sink(Box::new(RecordingSink(Arc::new(Mutex::new(Vec::new())))));
    let next = model
        .turn(&ask(vec![Message::user_text("Reply with exactly: pong")]))
        .expect("the client is still usable after a cancelled turn");
    eprintln!("  next turn = {:?}", next.message.text());
    assert!(next.tokens_used > 0);
}

/// The one thing a stub cannot prove: that a **real** local provider answers
/// this wire format, and that it sends the token metering the loop's budget
/// depends on.
///
/// `#[ignore]`d, so `cargo test --workspace` stays green on a machine — and on
/// a CI runner — with no Ollama installed. `.github/workflows/core.yml` runs
/// `cargo test --workspace` without `--include-ignored`, so this never runs
/// there. Run it by hand:
///
/// ```text
/// $env:SKEIN_LIVE_MODEL = "llama3.1"
/// cargo test -p skein-gateway --test openai_compat -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs a real local provider; set SKEIN_LIVE_MODEL to run"]
fn a_live_local_provider_answers() {
    let Some(model_name) = std::env::var_os("SKEIN_LIVE_MODEL") else {
        // Skipping cleanly rather than failing: the test's absence of a
        // provider is a fact about the machine, not about the code.
        eprintln!("SKEIN_LIVE_MODEL is unset; skipping the live provider test");
        return;
    };
    let model_name = model_name.to_string_lossy().to_string();
    let base_url = std::env::var("SKEIN_MODEL_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());

    let mut model = OpenAiCompatClient::new(
        LocalEndpoint::parse(&base_url).expect("a loopback base URL"),
        &model_name,
        Duration::from_secs(120),
    );

    let response = model
        .turn(&ask(vec![Message::user_text(
            "Reply with exactly the word: pong",
        )]))
        .unwrap_or_else(|e| panic!("{base_url} did not answer for model {model_name:?}: {e}"));

    eprintln!(
        "live {model_name} @ {base_url}\n  content     = {:?}\n  tokens_used = {}\n  final_output = {}",
        response.message.text(),
        response.tokens_used,
        response.final_output
    );
    assert!(
        !response.message.text().is_empty(),
        "a live provider must return content"
    );
    // The risk a mocked test cannot cover: a real provider that sends no
    // `usage` would have made `turn` fail above, which is exactly the loud
    // refusal D8 chose over metering zero.
    assert!(
        response.tokens_used > 0,
        "a live provider must meter its own turn"
    );
}

/// The other thing a stub cannot prove: that the bytes captured off a **real**
/// provider are that provider's own, carrying metering a stub would only have
/// because the test wrote it there.
///
/// Run it the same way as [`a_live_local_provider_answers`]:
///
/// ```text
/// $env:SKEIN_LIVE_MODEL = "llama3.1"
/// cargo test -p skein-gateway --test openai_compat -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs a real local provider; set SKEIN_LIVE_MODEL to run"]
fn a_live_local_provider_exchange_is_captured_with_its_own_metering() {
    let Some(model_name) = std::env::var_os("SKEIN_LIVE_MODEL") else {
        eprintln!("SKEIN_LIVE_MODEL is unset; skipping the live provider test");
        return;
    };
    let model_name = model_name.to_string_lossy().to_string();
    let base_url = std::env::var("SKEIN_MODEL_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());

    let mut model = OpenAiCompatClient::new(
        LocalEndpoint::parse(&base_url).expect("a loopback base URL"),
        &model_name,
        Duration::from_secs(120),
    );

    let response = model
        .turn(&ask(vec![Message::user_text(
            "Reply with exactly the word: pong",
        )]))
        .unwrap_or_else(|e| panic!("{base_url} did not answer for model {model_name:?}: {e}"));
    let exchange = model
        .take_wire_exchange()
        .expect("a turn that reached a real socket captured its exchange");

    eprintln!(
        "live wire {model_name} @ {base_url}\n  url      = {}\n  status   = {}\n  request  = {}\n  response = {}",
        exchange.url, exchange.status, exchange.request, exchange.response
    );

    assert_eq!(exchange.status, 200);
    assert_eq!(exchange.url, format!("{base_url}/chat/completions"));

    let sent: serde_json::Value =
        serde_json::from_str(&exchange.request).expect("the captured request is JSON");
    assert_eq!(sent["model"], model_name);
    assert_eq!(
        sent["messages"][0]["content"],
        "Reply with exactly the word: pong"
    );

    // The capture is the event stream itself, not the object reassembled from
    // it, so it is framed and it ends where the provider ended it.
    assert!(
        exchange.response.starts_with("data: "),
        "the captured response must be the SSE framing, got {:?}",
        &exchange.response[..exchange.response.len().min(200)]
    );
    assert!(exchange.response.contains("data: [DONE]"));
    assert!(exchange.streamed, "a live turn that succeeded was streamed");

    // The provider's own `usage`, cross-checked against the number the loop
    // would have budgeted against. Two independently produced records of one
    // fact: the wire says it and the translation says it, and they agree. It is
    // on the stream at all only because the request asked for `stream_options`,
    // so this assertion is also the proof that the field is being honoured.
    let metering = live_events(&exchange.response)
        .into_iter()
        .find(|event| event.get("usage").is_some_and(|u| !u.is_null()))
        .expect("the stream carries the provider's own metering event");
    assert_eq!(
        metering["usage"]["total_tokens"], response.tokens_used,
        "the captured bytes must carry the metering the loop acted on"
    );

    // Taken, not borrowed: the second call must not re-offer the first's bytes.
    assert!(model.take_wire_exchange().is_none());
}

/// Every `data:` payload of a captured stream, parsed. Read off the capture
/// rather than off a second connection, so a live assertion is made against the
/// same bytes the chain would hold.
fn live_events(raw: &str) -> Vec<serde_json::Value> {
    raw.lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|payload| *payload != "[DONE]")
        .map(|payload| {
            serde_json::from_str(payload)
                .unwrap_or_else(|e| panic!("a captured event is not JSON: {e}: {payload}"))
        })
        .collect()
}

/// The third thing a stub cannot prove: that a **real** provider's streamed tool
/// call reassembles into the call the loop would act on — id, name and
/// arguments alike — however that provider chose to frame it across events.
///
/// Run it the same way as [`a_live_local_provider_answers`], with a model that
/// has the `tools` capability:
///
/// ```text
/// $env:SKEIN_LIVE_MODEL = "qwen3.8:27b"
/// cargo test -p skein-gateway --test openai_compat -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs a real tool-capable local provider; set SKEIN_LIVE_MODEL to run"]
fn a_live_local_provider_streams_a_tool_call() {
    let Some(model_name) = std::env::var_os("SKEIN_LIVE_MODEL") else {
        eprintln!("SKEIN_LIVE_MODEL is unset; skipping the live provider test");
        return;
    };
    let model_name = model_name.to_string_lossy().to_string();
    let base_url = std::env::var("SKEIN_MODEL_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());

    let mut model = OpenAiCompatClient::new(
        LocalEndpoint::parse(&base_url).expect("a loopback base URL"),
        &model_name,
        Duration::from_secs(300),
    );

    let response = model
        .turn(&TurnRequest {
            run_id: "run-live".into(),
            messages: vec![Message::user_text(
                "Read the file /etc/hosts. Use the fs_read tool.",
            )],
            tools: vec![ToolSpec::new(
                "fs_read",
                "Read a UTF-8 text file.",
                serde_json::json!({
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"],
                    "type": "object",
                }),
            )],
        })
        .unwrap_or_else(|e| panic!("{base_url} did not answer for model {model_name:?}: {e}"));
    let exchange = model
        .take_wire_exchange()
        .expect("a turn that reached a real socket captured its exchange");

    eprintln!(
        "live tool call {model_name} @ {base_url}\n  calls  = {:?}\n  tokens = {}\n  final  = {}",
        response.tool_calls, response.tokens_used, response.final_output
    );

    let call = response.tool_calls.first().unwrap_or_else(|| {
        panic!(
            "the model asked for no tool; it answered {:?}",
            response.message
        )
    });
    assert_eq!(call.tool, "fs_read");
    // The arguments reassembled into an object the gateway could parse, which a
    // half-accumulated `arguments` string could not have produced.
    assert!(
        call.args.get("path").and_then(|p| p.as_str()).is_some(),
        "the accumulated arguments must carry the model's own path: {:?}",
        call.args
    );
    assert!(!call.id.is_empty());
    assert!(!response.final_output, "a tool request is not an answer");

    // Every call the provider put on the wire survived the accumulation,
    // whether it framed them one per event or several in one. This is the
    // assertion that would catch an accumulator keeping only the last event's
    // calls, which is the plausible bug the `index` keying exists to prevent.
    let wired: usize = live_events(&exchange.response)
        .iter()
        .filter_map(|event| {
            event["choices"][0]["delta"]["tool_calls"]
                .as_array()
                .map(Vec::len)
        })
        .sum();
    assert_eq!(
        response.tool_calls.len(),
        wired,
        "every tool call on the wire must reach the TurnResponse"
    );
}
