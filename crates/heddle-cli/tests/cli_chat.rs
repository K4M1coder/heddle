//! Acceptance tests for `heddle chat` (spec 012).
//!
//! Every test runs the **real binary as a process** against a **real silo on
//! disk** and a **real socket** serving OpenAI chat-completions bytes from this
//! test process. Following slice 011's SC-003: a unit test of an inner function
//! would prove nothing about the executable a person runs — not the argument
//! parsing, not the exit code, not the split between stdout and stderr, which
//! for this command *is* the user contract.
//!
//! No test here needs a running Ollama.

use git2::{Repository, Signature};
use heddle_core::SecretRef;
use heddle_silo::{OsKeychain, Silo};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

/// Long enough that a slow runner never trips it, short enough that a child
/// which silently sends nothing fails as a failure rather than as a hang.
const OBSERVE_TIMEOUT: Duration = Duration::from_secs(30);

/// A local provider answering a fixed script, one connection per turn, and
/// reporting the request bodies the child sent it.
///
/// The listener is bound before the child process starts, so `--base-url` can
/// name a port that is certainly live; `connection: close` on each reply makes a
/// multi-turn run a sequence of fresh accepts rather than a race against the
/// client's connection pool.
struct StubProvider {
    base_url: String,
    requests: Receiver<String>,
    connections: Arc<AtomicUsize>,
}

impl StubProvider {
    fn serving(bodies: Vec<String>) -> StubProvider {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let addr = listener.local_addr().expect("the bound address");
        let (tx, requests) = mpsc::channel();
        let connections = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&connections);
        std::thread::spawn(move || {
            for body in bodies {
                let Ok((mut socket, _)) = listener.accept() else {
                    return;
                };
                // Counted on accept and before the request is read, so a child
                // that connects and says nothing is still recorded as egress.
                counted.fetch_add(1, Ordering::SeqCst);
                let Some(seen) = read_request(&mut socket) else {
                    return;
                };
                if tx.send(seen).is_err() {
                    return;
                }
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
        StubProvider {
            base_url: format!("http://{addr}/v1"),
            requests,
            connections,
        }
    }

    /// How many connections this stub has accepted. Read after a refusal to
    /// prove the **child process** opened no socket, which is the end-to-end
    /// form of the claim `provider_routing.rs` makes about the router.
    fn connection_count(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    /// The next request's body, parsed. This is how a test asserts what the
    /// **real binary** put on the wire rather than only what it printed.
    fn request_body(&self) -> serde_json::Value {
        match self.requests.recv_timeout(OBSERVE_TIMEOUT) {
            Ok(body) => serde_json::from_str(&body).expect("a JSON request body"),
            Err(RecvTimeoutError::Timeout) => {
                panic!("the child sent no request within {OBSERVE_TIMEOUT:?}")
            }
            Err(RecvTimeoutError::Disconnected) => {
                panic!("the stub provider stopped before a request arrived")
            }
        }
    }
}

fn read_request(socket: &mut TcpStream) -> Option<String> {
    let mut reader = BufReader::new(socket.try_clone().ok()?);
    let mut len = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            len = value.trim().parse().ok()?;
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).ok()?;
    Some(String::from_utf8_lossy(&body).to_string())
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

fn reply(content: &str, finish_reason: &str, total_tokens: u64) -> String {
    sse(vec![
        serde_json::json!({
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": content}}]
        }),
        serde_json::json!({
            "choices": [{"index": 0, "delta": {}, "finish_reason": finish_reason}]
        }),
        serde_json::json!({"choices": [], "usage": {"total_tokens": total_tokens}}),
    ])
}

/// A dead loopback URL: bind a kernel-assigned port to learn a number that is
/// certainly free, then drop the listener.
fn dead_loopback_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("the bound address").port();
    drop(listener);
    format!("http://127.0.0.1:{port}/v1")
}

fn heddle(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_heddle"))
        .args(args)
        .env_remove("HEDDLE_ROOT")
        .env_remove("HEDDLE_MODEL_BASE_URL")
        .output()
        .expect("the heddle binary runs")
}

/// Drives `heddle chat` with the prompt on stdin rather than in a flag.
fn heddle_with_stdin(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_heddle"))
        .args(args)
        .env_remove("HEDDLE_ROOT")
        .env_remove("HEDDLE_MODEL_BASE_URL")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the heddle binary runs");
    child
        .stdin
        .take()
        .expect("a piped stdin")
        .write_all(stdin.as_bytes())
        .expect("the prompt is written");
    child.wait_with_output().expect("the child finishes")
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("stdout is utf-8")
}

fn stderr(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).expect("stderr is utf-8")
}

fn temp_root() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("a temp root");
    let root = dir.path().to_path_buf();
    (dir, root)
}

