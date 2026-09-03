//! The slice's reason to exist (spec 016, SC-005): one governed run in which
//! **nothing between the model and the file is a double**.
//!
//! The model is a stub, because a local model's competence is not what is under
//! test and a test that needed Ollama could not run in CI. Everything else is
//! the shipped article: a real socket serving OpenAI chat-completions bytes, the
//! real `OpenAiCompatClient`, the real `NativeLoop`, the real `ToolGateway` with
//! a real `ToolPolicy`, the real `LocalConnector`, and the real `FsServer`
//! reading a real file off disk. This crate is the only one that can see all of
//! them at once, which is why the test lives here.
//!
//! Plain `#[test]`: the connector owns a runtime and blocks on it.

use skein_connectors::{fs_connector, FsRoot, LocalConnector};
use skein_core::{
    Ledger, LoopBudget, LoopController, Message, NativeLoop, ProgressProbe, Redactor, StepKind,
    ToolAccess, ToolGateway, ToolPolicy, TurnRequest,
};
use skein_gateway::{LocalEndpoint, OpenAiCompatClient};
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
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
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

/// A turn in which the model asks for one tool, shaped the way Ollama's
/// OpenAI-compatible endpoint shapes one: `content: null`, and the arguments as
/// a JSON *string* holding JSON.
fn tool_call_reply(tool: &str, arguments: serde_json::Value) -> String {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": tool, "arguments": arguments.to_string()}
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {"total_tokens": 12}
    })
    .to_string()
}

fn final_reply(content: &str) -> String {
    serde_json::json!({
        "choices": [{
            "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"
        }],
        "usage": {"total_tokens": 9}
    })
    .to_string()
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

/// `skein chat`'s policy, restated here rather than imported: `skein-cli` has no
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
    _dir: TempDir,
    root: PathBuf,
    connector: LocalConnector,
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
        connector: fs_connector(FsRoot::new(&root).expect("a canonicalizable root"))
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

fn captured_requests(ledger: &Ledger) -> Vec<TurnRequest> {
    ledger
        .log("run-fs")
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
    let fed_back = second["messages"]
        .as_array()
        .expect("a messages array")
        .last()
        .expect("the tool result is the last message")["content"]
        .as_str()
        .expect("text content");
    assert!(
        fed_back.starts_with("[tool_result tool=fs_read status=ok]"),
        "{fed_back}"
    );
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
    let requests = captured_requests(&ledger);
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
            StepKind::LlmResponse,
            StepKind::BudgetSpent,
            StepKind::ToolCall,
            StepKind::Approval,
            StepKind::ToolResult,
            StepKind::IterationBoundary,
            StepKind::LlmRequest,
            StepKind::LlmResponse,
            StepKind::BudgetSpent,
            StepKind::Exit,
        ]
    );
    ledger
        .verify_chain("run-fs")
        .expect("a run that called a real tool still verifies");
}

/// The last message of the run's final captured request: what the model was
/// told about the tool it asked for.
fn tool_feedback(ledger: &Ledger) -> String {
    captured_requests(ledger)
        .last()
        .expect("a second request")
        .messages
        .last()
        .expect("a fed-back tool result")
        .text()
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
    assert!(
        told.starts_with("[tool_result tool=fs_write status=denied]")
            && told.contains("not in the allowlist"),
        "the model must be told plainly why, got: {told}"
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
        told.starts_with("[tool_result tool=fs_read status=ok]"),
        "`ok` is right and load-bearing: the *transport* succeeded, and the \
         refusal is inside the result where the model can read it — a transport \
         failure would have ended the run. Got: {told}"
    );
    assert!(
        told.contains("\"isError\":true") && told.contains("outside the root"),
        "the server's own refusal must reach the model: {told}"
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
