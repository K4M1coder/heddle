//! The slice's reason to exist (spec 017, SC-008, SC-009): one governed run in
//! which **nothing between the model and the repository is a double**.
//!
//! The model is a stub, because a local model's competence is not what is under
//! test and a test that needed Ollama could not run in CI. Everything else is
//! the shipped article: a real socket serving OpenAI chat-completions bytes, the
//! real `OpenAiCompatClient`, the real `NativeLoop`, the real `ToolGateway` with
//! a real `ToolPolicy`, the real `LocalConnector`, the real `EmbeddedServer`,
//! and a real temporary repository with real commits written by `git2`. This
//! crate is the only one that can see all of them at once.
//!
//! The `Stub`/`request_body`/`tool_call_reply`/`final_reply`/`NoGroundTruth`
//! shapes are `governed_fs_run.rs`'s, **copied rather than shared** for the same
//! reason that file restates `skein chat`'s policy rather than importing it:
//! `skein-cli` has no `lib` target and Rust integration-test binaries do not
//! share helpers.
//!
//! Plain `#[test]`: the connector owns a runtime and blocks on it.

use git2::{Repository, Signature};
use skein_connectors::{local_connector, FsRoot, LocalConnector};
use skein_core::{
    Ledger, LoopBudget, LoopController, Message, NativeLoop, ProgressProbe, Redactor, Role,
    StepKind, ToolAccess, ToolGateway, ToolPolicy, TurnRequest,
};
use skein_gateway::{LocalEndpoint, OpenAiCompatClient};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;
use tempfile::TempDir;

/// Long enough that a slow runner never trips it, short enough that a client
/// which silently sends nothing fails as a failure rather than as a hang.
const OBSERVE_TIMEOUT: Duration = Duration::from_secs(10);

/// The branch every fixture is on, named here rather than inherited from
/// whatever `init.defaultBranch` the machine running the test happens to have.
const BRANCH: &str = "work";

const RUN: &str = "run-git";

/// A provider answering `replies` in order and reporting the request bodies it
/// was sent. The server thread asserts nothing, so a failure names the test
/// rather than a worker thread.
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
/// answers "no progress" rather than a convenient truth (Constitution VIII(b)).
struct NoGroundTruth;

impl ProgressProbe for NoGroundTruth {
    fn observe(&mut self) -> bool {
        false
    }
}

/// `skein chat`'s policy over a root that **is** a repository, restated here
/// rather than imported: `skein-cli` has no `lib` target.
///
/// The order is the order the model is told, and both git tools are
/// `ReadOnly` because neither mutates anything. `fs_write` stays off the list
/// for the reason `governed_fs_run.rs` records — a non-interactive command has
/// nobody to ask — and there is no git equivalent of that asymmetry, because
/// this slice built nothing to confirm.
fn chat_policy() -> ToolPolicy {
    ToolPolicy::new(
        vec![
            ("fs_read".to_string(), ToolAccess::ReadOnly),
            ("fs_list".to_string(), ToolAccess::ReadOnly),
            ("git_status".to_string(), ToolAccess::ReadOnly),
            ("git_log".to_string(), ToolAccess::ReadOnly),
        ],
        vec![],
    )
}

struct Harness {
    connector: LocalConnector,
    /// Declared **last**, for the reason `fs_root.rs`'s fixture records: the
    /// connector's root holds an open directory handle, and fields drop in
    /// declaration order.
    _dir: TempDir,
}

/// A real repository at the root, on [`BRANCH`], with one commit per entry of
/// `commits` and `notes.txt` left modified in the working tree afterwards — so
/// `git_status` has something true to report and `git_log` has real summaries.
fn harness(commits: &[&str]) -> Harness {
    let dir = TempDir::new().expect("a temp dir");
    let repo = Repository::init(dir.path()).expect("a repository is initialised");
    repo.set_head(&format!("refs/heads/{BRANCH}"))
        .expect("HEAD names the fixture's branch");

    for (i, message) in commits.iter().enumerate() {
        let name = format!("file-{i}.txt");
        std::fs::write(dir.path().join(&name), format!("contents of {name}\n"))
            .expect("a file to commit");
        commit(&repo, &name, message);
    }
    std::fs::write(dir.path().join("notes.txt"), "untracked and unstaged\n")
        .expect("a file in the worktree");

    Harness {
        connector: local_connector(FsRoot::new(dir.path()).expect("a canonicalizable root"))
            .expect("the embedded server starts"),
        _dir: dir,
    }
}

fn commit(repo: &Repository, path: &str, message: &str) {
    let mut index = repo.index().expect("the index opens");
    index.add_path(Path::new(path)).expect("the path is staged");
    index.write().expect("the index is written");
    let tree = repo
        .find_tree(index.write_tree().expect("the index writes a tree"))
        .expect("the tree is found");
    let who = Signature::now("Fixture Author", "fixture@example.invalid").expect("a signature");
    let parents: Vec<git2::Commit> = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok())
        .into_iter()
        .collect();
    repo.commit(
        Some("HEAD"),
        &who,
        &who,
        message,
        &tree,
        &parents.iter().collect::<Vec<_>>(),
    )
    .expect("the commit is written");
}