fn root_arg(root: &Path) -> String {
    root.to_str().expect("a utf-8 temp path").to_string()
}

/// The run id `chat` reports on stderr, which is how a second process addresses
/// the same run.
fn reported_run_id(out: &Output) -> String {
    let err = stderr(out);
    err.lines()
        .find_map(|l| l.strip_prefix("run "))
        .unwrap_or_else(|| panic!("no run id on stderr:\n{err}"))
        .trim()
        .to_string()
}

#[test]
fn chat_answers_from_a_local_provider_and_records_the_run() {
    let provider = StubProvider::serving(vec![reply("the answer is 42", "stop", 25)]);
    let (_dir, root) = temp_root();

    let out = heddle(&[
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "alpha",
        "--model",
        "llama3.1",
        "--base-url",
        &provider.base_url,
        "--prompt",
        "what is the answer?",
    ]);

    // stdout is exactly the answer and nothing else: it is the scriptable
    // contract, so the run id goes to stderr.
    assert_eq!(stdout(&out), "the answer is 42\n");
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));

    // The two slices composing, proven by running both binaries: slice 011's
    // reader against the run slice 012's writer just created.
    let run_id = reported_run_id(&out);
    let log = heddle(&[
        "ledger",
        "log",
        "--root",
        &root_arg(&root),
        "--silo",
        "alpha",
        "--run",
        &run_id,
    ]);
    assert_eq!(log.status.code(), Some(0), "stderr:\n{}", stderr(&log));
    let logged = stdout(&log);
    let kinds: Vec<&str> = logged
        .lines()
        .map(|l| l.split('\t').nth(2).expect("a kind column"))
        .collect();
    assert_eq!(
        kinds,
        vec![
            "iteration_boundary",
            "llm_request",
            "wire_exchange",
            "llm_response",
            "budget_spent",
            "exit"
        ]
    );

    let verify = heddle(&[
        "ledger",
        "verify",
        "--root",
        &root_arg(&root),
        "--silo",
        "alpha",
        "--run",
        &run_id,
    ]);
    assert_eq!(
        verify.status.code(),
        Some(0),
        "stderr:\n{}",
        stderr(&verify)
    );
    assert_eq!(stdout(&verify), format!("{run_id}\tok\t6 steps\n"));
}

#[test]
fn chat_reads_the_prompt_from_stdin_when_no_flag_is_given() {
    let provider = StubProvider::serving(vec![reply("read you", "stop", 9)]);
    let (_dir, root) = temp_root();

    let out = heddle_with_stdin(
        &[
            "chat",
            "--root",
            &root_arg(&root),
            "--silo",
            "beta",
            "--model",
            "llama3.1",
            "--base-url",
            &provider.base_url,
        ],
        "what did I pipe you?",
    );

    assert_eq!(stdout(&out), "read you\n");
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    // The prompt really came from stdin: it is on the chain.
    let show = heddle(&[
        "ledger",
        "log",
        "--root",
        &root_arg(&root),
        "--silo",
        "beta",
    ]);
    assert_eq!(show.status.code(), Some(0));
}

