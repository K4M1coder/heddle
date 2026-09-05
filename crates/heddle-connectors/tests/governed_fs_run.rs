//! The slice's reason to exist (spec 016, SC-005): one governed run in which
//! **nothing between the model and the file is a double**.
//!
//! The model is a stub, because a local model's competence is not what is under
//! test and a test that needed Ollama could not run in CI. Everything else is
//! the shipped article: a real socket serving OpenAI chat-completions bytes, the
//! real `OpenAiCompatClient`, the real `NativeLoop`, the real `ToolGateway` with
//! a real `ToolPolicy`, the real `LocalConnector`, and the real `EmbeddedServer`
//! reading a real file off disk. This crate is the only one that can see all of
//! them at once, which is why the test lives here.
//!
//! Plain `#[test]`: the connector owns a runtime and blocks on it.

mod reparse;

use heddle_connectors::{local_connector, FsRoot, LocalConnector};
use heddle_core::{
    replay_tool_calls, Exit, Ledger, LoopBudget, LoopController, Message, NativeLoop,
    ProgressProbe, Redactor, Role, StepKind, ToolAccess, ToolGateway, ToolPolicy, TurnRequest,
};
use heddle_gateway::{LocalEndpoint, OpenAiCompatClient};
use reparse::reparse_dir;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;
use tempfile::TempDir;

/// Long enough that a slow runner never trips it, short enough that a client
/// which silently sends nothing fails as a failure rather than as a hang.
const OBSERVE_TIMEOUT: Duration = Duration::from_secs(10);

const FILE_CONTENTS: &str = "the first line of notes\nand a second one";

/// A provider answering `replies` in order and reporting the request bodies it
/// was sent. `openai_compat.rs`'s precedent: the server thread asserts nothing,
/// so a failure names the test rather than a worker thread.
struct Stub {
    base_url: String,
    requests: Receiver<String>,
}