/// One governed run against `stub`, under [`chat_policy`], with `secrets`
/// configured for redaction. Returns the run's chain.
fn governed_run(stub: &Stub, connector: LocalConnector, secrets: Vec<String>) -> Ledger {
    let redactor = Redactor::new(secrets);
    let client = OpenAiCompatClient::new(
        LocalEndpoint::parse(&stub.base_url).expect("a loopback base URL"),
        "llama3.1",
        Duration::from_secs(10),
    );
    let mut loops = NativeLoop::new(
        client,
        NoGroundTruth,
        ToolGateway::new(connector, chat_policy(), redactor.clone()),
        redactor,
    );
    let mut ledger = Ledger::new();
    let mut controller = LoopController::new(LoopBudget::new(4, 1_000_000, 4));

    loops
        .run(
            RUN,
            Message::user_text("what has changed in this repository?"),
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
        .log(RUN)
        .into_iter()
        .filter(|s| s.kind == StepKind::LlmRequest)
        .map(|s| serde_json::from_str(&s.payload).expect("a captured TurnRequest"))
        .collect()
}

/// The last message of the run's final captured request: what the model was
/// told about the tool it asked for.
fn tool_feedback(ledger: &Ledger) -> String {
    let told = captured_requests(ledger)
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
fn a_model_asks_for_git_status_and_gets_the_real_repositorys_state_through_the_governed_gateway() {
    let stub = Stub::serving(vec![
        // No arguments at all, which is the shape of the tool: an empty object
        // is everything a model can say about `git_status`.
        tool_call_reply("git_status", serde_json::json!({})),
        final_reply("notes.txt is untracked."),
    ]);
    let Harness { _dir, connector } = harness(&["the only commit"]);
    let ledger = governed_run(&stub, connector, Vec::new());

    // 1. The first request tells the model what it can do, with the schemas the
    //    server derived — in allowlist order, and `fs_write` absent.
    let first = stub.request_body();
    let advertised = first["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("the first request must carry a tools array: {first}"));
    assert_eq!(
        advertised
            .iter()
            .map(|t| t["function"]["name"].as_str().expect("a tool name"))
            .collect::<Vec<_>>(),
        vec!["fs_read", "fs_list", "git_status", "git_log"]
    );
    assert_eq!(
        advertised[2]["function"]["parameters"]["properties"],
        serde_json::json!({}),
        "`git_status` is advertised with nothing to fill in, which is the whole \
         injection argument: {first}"
    );
    assert_eq!(
        advertised[3]["function"]["parameters"]["properties"]["count"]["type"],
        serde_json::json!("integer"),
        "`git_log`'s one argument is advertised as a number: {first}"
    );

    // 2. The real chain answered from the real repository: nothing in it is a
    //    double, and the porcelain the tool produced is what the model reads.
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
    assert!(
        fed_back.contains(&escaped("## work\n??\tnotes.txt"))
            && fed_back.contains("\"isError\":false"),
        "the repository's actual state must reach the model: {fed_back}"
    );

    // 3. The same thing, seen from the chain rather than from the wire.
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

    // 4. The governed sequence, and a chain that still verifies.
    assert_eq!(
        ledger
            .log(RUN)
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
        .verify_chain(RUN)
        .expect("a run that read a real repository still verifies");
}

#[test]
fn a_model_asks_for_git_log_and_gets_the_real_commit_summaries() {
    let stub = Stub::serving(vec![
        tool_call_reply("git_log", serde_json::json!({"count": 3})),
        final_reply("Three commits, newest first."),
    ]);
    let Harness { _dir, connector } = harness(&["the oldest work", "then more work", "the newest"]);

    let ledger = governed_run(&stub, connector, Vec::new());

    let told = tool_feedback(&ledger);
    assert!(told.contains("\"isError\":false"), "{told}");
    for summary in ["the newest", "then more work", "the oldest work"] {
        assert!(
            told.contains(summary),
            "every real commit summary must reach the model: {told}"
        );
    }
    // Newest first, as the tool promises: the positions of the summaries in the
    // fed-back text are the order the model reads them in.
    let position = |summary: &str| told.find(summary).expect("the summary is present");
    assert!(
        position("the newest") < position("then more work")
            && position("then more work") < position("the oldest work"),
        "newest first: {told}"
    );
    assert!(
        !told.contains("fixture@example.invalid"),
        "an author's email must never reach the chain: {told}"
    );
    ledger.verify_chain(RUN).expect("the chain verifies");
}

/// Long and distinctive, so a substring assertion cannot pass by accident.
const SECRET_IN_A_COMMIT: &str = "sk-from-a-commit-message-SECRET-abc123";

#[test]
fn a_secret_in_a_commit_message_is_scrubbed_from_every_payload_of_the_run() {
    let stub = Stub::serving(vec![
        tool_call_reply("git_log", serde_json::json!({"count": 2})),
        final_reply("I read the log."),
    ]);
    let Harness { _dir, connector } = harness(&[
        "an ordinary commit",
        &format!("oops, committed {SECRET_IN_A_COMMIT} in the subject"),
    ]);

    // Constitution V, verified rather than assumed. Slice 016 proved the
    // `Redactor` against a secret that came off disk in a file's contents; this
    // one is in a **commit message**, which reaches the chain by a different
    // route through the same gateway.
    let ledger = governed_run(&stub, connector, vec![SECRET_IN_A_COMMIT.to_string()]);

    let payloads: Vec<String> = ledger.log(RUN).iter().map(|s| s.payload.clone()).collect();
    assert!(
        payloads.iter().all(|p| !p.contains(SECRET_IN_A_COMMIT)),
        "no payload of the run may carry a configured secret: {payloads:?}"
    );
    assert!(
        payloads.iter().any(|p| p.contains("***")),
        "the scrubbing must be visible rather than the secret merely absent: {payloads:?}"
    );
    // `in the subject` proves the rest of the commit summary did come through,
    // so the secret's absence is redaction and not the log having failed. The
    // unconfigured case remains the same stated gap slice 016 recorded: a
    // credential the operator never registered still lands here in cleartext.
    assert!(
        payloads.iter().any(|p| p.contains("in the subject")),
        "only the configured value is scrubbed, not the commit: {payloads:?}"
    );
}

#[test]
fn a_crafted_count_is_refused_as_a_tool_error_and_the_run_survives() {
    let stub = Stub::serving(vec![
        tool_call_reply(
            "git_log",
            serde_json::json!({"count": "5 --upload-pack=touch pwned"}),
        ),
        final_reply("That is not a number."),
    ]);
    let Harness { _dir, connector } = harness(&["the only commit"]);

    // `git_log` *is* allowlisted, so the policy allows this and the server is
    // genuinely reached. The refusal comes from the typed boundary — `count` is
    // a `u32` — and rmcp reports an argument-deserialization failure as a
    // tool-level error rather than a protocol one, which is what lets the run
    // go on. There is no subprocess, no argument vector and no shell anywhere
    // in this slice for the crafted text to have become command structure in.
    let ledger = governed_run(&stub, connector, Vec::new());

    let told = tool_feedback(&ledger);
    assert!(
        told.contains("\"isError\":true"),
        "the *transport* succeeded and the refusal is inside the result, where the \
         model can read it and the run can continue — a transport failure would \
         have ended the run instead. Got: {told}"
    );
    assert_eq!(
        ledger.log(RUN).last().expect("a step").kind.clone(),
        StepKind::Exit,
        "the run must reach its own exit rather than dying on the refusal"
    );
    ledger
        .verify_chain(RUN)
        .expect("a run holding a tool-level refusal verifies");
}

/// The one thing a stub cannot prove: that a **real** local model, told about
/// these tools in this wire format, actually asks for one — and that what comes
/// back is the repository.
///
/// `#[ignore]`d, so `cargo test --workspace` stays green on a machine with no
/// Ollama; `.github/workflows/core.yml` runs it without `--include-ignored`, so
/// it never runs there. `a_live_model_calls_a_real_fs_tool`'s pattern exactly,
/// which exists so a hand-verification is repeatable rather than a one-off. Run
/// it by hand:
///
/// ```text
/// $env:SKEIN_LIVE_MODEL = "qwen3:8b"
/// cargo test -p skein-connectors --test governed_git_run -- --ignored --nocapture
/// ```
///
/// A zero-argument tool is the open question here rather than tool calling as
/// such: a model that will not call `git_status` because it has nothing to fill
/// in is a T13 finding to record, not a reason to invent a dummy parameter.
#[test]
#[ignore = "needs a real tool-capable local provider; set SKEIN_LIVE_MODEL to run"]
fn a_live_model_calls_a_real_git_tool() {
    let Some(model_name) = std::env::var_os("SKEIN_LIVE_MODEL") else {
        eprintln!("SKEIN_LIVE_MODEL is unset; skipping the live model tool-call test");
        return;
    };
    let model_name = model_name.to_string_lossy().to_string();
    let base_url = std::env::var("SKEIN_MODEL_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());
    let Harness { _dir, connector } = harness(&["the only commit"]);

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
            Message::user_text(
                "What has changed in this git repository, and what was the last commit?",
            ),
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
        results
            .iter()
            .any(|p| p.contains("git_status") || p.contains("git_log")),
        "the model was told it can read this repository and did not ask; if it will not call a \
         zero-argument tool that is a model-selection finding, not a defect: {:?}",
        ledger.log("run-live")
    );
    assert!(
        results.iter().any(|p| p.contains("the only commit")
            || p.contains(&escaped("## work"))
            || p.contains("notes.txt")),
        "something the real repository actually says must have reached the chain: {results:?}"
    );
    ledger
        .verify_chain("run-live")
        .expect("a live run's chain verifies");
}