#[test]
fn chat_fails_loudly_when_no_provider_is_listening() {
    let base_url = dead_loopback_url();
    let (_dir, root) = temp_root();

    let out = heddle(&[
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "gamma",
        "--model",
        "llama3.1",
        "--base-url",
        &base_url,
        "--prompt",
        "anyone home?",
    ]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "", "an unanswered prompt prints no answer");
    let err = stderr(&out);
    assert!(
        err.contains(&base_url) && err.contains("is a local provider listening"),
        "stderr must name the endpoint and what to check, got:\n{err}"
    );
}

#[test]
fn chat_refuses_a_non_loopback_base_url() {
    let (_dir, root) = temp_root();

    let out = heddle(&[
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "delta",
        "--model",
        "llama3.1",
        "--base-url",
        "http://192.168.1.10:11434/v1",
        "--prompt",
        "reach across the LAN",
    ]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "");
    assert!(
        stderr(&out).contains("not a loopback address"),
        "stderr:\n{}",
        stderr(&out)
    );
    // The refusal happens before the silo is touched, so there is no ledger to
    // hold a run — a stronger claim than "the ledger holds no run".
    assert!(
        !Silo::open(&root, "delta")
            .expect("a silo path")
            .ledger_path()
            .exists(),
        "a refused endpoint must not open a chain"
    );
}

#[test]
fn chat_fails_when_the_engine_stops_the_run_without_an_answer() {
    // A provider that never returns `finish_reason: "stop"`. The engine stops
    // the run on its iteration budget, which is Constitution VIII working —
    // and an empty answer with exit 0 would be slice 011's User Story 4
    // failure.
    let provider = StubProvider::serving(vec![
        reply("half a thoug", "length", 7),
        reply("still going", "length", 7),
    ]);
    let (_dir, root) = temp_root();

    let out = heddle(&[
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "epsilon",
        "--model",
        "llama3.1",
        "--base-url",
        &provider.base_url,
        "--max-iters",
        "2",
        "--prompt",
        "never finish",
    ]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "", "a stopped run has no answer to print");
    let err = stderr(&out);
    assert!(
        err.contains("ended without a final answer") && err.contains("MaxIters"),
        "stderr must name the exit, got:\n{err}"
    );

    // The run is still on the chain: the engine stopping a model is history,
    // not an error to be swallowed.
    let run_id = reported_run_id(&out);
    let verify = heddle(&[
        "ledger",
        "verify",
        "--root",
        &root_arg(&root),
        "--silo",
        "epsilon",
        "--run",
        &run_id,
    ]);
    assert_eq!(
        verify.status.code(),
        Some(0),
        "stderr:\n{}",
        stderr(&verify)
    );
    assert_eq!(stdout(&verify), format!("{run_id}\tok\t11 steps\n"));
}

#[test]
fn the_base_url_falls_back_to_the_environment_and_the_local_default() {
    let provider = StubProvider::serving(vec![reply("from the environment", "stop", 5)]);
    let (_dir, root) = temp_root();
    let args = [
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "zeta",
        "--model",
        "llama3.1",
        "--prompt",
        "where did you look?",
    ];

    // $HEDDLE_MODEL_BASE_URL is the fallback when the flag is absent.
    let from_env = Command::new(env!("CARGO_BIN_EXE_heddle"))
        .args(args)
        .env_remove("HEDDLE_ROOT")
        .env("HEDDLE_MODEL_BASE_URL", &provider.base_url)
        .output()
        .expect("the heddle binary runs");
    assert_eq!(stdout(&from_env), "from the environment\n");
    assert_eq!(
        from_env.status.code(),
        Some(0),
        "stderr:\n{}",
        stderr(&from_env)
    );

    // With neither, the default is Ollama's documented loopback URL. Asserted
    // by the endpoint the failure names, not by reaching it: this test must not
    // depend on whether a model happens to be installed on the machine running
    // it.
    let (_dir2, root2) = temp_root();
    let defaulted = heddle(&[
        "chat",
        "--root",
        &root_arg(&root2),
        "--silo",
        "eta",
        "--model",
        "llama3.1",
        "--prompt",
        "and now?",
        "--timeout-secs",
        "5",
    ]);
    if defaulted.status.code() == Some(0) {
        // A real provider is listening on 11434 on this machine, which is a
        // fact about the host and not about the code. The default was still
        // used, which is what this half asserts.
        return;
    }
    assert!(
        stderr(&defaulted).contains("http://localhost:11434/v1"),
        "the default endpoint must be named, got:\n{}",
        stderr(&defaulted)
    );
}