impl Stub {
    fn serving(replies: Vec<String>) -> Stub {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let addr = listener.local_addr().expect("the bound address");
        let (tx, requests) = mpsc::channel();
        std::thread::spawn(move || {
            for body in replies {
                let Ok((mut socket, _)) = listener.accept() else {
                    return;
                };
                let Some(seen) = read_request(&mut socket) else {
                    return;
                };
                if tx.send(seen).is_err() {
                    return;
                }
                // `connection: close` makes each turn a fresh accept, so a
                // multi-turn run counts connections deterministically instead of
                // racing ureq's pool.
                let _ = socket.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
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

    /// The next request's body, parsed.
    fn request_body(&self) -> serde_json::Value {
        match self.requests.recv_timeout(OBSERVE_TIMEOUT) {
            Ok(raw) => {
                let (_, body) = raw
                    .replace('\r', "")
                    .split_once("\n\n")
                    .map(|(h, b)| (h.to_string(), b.to_string()))
                    .expect("headers and body separated by a blank line");
                serde_json::from_str(&body).expect("a JSON request body")
            }
            Err(RecvTimeoutError::Timeout) => {
                panic!("the client sent no request within {OBSERVE_TIMEOUT:?}")
            }
            Err(RecvTimeoutError::Disconnected) => {
                panic!("the stub server stopped before a request arrived")
            }
        }
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

/// SSE framing as the real provider writes it, with a bare `\n\n` separator and
/// a terminating `[DONE]`. Spelled out here rather than shared across test
/// binaries for the reason each of these files already records: they are one
/// another's controls.
fn sse(events: Vec<serde_json::Value>) -> String {
    let mut raw = String::new();
    for event in events {
        raw.push_str(&format!("data: {event}\n\n"));
    }
    raw.push_str("data: [DONE]\n\n");
    raw
}

/// A turn in which the model asks for one tool, shaped the way Ollama's
/// OpenAI-compatible endpoint shapes one: `content: null`, and the arguments as
/// a JSON *string* holding JSON.
fn tool_call_reply(tool: &str, arguments: serde_json::Value) -> String {
    sse(vec![
        serde_json::json!({
            "choices": [{"index": 0, "delta": {
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "index": 0,
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": tool, "arguments": arguments.to_string()}
                }]
            }}]
        }),
        serde_json::json!({
            "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]
        }),
        serde_json::json!({"choices": [], "usage": {"total_tokens": 12}}),
    ])
}

/// Two calls in one assistant turn, the way Ollama really answers when a model
/// reads several files at once, each with the provider's own id.
fn two_tool_calls_reply(calls: &[(&str, serde_json::Value)]) -> String {
    sse(vec![
        serde_json::json!({
            "choices": [{"index": 0, "delta": {
                "role": "assistant",
                "content": "",
                "tool_calls": calls.iter().enumerate().map(|(i, (tool, arguments))| {
                    serde_json::json!({
                        "index": i,
                        "id": format!("call_{i}"),
                        "type": "function",
                        "function": {"name": tool, "arguments": arguments.to_string()}
                    })
                }).collect::<Vec<_>>()
            }}]
        }),
        serde_json::json!({
            "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]
        }),
        serde_json::json!({"choices": [], "usage": {"total_tokens": 12}}),
    ])
}

fn final_reply(content: &str) -> String {
    sse(vec![
        serde_json::json!({
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": content}}]
        }),
        serde_json::json!({
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
        }),
        serde_json::json!({"choices": [], "usage": {"total_tokens": 9}}),
    ])
}

/// The run always ends on `final_output`, so this never decides anything. It
/// answers "no progress" rather than a convenient truth, matching the CLI's
/// `NoGroundTruth`: a probe that cannot see the model's words must not be given
/// the model's opinion (Constitution VIII(b)).
struct NoGroundTruth;

impl ProgressProbe for NoGroundTruth {
    fn observe(&mut self) -> bool {
        false
    }
}

/// `heddle chat`'s policy, restated here rather than imported: `heddle-cli` has no
/// `lib` target, and the asymmetry it encodes — `fs_write` is **not** on the
/// list, because a non-interactive command has nobody to ask — is exactly what
/// this file has to be able to assert.
fn chat_policy() -> ToolPolicy {
    ToolPolicy::new(
        vec![
            ("fs_read".to_string(), ToolAccess::ReadOnly),
            ("fs_list".to_string(), ToolAccess::ReadOnly),
        ],
        vec![],
    )
}

struct Harness {
    root: PathBuf,
    connector: LocalConnector,
    /// Declared **last**, for the reason `fs_root.rs`'s fixture records: the
    /// connector's root holds an open directory handle, and fields drop in
    /// declaration order.
    _dir: TempDir,
}

/// A root holding `files`, a sibling directory **outside** it holding
/// `secrets.txt`, and a connector over the root. The sibling is what
/// `../outside/secrets.txt` reaches for, so an escape attempt names a file that
/// really exists — otherwise the refusal could be nothing but "no such file".
fn harness(files: &[(&str, &str)]) -> Harness {
    let dir = TempDir::new().expect("a temp dir");
    let root = dir.path().join("root");
    let outside = dir.path().join("outside");
    std::fs::create_dir(&root).expect("the root is created");
    std::fs::create_dir(&outside).expect("the sibling is created");
    std::fs::write(outside.join("secrets.txt"), "not yours").expect("a file outside the root");
    for (name, contents) in files {
        std::fs::write(root.join(name), contents).expect("a file in the root");
    }

    Harness {
        connector: local_connector(FsRoot::new(&root).expect("a canonicalizable root"))
            .expect("the embedded server starts"),
        root,
        _dir: dir,
    }
}

/// One governed run against `stub`, under `policy`, with `secrets` configured
/// for redaction. Returns the run's chain. The caller keeps the harness's temp
/// directory alive, because it holds the files the run is about.
fn governed_run(
    stub: &Stub,
    connector: LocalConnector,
    policy: ToolPolicy,
    secrets: Vec<String>,
) -> Ledger {
    let redactor = Redactor::new(secrets);
    let client = OpenAiCompatClient::new(
        LocalEndpoint::parse(&stub.base_url).expect("a loopback base URL"),
        "llama3.1",
        Duration::from_secs(10),
    );
    let mut loops = NativeLoop::new(
        client,
        NoGroundTruth,
        ToolGateway::new(connector, policy, redactor.clone()),
        redactor,
    );
    let mut ledger = Ledger::new();
    let mut controller = LoopController::new(LoopBudget::new(4, 1_000_000, 4));

    loops
        .run(
            "run-fs",
            Message::user_text("what is in notes.txt?"),
            &mut ledger,
            &mut controller,
        )
        .expect("the run completes");
    ledger
}

/// A string as it appears once serialized into JSON: the form a tool result's
/// contents take by the time they are inside a `CallToolResult`.
fn escaped(text: &str) -> String {
    let quoted = serde_json::to_string(text).expect("a string serializes");
    quoted.trim_matches('"').to_string()
}

fn captured_requests(ledger: &Ledger, run_id: &str) -> Vec<TurnRequest> {
    ledger
        .log(run_id)
        .into_iter()
        .filter(|s| s.kind == StepKind::LlmRequest)
        .map(|s| serde_json::from_str(&s.payload).expect("a captured TurnRequest"))
        .collect()
}

#[test]
fn a_model_asks_for_a_file_and_gets_its_real_contents_through_the_governed_gateway() {
    let stub = Stub::serving(vec![
        tool_call_reply("fs_read", serde_json::json!({"path": "notes.txt"})),
        final_reply("the first line is: the first line of notes"),
    ]);
    let Harness {
        _dir,
        root: _root,
        connector,
    } = harness(&[("notes.txt", FILE_CONTENTS)]);
    let ledger = governed_run(&stub, connector, chat_policy(), Vec::new());

    // 1. The first request tells the model what it can do, with the schemas the
    //    server derived — and only what the policy allows, in allowlist order.
    //    `fs_write` exists on the server and is absent here.
    let first = stub.request_body();
    let advertised = first["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("the first request must carry a tools array: {first}"));
    assert_eq!(
        advertised
            .iter()
            .map(|t| t["function"]["name"].as_str().expect("a tool name"))
            .collect::<Vec<_>>(),
        vec!["fs_read", "fs_list"]
    );
    assert_eq!(
        advertised[0]["function"]["parameters"]["properties"]["path"]["type"],
        serde_json::json!("string"),
        "the advertised schema is the server's derived one: {first}"
    );

    // 2 & 3. The stub asked for `fs_read`, and the real chain answered from the
    //    real file: nothing in it is a double.
    let second = stub.request_body();
    let last = second["messages"]
        .as_array()
        .expect("a messages array")
        .last()
        .expect("the tool result is the last message");
    assert_eq!(
        (&last["role"], &last["tool_call_id"]),
        (&serde_json::json!("tool"), &serde_json::json!("call_1")),
        "on the wire too, not only in the chain: {last}"
    );
    let fed_back = last["content"].as_str().expect("text content");
    // The whole `CallToolResult` is what the transport hands back — `isError`
    // and any structured content are part of what the tool said — so the file's
    // bytes arrive JSON-escaped inside it. Asserting the escaped form is
    // `rmcp_gateway.rs`'s own move, and it is the honest shape rather than a
    // convenient one.
    assert!(
        fed_back.contains(&escaped(FILE_CONTENTS)) && fed_back.contains("\"isError\":false"),
        "the file's actual contents must reach the model: {fed_back}"
    );

    // 4. The same thing, seen from the chain rather than from the wire.
    let requests = captured_requests(&ledger, "run-fs");
    assert_eq!(requests.len(), 2, "one request per iteration");
    assert_eq!(
        requests[1]
            .messages
            .last()
            .expect("a fed-back result")
            .text(),
        fed_back,
        "the captured request and the wire must agree"
    );
    assert_eq!(
        requests
            .iter()
            .map(|r| r.tools.iter().map(|t| t.name.clone()).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        vec![
            vec!["fs_read".to_string(), "fs_list".to_string()],
            vec!["fs_read".to_string(), "fs_list".to_string()]
        ],
        "every turn of the run is told the same catalogue"
    );

    // 5. The governed sequence, and a chain that still verifies.
    assert_eq!(
        ledger
            .log("run-fs")
            .iter()
            .map(|s| s.kind.clone())
            .collect::<Vec<_>>(),
        vec![
            StepKind::IterationBoundary,
            StepKind::LlmRequest,
            StepKind::WireExchange,
            StepKind::LlmResponse,
            StepKind::BudgetSpent,
            StepKind::ToolCall,
            StepKind::Approval,
            StepKind::ToolResult,
            StepKind::IterationBoundary,
            StepKind::LlmRequest,
            StepKind::WireExchange,
            StepKind::LlmResponse,
            StepKind::BudgetSpent,
            StepKind::Exit,
        ]
    );
    ledger
        .verify_chain("run-fs")
        .expect("a run that called a real tool still verifies");
}

#[test]
fn two_reads_in_one_turn_are_answered_by_id_through_the_real_chain() {
    // The same property `native_loop.rs` proves against a scripted client, here
    // with a real `EmbeddedServer`, a real `OpenAiCompatClient` and a real
    // socket — and with the two files' contents differing, so a wrong pairing
    // is visible rather than merely unproven.
    let stub = Stub::serving(vec![
        two_tool_calls_reply(&[
            ("fs_read", serde_json::json!({"path": "gamma.txt"})),
            ("fs_read", serde_json::json!({"path": "alpha.txt"})),
        ]),
        final_reply("gamma holds 4 and alpha holds 7."),
    ]);
    let Harness {
        _dir,
        root: _root,
        connector,
    } = harness(&[("alpha.txt", "7"), ("gamma.txt", "4")]);

    let ledger = governed_run(&stub, connector, chat_policy(), Vec::new());

    let _first = stub.request_body();
    let second = stub.request_body();
    let messages = second["messages"].as_array().expect("a messages array");

    assert_eq!(
        messages[1]["tool_calls"]
            .as_array()
            .expect("the turn that asked is replayed with its calls")
            .iter()
            .map(|c| (
                c["id"].as_str().expect("an id"),
                c["function"]["arguments"].as_str().expect("a JSON string")
            ))
            .collect::<Vec<_>>(),
        vec![
            ("call_0", r#"{"path":"gamma.txt"}"#),
            ("call_1", r#"{"path":"alpha.txt"}"#)
        ],
        "arguments travel back as a JSON string, per the wire format: {second}"
    );

    // The pairing itself: the answer naming `call_0` holds gamma's contents.
    for (message, (id, contents)) in messages[2..].iter().zip([("call_0", "4"), ("call_1", "7")]) {
        assert_eq!(message["role"], serde_json::json!("tool"));
        assert_eq!(message["tool_call_id"], serde_json::json!(id));
        let body = message["content"].as_str().expect("text content");
        assert!(
            body.contains(&format!(r#""text":"{contents}""#)),
            "{id} must answer with the contents of the file it named: {body}"
        );
    }

    ledger
        .verify_chain("run-fs")
        .expect("a run answering two calls by id still verifies");
}

/// The last message of the run's final captured request: what the model was
/// told about the tool it asked for.
fn tool_feedback(ledger: &Ledger) -> String {
    let told = captured_requests(ledger, "run-fs")
        .last()
        .expect("a second request")
        .messages
        .last()
        .expect("a fed-back tool result")
        .clone();
    // The envelope, checked once here so every caller's subject is the body:
    // the result is external content by its role, and it names the call the
    // stub made rather than resting on its position in the history.
    assert_eq!(told.role, Role::Tool);
    assert_eq!(told.tool_call_id.as_deref(), Some("call_1"));
    told.text()
}

#[test]
fn an_unlisted_write_never_reaches_the_server() {
    let stub = Stub::serving(vec![
        tool_call_reply(
            "fs_write",
            serde_json::json!({"path": "planted.txt", "content": "planted"}),
        ),
        final_reply("I was not allowed to write."),
    ]);
    let Harness {
        _dir,
        root,
        connector,
    } = harness(&[("notes.txt", FILE_CONTENTS)]);

    // `chat_policy` does not allowlist `fs_write` at all. The server implements
    // it, and would have written the file — so its **absence on disk** is the
    // ground truth that nothing downstream of the policy ran. Not a counter in
    // the server: an effect the server would have had.
    let ledger = governed_run(&stub, connector, chat_policy(), Vec::new());

    assert!(
        !root.join("planted.txt").exists(),
        "an unlisted mutating tool must have had no effect whatsoever"
    );
    let told = tool_feedback(&ledger);
    assert_eq!(
        told, "the fs_write tool call was refused: tool is not in the allowlist",
        "the model must be told plainly why"
    );
    // The refusal is history, not an error: the attempt and the verdict are on
    // the chain, there is no `ToolResult` because nothing was executed, and the
    // run went on to answer.
    assert_eq!(
        ledger
            .log("run-fs")
            .iter()
            .filter(|s| matches!(
                s.kind,
                StepKind::ToolCall | StepKind::Approval | StepKind::ToolResult
            ))
            .map(|s| s.kind.clone())
            .collect::<Vec<_>>(),
        vec![StepKind::ToolCall, StepKind::Approval]
    );
    ledger
        .verify_chain("run-fs")
        .expect("a run holding a denial verifies");
}

#[test]
fn an_out_of_root_read_is_refused_by_the_server_and_the_run_survives() {
    let stub = Stub::serving(vec![
        tool_call_reply(
            "fs_read",
            serde_json::json!({"path": "../outside/secrets.txt"}),
        ),
        final_reply("That file is outside my root."),
    ]);
    let Harness {
        _dir,
        root: _root,
        connector,
    } = harness(&[("notes.txt", FILE_CONTENTS)]);

    // `fs_read` *is* allowlisted, so the policy allows this and the server is
    // genuinely reached. Containment is the server's decision, not the
    // governor's, and this is the test that says which layer refused.
    let ledger = governed_run(&stub, connector, chat_policy(), Vec::new());

    let told = tool_feedback(&ledger);
    assert!(
        told.contains("\"isError\":true") && told.contains("outside the root"),
        "the *transport* succeeded and the refusal is inside the result, where the \
         model can read it and the run can continue — a transport failure would \
         have ended the run instead. Got: {told}"
    );
    assert!(
        !told.contains("not yours"),
        "the out-of-root file's contents must not appear anywhere: {told}"
    );
    ledger
        .verify_chain("run-fs")
        .expect("a run holding a tool-level refusal verifies");
}

/// The governed counterpart of `fs_server.rs`'s reparse-point test, and the one
/// that matters for spec 021: the escape is planted **after** the server — and
/// so its root handle — already exists, which is precisely the window the old
/// canonicalize-then-open mechanism left open.
#[test]
fn a_read_through_a_reparse_point_planted_after_the_server_is_refused() {
    let stub = Stub::serving(vec![
        tool_call_reply("fs_read", serde_json::json!({"path": "sub/secrets.txt"})),
        final_reply("That path leads outside my root."),
    ]);
    let Harness {
        root,
        connector,
        _dir,
    } = harness(&[("notes.txt", FILE_CONTENTS)]);

    let outside = root
        .parent()
        .expect("the fixture root has a parent")
        .join("outside");
    let swapped = root.join("sub");
    if reparse_dir(&outside, &swapped).is_err() {
        eprintln!("this machine does not permit creating reparse points; skipping");
        return;
    }
    assert_eq!(
        std::fs::read_to_string(swapped.join("secrets.txt")).expect("the swap really escapes"),
        "not yours",
        "positive control: without containment this path reads the outside file"
    );

    let ledger = governed_run(&stub, connector, chat_policy(), Vec::new());

    let told = tool_feedback(&ledger);
    assert!(
        told.contains("\"isError\":true") && told.contains("outside the root"),
        "the *transport* succeeded and the refusal is inside the result, where the \
         model can read it and the run can continue — a transport failure would \
         have ended the run instead. Got: {told}"
    );
    assert!(
        !told.contains("not yours"),
        "the out-of-root file's contents must not appear anywhere: {told}"
    );
    ledger
        .verify_chain("run-fs")
        .expect("a run holding a tool-level refusal verifies");
}

/// Long and distinctive, so a substring assertion cannot pass by accident.
const SECRET_ON_DISK: &str = "sk-from-disk-SECRET-abc123";

#[test]
fn a_secret_in_a_files_contents_is_scrubbed_from_the_chain() {
    let contents = format!("api_key={SECRET_ON_DISK}\nendpoint=http://localhost:11434");
    let stub = Stub::serving(vec![
        tool_call_reply("fs_read", serde_json::json!({"path": "config.txt"})),
        final_reply("I read the config."),
    ]);
    let Harness {
        _dir,
        root,
        connector,
    } = harness(&[("config.txt", &contents)]);

    // Constitution V, verified rather than assumed: `Redactor` has only ever
    // been proven against a secret the *model* or a *stub tool* produced. Here
    // it came off disk, through a real server, in a real tool result.
    let ledger = governed_run(
        &stub,
        connector,
        chat_policy(),
        vec![SECRET_ON_DISK.to_string()],
    );

    assert!(
        std::fs::read_to_string(root.join("config.txt"))
            .expect("the file is still there")
            .contains(SECRET_ON_DISK),
        "sanity: the file really holds the secret, so only redaction can explain its absence below"
    );
    let payloads: Vec<String> = ledger
        .log("run-fs")
        .iter()
        .map(|s| s.payload.clone())
        .collect();
    assert!(
        payloads.iter().all(|p| !p.contains(SECRET_ON_DISK)),
        "no payload of the run may carry a configured secret: {payloads:?}"
    );
    assert!(
        payloads.iter().any(|p| p.contains("***")),
        "the scrubbing must be visible rather than the secret merely absent: {payloads:?}"
    );
    // The unconfigured case is *not* covered, and the spec says so plainly: a
    // credential in a file the operator never registered still lands here in
    // cleartext. `endpoint=` proves the rest of the file did come through.
    assert!(
        payloads.iter().any(|p| p.contains("endpoint=")),
        "only the configured value is scrubbed, not the file: {payloads:?}"
    );
}

/// The same shape as `SECRET_ON_DISK`, and one character different in the way
/// that matters: a quote, so the secret is on an already-serialized tool result
/// in escaped form rather than as written.
const AWKWARD_ON_DISK: &str = "sk-\"awkward\"-SECRET-abc123";

/// The decoded text inside a serialized result body. Every assertion below goes
/// through here, and the assertions above deliberately do not — which is the
/// finding this test exists to encode.
///
/// A `contains` over a step payload cannot see this secret at all. The
/// `ToolResult` payload is a serialized `CapturedResult` whose `content` is
/// *itself* serialized JSON, so a quote in the secret is escaped **twice**
/// there: the payload holds `sk-\\\"awkward…`, which contains neither the
/// literal needle nor `escaped()`'s singly-escaped one. Measured before this
/// test was written: with the defect present, `SECRET_ON_DISK`'s assertion
/// shape reported the run clean while the secret was on four payloads and in
/// the body the provider received.
fn body_text(body: &str) -> String {
    let parsed: serde_json::Value =
        serde_json::from_str(body).expect("a result body is still the JSON it was");
    parsed["content"][0]["text"]
        .as_str()
        .expect("scrubbing leaves the body's shape intact")
        .to_string()
}

#[test]
fn a_secret_with_a_quote_in_it_is_scrubbed_from_a_real_tool_result() {
    let contents = format!("api_key={AWKWARD_ON_DISK}\nendpoint=http://localhost:11434");
    let stub = Stub::serving(vec![
        tool_call_reply("fs_read", serde_json::json!({"path": "config.txt"})),
        final_reply("I read the config."),
    ]);
    let Harness {
        _dir,
        root,
        connector,
    } = harness(&[("config.txt", &contents)]);

    // The escaping has to arise from a **real** server rather than from a
    // double's `format!`, because that is the premise the fix rests on:
    // `heddle-mcp` hands the port `serde_json::to_string(&CallToolResult)`.
    let ledger = governed_run(
        &stub,
        connector,
        chat_policy(),
        vec![AWKWARD_ON_DISK.to_string()],
    );

    assert!(
        std::fs::read_to_string(root.join("config.txt"))
            .expect("the file is still there")
            .contains(AWKWARD_ON_DISK),
        "sanity: the file really holds the secret, so only redaction can explain its absence below"
    );

    let scrubbed = "api_key=***\nendpoint=http://localhost:11434";
    assert_eq!(
        body_text(&replay_tool_calls(&ledger, "run-fs").expect("the run replays")[0].content),
        scrubbed,
        "the ToolResult capture must not carry a configured secret in escaped form"
    );
    // The capture is also what `NativeLoop::mediate` feeds back, so this is the
    // assertion that the secret never reached the provider either — the copy
    // where it would be escaped twice and past reach of any needle.
    assert_eq!(
        body_text(&tool_feedback(&ledger)),
        scrubbed,
        "the model must be told the scrubbed body, not the raw one"
    );
    ledger
        .verify_chain("run-fs")
        .expect("a run holding a scrubbed tool result still verifies");
}

/// The one thing a stub cannot prove: that a **real** local model, told about
/// these tools in this wire format, actually asks for one — and that what comes
/// back is the file.
///
/// `#[ignore]`d, so `cargo test --workspace` stays green on a machine with no
/// Ollama; `.github/workflows/core.yml` runs it without `--include-ignored`, so
/// it never runs there. This is `openai_compat.rs`'s
/// `a_live_local_provider_answers` pattern, which exists so a hand-verification
/// is repeatable rather than a one-off. Run it by hand:
///
/// ```text
/// $env:HEDDLE_LIVE_MODEL = "qwen3:8b"
/// cargo test -p heddle-connectors --test governed_fs_run -- --ignored --nocapture
/// ```
///
/// Not every Ollama model supports tool calling. If a model ignores the
/// `tools` array this fails, and that is a model-selection finding rather than
/// a code defect.
#[test]
#[ignore = "needs a real tool-capable local provider; set HEDDLE_LIVE_MODEL to run"]
fn a_live_model_calls_a_real_fs_tool() {
    let Some(model_name) = std::env::var_os("HEDDLE_LIVE_MODEL") else {
        eprintln!("HEDDLE_LIVE_MODEL is unset; skipping the live model tool-call test");
        return;
    };
    let model_name = model_name.to_string_lossy().to_string();
    let base_url = std::env::var("HEDDLE_MODEL_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());
    let Harness {
        _dir,
        root: _root,
        connector,
    } = harness(&[("notes.txt", FILE_CONTENTS)]);

    let redactor = Redactor::new(Vec::new());
    let mut loops = NativeLoop::new(
        OpenAiCompatClient::new(
            LocalEndpoint::parse(&base_url).expect("a loopback base URL"),
            &model_name,
            Duration::from_secs(120),
        ),
        NoGroundTruth,
        ToolGateway::new(connector, chat_policy(), redactor.clone()),
        redactor,
    );
    let mut ledger = Ledger::new();
    let mut controller = LoopController::new(LoopBudget::new(4, 1_000_000, 4));

    let run = loops
        .run(
            "run-live",
            Message::user_text("Read the file notes.txt and tell me its first line."),
            &mut ledger,
            &mut controller,
        )
        .unwrap_or_else(|e| panic!("{base_url} did not complete a run for {model_name:?}: {e}"));

    for step in ledger.log("run-live") {
        eprintln!("{:>20}  {}", format!("{:?}", step.kind), step.payload);
    }
    eprintln!("exit = {:?}\nanswer = {:?}", run.exit, run.final_message);

    let results: Vec<String> = ledger
        .log("run-live")
        .iter()
        .filter(|s| s.kind == StepKind::ToolResult)
        .map(|s| s.payload.clone())
        .collect();
    assert!(
        results.iter().any(|p| p.contains("fs_read")),
        "the model was told it can read files and did not ask; if it cannot call tools that is a \
         model-selection finding, not a defect: {:?}",
        ledger.log("run-live")
    );
    assert!(
        results.iter().any(|p| p.contains(&escaped(FILE_CONTENTS))),
        "the real file's contents must have reached the chain: {results:?}"
    );
    ledger
        .verify_chain("run-live")
        .expect("a live run's chain verifies");
}

/// The multi-call shape against a real model, which is the only place the
/// slice's own claim can be checked end to end: a provider that accepts the
/// echoed `tool_calls`, and a model that answers from ids rather than from the
/// order it happened to read in. `#[ignore]`d for the same reason its sibling
/// above is — `cargo test --workspace` must stay green with nothing listening.
///
/// ```text
/// $env:HEDDLE_LIVE_MODEL = "gemma4:latest"
/// cargo test -p heddle-connectors --test governed_fs_run -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs a real tool-capable local provider; set HEDDLE_LIVE_MODEL to run"]
fn a_live_model_reads_two_files_and_is_answered_by_id() {
    let Some(model_name) = std::env::var_os("HEDDLE_LIVE_MODEL") else {
        eprintln!("HEDDLE_LIVE_MODEL is unset; skipping the live two-file test");
        return;
    };
    let model_name = model_name.to_string_lossy().to_string();
    let base_url = std::env::var("HEDDLE_MODEL_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());
    // Distinct contents, so a misattributed pairing is visible in the answer
    // rather than merely unproven.
    let Harness {
        _dir,
        root: _root,
        connector,
    } = harness(&[("alpha.txt", "7"), ("gamma.txt", "4")]);

    let redactor = Redactor::new(Vec::new());
    let mut loops = NativeLoop::new(
        OpenAiCompatClient::new(
            LocalEndpoint::parse(&base_url).expect("a loopback base URL"),
            &model_name,
            Duration::from_secs(120),
        ),
        NoGroundTruth,
        ToolGateway::new(connector, chat_policy(), redactor.clone()),
        redactor,
    );
    let mut ledger = Ledger::new();
    let mut controller = LoopController::new(LoopBudget::new(6, 1_000_000, 6));

    let run = loops
        .run(
            "run-live",
            Message::user_text(
                "Read the files alpha.txt and gamma.txt. Reply with one line per file, \
                 in the form <name>=<contents>.",
            ),
            &mut ledger,
            &mut controller,
        )
        .unwrap_or_else(|e| panic!("{base_url} did not complete a run for {model_name:?}: {e}"));

    for step in ledger.log("run-live") {
        eprintln!("{:>20}  {}", format!("{:?}", step.kind), step.payload);
    }
    eprintln!("exit = {:?}\nanswer = {:?}", run.exit, run.final_message);

    let reads = ledger
        .log("run-live")
        .iter()
        .filter(|s| s.kind == StepKind::ToolResult)
        .count();
    assert_eq!(
        reads,
        2,
        "the model was told it can read files and did not read both; a model that cannot \
         drive two calls is a model-selection finding, not a defect: {:?}",
        ledger.log("run-live")
    );
    assert_eq!(run.exit, Exit::FinalOutput);

    // The point of the whole slice, seen on a real conversation: every answer
    // the provider was sent names a call the provider itself asked for.
    let answered: Vec<String> = captured_requests(&ledger, "run-live")
        .into_iter()
        .flat_map(|r| r.messages)
        .filter(|m| m.role == Role::Tool)
        .map(|m| m.tool_call_id.expect("a live tool answer names its call"))
        .collect();
    let asked: Vec<String> = captured_requests(&ledger, "run-live")
        .into_iter()
        .flat_map(|r| r.messages)
        .flat_map(|m| m.tool_calls)
        .map(|c| c.id)
        .collect();
    assert!(
        !answered.is_empty() && answered.iter().all(|id| asked.contains(id)),
        "asked {asked:?}, answered {answered:?}"
    );
    ledger
        .verify_chain("run-live")
        .expect("a live run's chain verifies");
}