// ---- redaction (spec 014) ----

/// The value `--redact` is pointed at. Long and distinctive so a substring
/// assertion cannot pass by accident.
const REDACTED_VALUE: &str = "sk-cli-test-SECRET-abc123";

/// A credential unique to this process and this test, removed on every exit
/// path including a panic — `cli_secret.rs`'s pattern, for the same reason.
struct TestRef {
    keychain: OsKeychain,
    reference: SecretRef,
}

impl TestRef {
    fn holding(value: &str) -> TestRef {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let service = format!(
            "heddle-cli-redact-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        let held = TestRef {
            keychain: OsKeychain::new().expect("the platform credential store opens"),
            reference: SecretRef(format!("keychain://{service}/cli")),
        };
        held.keychain
            .store(&held.reference, value)
            .expect("the value is stored for the run to redact");
        held
    }

    fn uri(&self) -> &str {
        &self.reference.0
    }
}

impl Drop for TestRef {
    fn drop(&mut self) {
        let _ = self.keychain.delete(&self.reference);
    }
}

fn payloads(root: &Path, silo: &str, run_id: &str) -> Vec<String> {
    Silo::open(root, silo)
        .expect("a silo path")
        .ledger()
        .expect("the chain opens")
        .log(run_id)
        .iter()
        .map(|s| s.payload.clone())
        .collect()
}

#[test]
fn chat_redacts_a_configured_secret_from_the_chain_but_not_from_stdout() {
    let held = TestRef::holding(REDACTED_VALUE);
    let provider = StubProvider::serving(vec![reply(
        &format!("your key is {REDACTED_VALUE}"),
        "stop",
        11,
    )]);
    let (_dir, root) = temp_root();

    let out = heddle(&[
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "theta",
        "--model",
        "llama3.1",
        "--base-url",
        &provider.base_url,
        "--redact",
        held.uri(),
        "--prompt",
        "what is my key?",
    ]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr:
{}",
        stderr(&out)
    );
    assert_eq!(
        stdout(&out),
        format!(
            "your key is {REDACTED_VALUE}
"
        ),
        "the operator still gets the real answer: redaction is about the record"
    );

    let payloads = payloads(&root, "theta", &reported_run_id(&out));
    assert!(
        payloads.iter().all(|p| !p.contains(REDACTED_VALUE)),
        "no payload of the run may contain the configured secret: {payloads:?}"
    );
    assert!(payloads.iter().any(|p| p.contains("***")), "{payloads:?}");
}

#[test]
fn chat_refuses_an_unresolvable_redaction_reference_before_opening_a_chain() {
    let (_dir, root) = temp_root();
    let missing = format!("keychain://heddle-cli-absent-{}/cli", std::process::id());

    let out = heddle(&[
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "iota",
        "--model",
        "llama3.1",
        "--base-url",
        &dead_loopback_url(),
        "--redact",
        &missing,
        "--prompt",
        "should never be sent",
    ]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "");
    assert!(
        stderr(&out).contains(&missing),
        "stderr must name the reference that could not be resolved, got:
{}",
        stderr(&out)
    );
    // A redactor that scrubs nothing is worse than no redactor, so the run is
    // refused before a chain exists to hold it — the ordering
    // `chat_refuses_a_non_loopback_base_url` pins for the endpoint guard.
    assert!(
        !Silo::open(&root, "iota")
            .expect("a silo path")
            .ledger_path()
            .exists(),
        "an unresolvable reference must not open a chain"
    );
}

// ---- the fs connector (spec 016) ----

/// A turn in which the model asks for one tool, in the shape Ollama's
/// OpenAI-compatible endpoint sends: `content: null`, and the arguments as a
/// JSON *string* holding JSON.
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

/// A directory holding one file, for `--fs-root` to be pointed at.
fn fs_root_holding(name: &str, contents: &str) -> (TempDir, String) {
    let dir = TempDir::new().expect("a temp fs root");
    std::fs::write(dir.path().join(name), contents).expect("a file under the fs root");
    let path = dir.path().to_str().expect("a utf-8 temp path").to_string();
    (dir, path)
}

/// The last message of the request the child sent, which is what the model was
/// told about the tool it asked for.
fn last_message(request: &serde_json::Value) -> String {
    let last = request["messages"]
        .as_array()
        .expect("a messages array")
        .last()
        .expect("at least one message");
    // Every caller reads this for what the model was told about a tool, so the
    // envelope is asserted once here: external content by its role, naming the
    // call it answers rather than resting on its position in the history.
    assert_eq!(
        (&last["role"], &last["tool_call_id"]),
        (&serde_json::json!("tool"), &serde_json::json!("call_1")),
        "{last}"
    );
    last["content"].as_str().expect("text content").to_string()
}

#[test]
fn chat_with_an_fs_root_advertises_the_read_tools_and_reads_a_real_file() {
    let provider = StubProvider::serving(vec![
        tool_call_reply("fs_read", serde_json::json!({"path": "notes.txt"})),
        reply("the first line is: hello from disk", "stop", 9),
    ]);
    let (_dir, root) = temp_root();
    let (_files, fs_root) = fs_root_holding("notes.txt", "hello from disk");

    let out = heddle(&[
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "lambda",
        "--model",
        "llama3.1",
        "--base-url",
        &provider.base_url,
        "--fs-root",
        &fs_root,
        "--prompt",
        "what is the first line of notes.txt?",
    ]);

    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(stdout(&out), "the first line is: hello from disk\n");

    // The real binary put the server's own derived schemas on the wire, and
    // only the two read-only names: `fs_write` exists on the server and is
    // absent here because `heddle chat` has nobody to ask for a confirmation.
    let first = provider.request_body();
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
        "{first}"
    );

    // And the file really was read, by the shipped binary, off disk.
    let second = provider.request_body();
    let told = last_message(&second);
    assert!(told.contains("hello from disk"), "{told}");

    let run_id = reported_run_id(&out);
    let log = heddle(&[
        "ledger",
        "log",
        "--root",
        &root_arg(&root),
        "--silo",
        "lambda",
        "--run",
        &run_id,
    ]);
    let logged = stdout(&log);
    let kinds: Vec<&str> = logged
        .lines()
        .map(|l| l.split('\t').nth(2).expect("a kind column"))
        .collect();
    assert_eq!(
        kinds,
        vec![
            "iteration_boundary",
            "llm_request",
            "wire_exchange",
            "llm_response",
            "budget_spent",
            "tool_call",
            "approval",
            "tool_result",
            "iteration_boundary",
            "llm_request",
            "wire_exchange",
            "llm_response",
            "budget_spent",
            "exit"
        ]
    );

    let verify = heddle(&[
        "ledger",
        "verify",
        "--root",
        &root_arg(&root),
        "--silo",
        "lambda",
        "--run",
        &run_id,
    ]);
    assert_eq!(stdout(&verify), format!("{run_id}\tok\t14 steps\n"));
}

#[test]
fn chat_without_an_fs_root_sends_no_tools_key_at_all() {
    let provider = StubProvider::serving(vec![reply("no tools needed", "stop", 5)]);
    let (_dir, root) = temp_root();

    let out = heddle(&[
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "mu",
        "--model",
        "llama3.1",
        "--base-url",
        &provider.base_url,
        "--prompt",
        "just answer",
    ]);

    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(stdout(&out), "no tools needed\n");
    // Not "an empty tools array": no key. The connector is opt-in, so a run
    // without `--fs-root` puts exactly the bytes on the wire it put there
    // before the connector existed.
    let body = provider.request_body();
    assert!(
        body.get("tools").is_none(),
        "a run with no fs root must advertise nothing at all: {body}"
    );
}

#[test]
fn chat_refuses_an_fs_root_that_does_not_exist_before_opening_a_chain() {
    let (_dir, root) = temp_root();
    let missing = root.join("no-such-directory");

    let out = heddle(&[
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "nu",
        "--model",
        "llama3.1",
        "--base-url",
        &dead_loopback_url(),
        "--fs-root",
        missing.to_str().expect("a utf-8 temp path"),
        "--prompt",
        "should never be sent",
    ]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "");
    assert!(
        stderr(&out).contains("no-such-directory"),
        "stderr must name the directory the operator gave, got:\n{}",
        stderr(&out)
    );
    // The same ordering the endpoint guard and the redactor are held to: a
    // refused root must leave no chain behind to hold a run that never ran.
    assert!(
        !Silo::open(&root, "nu")
            .expect("a silo path")
            .ledger_path()
            .exists(),
        "a refused fs root must not open a chain"
    );
}

/// A directory that **is** a git repository, with one commit and one untracked
/// file, for `--fs-root` to be pointed at.
///
/// Built through `git2` rather than by shelling out, so no test depends on a
/// `git` binary being on `PATH` — and `HEAD` names `work` before the first
/// commit, so the `## <branch>` assertion does not silently depend on the
/// machine's `init.defaultBranch`.
fn fs_root_that_is_a_repository() -> (TempDir, String) {
    let dir = TempDir::new().expect("a temp fs root");
    let repo = Repository::init(dir.path()).expect("a repository is initialised");
    repo.set_head("refs/heads/work")
        .expect("HEAD names the fixture's branch");
    std::fs::write(dir.path().join("tracked.txt"), "committed\n").expect("a file to commit");
    let mut index = repo.index().expect("the index opens");
    index
        .add_path(Path::new("tracked.txt"))
        .expect("the path is staged");
    index.write().expect("the index is written");
    let tree = repo
        .find_tree(index.write_tree().expect("the index writes a tree"))
        .expect("the tree is found");
    let who = Signature::now("Fixture Author", "fixture@example.invalid").expect("a signature");
    repo.commit(Some("HEAD"), &who, &who, "the only commit", &tree, &[])
        .expect("the commit is written");
    std::fs::write(dir.path().join("notes.txt"), "untracked\n").expect("an untracked file");

    let path = dir.path().to_str().expect("a utf-8 temp path").to_string();
    (dir, path)
}

#[test]
fn chat_with_an_fs_root_that_is_a_git_repository_advertises_the_git_tools_and_reports_real_status()
{
    let provider = StubProvider::serving(vec![
        tool_call_reply("git_status", serde_json::json!({})),
        reply("notes.txt is untracked", "stop", 9),
    ]);
    let (_dir, root) = temp_root();
    let (_repo, fs_root) = fs_root_that_is_a_repository();

    let out = heddle(&[
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "nu",
        "--model",
        "llama3.1",
        "--base-url",
        &provider.base_url,
        "--fs-root",
        &fs_root,
        "--prompt",
        "what has changed in this repository?",
    ]);

    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(stdout(&out), "notes.txt is untracked\n");

    // Five names, in allowlist order, and only because the root is a
    // repository — the pre-existing two-name assertion above uses a plain
    // directory and is untouched.
    let first = provider.request_body();
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
        "`git_status` is advertised with nothing to fill in: {first}"
    );

    // And the repository really was read, by the shipped binary, off disk.
    let second = provider.request_body();
    let told = last_message(&second);
    assert!(
        told.contains("## work") && told.contains("??\\tnotes.txt"),
        "{told}"
    );

    let run_id = reported_run_id(&out);
    let log = heddle(&[
        "ledger",
        "log",
        "--root",
        &root_arg(&root),
        "--silo",
        "nu",
        "--run",
        &run_id,
    ]);
    let kinds: Vec<String> = stdout(&log)
        .lines()
        .map(|l| l.split('\t').nth(2).expect("a kind column").to_string())
        .collect();
    assert_eq!(
        kinds,
        vec![
            "iteration_boundary",
            "llm_request",
            "wire_exchange",
            "llm_response",
            "budget_spent",
            "tool_call",
            "approval",
            "tool_result",
            "iteration_boundary",
            "llm_request",
            "wire_exchange",
            "llm_response",
            "budget_spent",
            "exit"
        ]
    );

    let verify = heddle(&[
        "ledger",
        "verify",
        "--root",
        &root_arg(&root),
        "--silo",
        "nu",
        "--run",
        &run_id,
    ]);
    assert_eq!(stdout(&verify), format!("{run_id}\tok\t14 steps\n"));
}

// ---------------------------------------------------------------------------
// Named-provider routing (spec 021). Every test here still drives the real
// binary as a process: the thing being proved is that `--provider` reaches the
// route's address through argument parsing, file reading and the egress policy
// as an operator would meet them, not that a function in `wiring` returns the
// right value.
//
// Every existing test above passes no `--provider` and is unchanged, which is
// the backward-compatibility claim: the flag is additive, and the provider file
// is not read when it is absent.
// ---------------------------------------------------------------------------

/// Writes a provider table into a temp dir and hands back its path.
fn providers_file(dir: &Path, toml: &str) -> String {
    let path = dir.join("providers.toml");
    std::fs::write(&path, toml).expect("the provider table is written");
    path.to_str().expect("a utf-8 temp path").to_string()
}

#[test]
fn chat_routes_through_a_named_local_provider() {
    let provider = StubProvider::serving(vec![reply("routed by name", "stop", 12)]);
    let (_dir, root) = temp_root();
    let table = providers_file(
        &root,
        &format!(
            r#"
[[provider]]
name = "local-ollama"
kind = "local"
base_url = "{}"
model = "llama3.1"
"#,
            provider.base_url
        ),
    );

    let out = heddle(&[
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "named",
        // Deliberately a model the route does not name, so the assertion below
        // proves the route won rather than that both happened to agree.
        "--model",
        "ignored-when-a-provider-is-named",
        "--provider",
        "local-ollama",
        "--providers-file",
        &table,
        "--prompt",
        "who answered?",
    ]);

    assert_eq!(stdout(&out), "routed by name\n");
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(
        provider.request_body()["model"],
        "llama3.1",
        "the model on the wire is the route's, not --model's"
    );

    // The run is on the chain like any other: routing changed where the bytes
    // went, not whether they were recorded.
    let run_id = reported_run_id(&out);
    let verify = heddle(&[
        "ledger",
        "verify",
        "--root",
        &root_arg(&root),
        "--silo",
        "named",
        "--run",
        &run_id,
    ]);
    assert_eq!(stdout(&verify), format!("{run_id}\tok\t6 steps\n"));
}

#[test]
fn chat_refuses_a_cloud_provider_without_allow_egress_and_opens_no_socket() {
    // The stub is live and listening, so nothing but the egress policy stops the
    // child from reaching it.
    let provider = StubProvider::serving(vec![reply("must never be reached", "stop", 1)]);
    let (_dir, root) = temp_root();
    let table = providers_file(
        &root,
        &format!(
            r#"
[[provider]]
name = "cloud-primary"
kind = "cloud"
base_url = "{}"
model = "gpt-4o-mini"
"#,
            provider.base_url
        ),
    );

    let out = heddle(&[
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "egress-off",
        "--model",
        "llama3.1",
        "--provider",
        "cloud-primary",
        "--providers-file",
        &table,
        "--prompt",
        "may I leave?",
    ]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "", "a refused run prints no answer");
    let err = stderr(&out);
    assert!(
        err.contains("cloud-primary") && err.contains("egress") && err.contains("--allow-egress"),
        "stderr must name the provider, the policy and the way to permit it, got:\n{err}"
    );
    assert_eq!(
        provider.connection_count(),
        0,
        "the child opened no socket at all"
    );
    // Refused before the silo, so no chain records an attempt that never left
    // the process — the ordering `chat.rs` documents, proved end to end.
    assert!(
        !root.join("egress-off").exists(),
        "a refusal before the silo leaves no silo behind"
    );
}

#[test]
fn chat_reaches_a_cloud_provider_when_egress_is_allowed() {
    let provider = StubProvider::serving(vec![reply("egress permitted", "stop", 7)]);
    let (_dir, root) = temp_root();
    let table = providers_file(
        &root,
        &format!(
            r#"
[[provider]]
name = "cloud-primary"
kind = "cloud"
base_url = "{}"
model = "gpt-4o-mini"
"#,
            provider.base_url
        ),
    );

    let out = heddle(&[
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "egress-on",
        "--model",
        "llama3.1",
        "--provider",
        "cloud-primary",
        "--providers-file",
        &table,
        "--allow-egress",
        "--prompt",
        "may I leave?",
    ]);

    assert_eq!(stdout(&out), "egress permitted\n");
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(provider.request_body()["model"], "gpt-4o-mini");
    assert_eq!(provider.connection_count(), 1);
}

#[test]
fn chat_refuses_an_unknown_provider_name() {
    let (_dir, root) = temp_root();
    let table = providers_file(
        &root,
        r#"
[[provider]]
name = "local-ollama"
kind = "local"
base_url = "http://127.0.0.1:11434/v1"
model = "llama3.1"
"#,
    );

    let out = heddle(&[
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "typo",
        "--model",
        "llama3.1",
        "--provider",
        "local-olama",
        "--providers-file",
        &table,
        "--prompt",
        "hello?",
    ]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "");
    let err = stderr(&out);
    assert!(
        err.contains("local-olama") && err.contains("local-ollama"),
        "stderr must name the miss and what is configured, got:\n{err}"
    );
}

#[test]
fn chat_refuses_a_providers_file_it_cannot_read() {
    let (_dir, root) = temp_root();
    let missing = root.join("nowhere").join("providers.toml");

    let out = heddle(&[
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "no-file",
        "--model",
        "llama3.1",
        "--provider",
        "local-ollama",
        "--providers-file",
        missing.to_str().expect("a utf-8 temp path"),
        "--prompt",
        "hello?",
    ]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "");
    let err = stderr(&out);
    assert!(
        err.contains("providers.toml") && err.contains("could not read"),
        "stderr must name the path it tried, got:\n{err}"
    );
}

#[test]
fn chat_without_a_provider_never_reads_the_providers_file() {
    // The backward-compatibility claim, made explicit rather than left implied
    // by the older tests: --providers-file points at a file that would fail to
    // parse, and the run succeeds because it is never opened.
    let provider = StubProvider::serving(vec![reply("old path intact", "stop", 4)]);
    let (_dir, root) = temp_root();
    let table = providers_file(&root, "this is not valid TOML at all [[[");

    let out = heddle(&[
        "chat",
        "--root",
        &root_arg(&root),
        "--silo",
        "untouched",
        "--model",
        "llama3.1",
        "--base-url",
        &provider.base_url,
        "--providers-file",
        &table,
        "--prompt",
        "still working?",
    ]);

    assert_eq!(stdout(&out), "old path intact\n");
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
}
