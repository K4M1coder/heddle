//! Acceptance tests for `heddle acp-agent` (spec 013).
//!
//! The headline test is the one thing no prior slice proved: a **real ACP
//! client from the `agent-client-protocol` crate** spawning the **real `heddle`
//! binary as a subprocess** and speaking to it over the child's actual stdio —
//! the same transport an editor uses. Slice 008's suite drives the same facade
//! in-process over a `tokio::io::duplex`, which cannot fail on argument
//! parsing, on an exit code, or on a stray byte written to stdout.
//!
//! The model behind it is a `std::net::TcpListener` serving real HTTP/1.1
//! chat-completions bytes from this test process, so no test here needs a
//! running Ollama and none needs an installed editor.

#[cfg(windows)]
mod guard;

use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock, InitializeRequest, NewSessionRequest, PermissionOptionKind,
    PromptRequest, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionId, SessionNotification, SessionUpdate, StopReason,
    TextContent, ToolCallId, ToolCallStatus,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{AcpAgent, AcpAgentConfig, Agent, Client, ConnectionTo};
use git2::{Repository, Signature};
use heddle_silo::Silo;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers. Copied from `cli_chat.rs` rather than shared — the same reasoning
// `heddle-acp/tests/acp_session.rs` records for its own doubles, and `heddle-cli`
// has no `lib` target to share them through, so those five tests stay this
// slice's controls.
// ---------------------------------------------------------------------------

/// Long enough that a slow runner never trips it, short enough that a child
/// which silently sends nothing fails as a failure rather than as a hang.
const OBSERVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// A local provider answering a fixed script, one connection per turn, and
/// reporting the request bodies the child sent it.
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
        }
    }

    /// Writes part of one SSE response, then holds the socket open until `gate`
    /// fires before writing the rest.
    ///
    /// `content-length` covers **both** parts, so the client is committed to
    /// reading more and cannot mistake the pause for the end of the body, and
    /// `set_nodelay` keeps the first part from sitting in a Nagle buffer. This
    /// is the only way a test can put a chunk on the client's transcript at a
    /// moment when the model's turn provably has not returned.
    fn stalling(first: String, rest: String, gate: Receiver<()>) -> StubProvider {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let addr = listener.local_addr().expect("the bound address");
        let (tx, requests) = mpsc::channel();
        std::thread::spawn(move || {
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            let _ = socket.set_nodelay(true);
            let Some(seen) = read_request(&mut socket) else {
                return;
            };
            if tx.send(seen).is_err() {
                return;
            }
            let _ = socket.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{first}",
                    first.len() + rest.len()
                )
                .as_bytes(),
            );
            let _ = socket.flush();
            let _ = gate.recv();
            let _ = socket.write_all(rest.as_bytes());
            let _ = socket.flush();
        });
        StubProvider {
            base_url: format!("http://{addr}/v1"),
            requests,
        }
    }

    /// The next request's body, parsed: what the real binary put on the wire.
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

/// One SSE event, framed as the real provider frames it: a `data:` line closed
/// by a blank one. The separator is a bare `\n\n`, not CRLF — measured, not
/// assumed.
fn event(value: serde_json::Value) -> String {
    format!("data: {value}\n\n")
}

/// A whole stream: the events, then `[DONE]`. Spelled out here rather than
/// shared across test binaries for the reason this file's header already
/// records: they are one another's controls.
fn sse(events: Vec<serde_json::Value>) -> String {
    let mut raw: String = events.into_iter().map(event).collect();
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

fn heddle(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_heddle"))
        .args(args)
        .env_remove("HEDDLE_ROOT")
        .env_remove("HEDDLE_MODEL_BASE_URL")
        .output()
        .expect("the heddle binary runs")
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

/// `block_on` has no timeout and `cargo test` has no per-test limit, so without
/// this a regression is a hung CI job on three operating systems instead of a
/// failure. An orphaned child is reaped when the test binary exits.
fn run_with_timeout<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_secs(60))
        .expect("the ACP client finished within 60s")
}

/// The `AgentMessageChunk` texts the client was notified of, in order.
fn chunks(updates: &Mutex<Vec<SessionUpdate>>) -> Vec<String> {
    updates
        .lock()
        .expect("the update log")
        .iter()
        .filter_map(|update| match update {
            SessionUpdate::AgentMessageChunk(chunk) => match &chunk.content {
                ContentBlock::Text(text) => Some(text.text.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The step kinds `heddle ledger log` reports for one run, in chain order.
fn logged_kinds(root: &Path, silo: &str, run_id: &str) -> Vec<String> {
    let log = heddle(&[
        "ledger",
        "log",
        "--root",
        &root_arg(root),
        "--silo",
        silo,
        "--run",
        run_id,
    ]);
    assert_eq!(log.status.code(), Some(0), "stderr:\n{}", stderr(&log));
    stdout(&log)
        .lines()
        .map(|l| l.split('\t').nth(2).expect("a kind column").to_string())
        .collect()
}

/// The claim slice 025 exists to make, stated so that only the claim can
/// satisfy it: **a chunk reaches the editor while `session/prompt` is still
/// outstanding.**
///
/// "More than one chunk arrived" would not prove it — the projection could
/// still be sending every one of them after the run, which is exactly the
/// behaviour being replaced. So the provider writes two events and then stops
/// writing, holding the socket open. Nothing releases it except the arrival of
/// a chunk at the client. Therefore:
///
/// - if the client sees a chunk, the provider's response is provably unfinished,
///   so `turn` has not returned, so the run has not ended, so `session/prompt`
///   has not been answered — the chunk can only have come from the live path;
/// - if the client sees no chunk, nothing releases the provider, the prompt
///   never completes, and `run_with_timeout` fails the test rather than passing
///   it quietly.
///
/// `answered` records the second half directly rather than leaving it to that
/// argument: the notification handler snapshots it, and the first chunk must
/// have found it `false`.
#[test]
fn a_chunk_reaches_the_client_while_the_prompt_is_still_outstanding() {
    let delta = |text: &str| {
        event(serde_json::json!({
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": text}}]
        }))
    };
    let held = [delta("The "), delta("answer ")].concat();
    let rest = [
        delta("is 42."),
        event(serde_json::json!({
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
        })),
        event(serde_json::json!({"choices": [], "usage": {"total_tokens": 25}})),
        "data: [DONE]

"
        .to_string(),
    ]
    .concat();

    let (release, gate) = mpsc::channel();
    let provider = StubProvider::stalling(held, rest, gate);
    let (_dir, root) = temp_root();
    let root_flag = root_arg(&root);

    let updates: Arc<Mutex<Vec<SessionUpdate>>> = Arc::default();
    let collected = updates.clone();
    // Set the instant the prompt is answered; read by the notification handler
    // for every chunk it receives.
    let answered = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let answered_when_notified = answered.clone();
    let seen_answered: Arc<Mutex<Vec<bool>>> = Arc::default();
    let recording = seen_answered.clone();

    let transport = AcpAgent::new(AcpAgentConfig::new(env!("CARGO_BIN_EXE_heddle")).args([
        "acp-agent",
        "--root",
        &root_flag,
        "--silo",
        "alpha",
        "--model",
        "llama3.1",
        "--base-url",
        &provider.base_url,
        "--timeout-secs",
        "30",
    ]));

    let stop = run_with_timeout(move || {
        futures::executor::block_on(
            Client
                .builder()
                .name("test-client")
                .on_receive_notification(
                    async move |notification: SessionNotification, _cx| {
                        if matches!(notification.update, SessionUpdate::AgentMessageChunk(_)) {
                            recording.lock().expect("the flag log").push(
                                answered_when_notified.load(std::sync::atomic::Ordering::SeqCst),
                            );
                            // Only now does the provider get to finish, so the
                            // turn cannot have ended before this line ran.
                            let _ = release.send(());
                        }
                        collected
                            .lock()
                            .expect("the update log")
                            .push(notification.update);
                        Ok(())
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .connect_with(transport, async move |cx: ConnectionTo<Agent>| {
                    cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                        .block_task()
                        .await?;
                    let session = cx
                        .send_request(NewSessionRequest::new(PathBuf::from(".")))
                        .block_task()
                        .await?;
                    let response = cx
                        .send_request(PromptRequest::new(
                            session.session_id,
                            vec![ContentBlock::Text(TextContent::new("what is the answer?"))],
                        ))
                        .block_task()
                        .await?;
                    answered.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok(response.stop_reason)
                }),
        )
        .expect("the ACP client ran to completion")
    });

    assert_eq!(stop, StopReason::EndTurn);
    let flags = seen_answered.lock().expect("the flag log").clone();
    assert_eq!(
        flags.first(),
        Some(&false),
        "the first chunk must arrive before session/prompt is answered, got {flags:?}"
    );
    // One per delta and nothing after them: a fourth entry holding the whole
    // answer would be the chain-derived projection repeating what the client
    // already has.
    assert_eq!(chunks(&updates), vec!["The ", "answer ", "is 42."]);

    // And the run is an ordinary governed run on the chain, unchanged by the
    // fact that its text left the process early.
    assert_eq!(
        logged_kinds(&root, "alpha", "heddle-1#1"),
        vec![
            "iteration_boundary",
            "llm_request",
            "wire_exchange",
            "llm_response",
            "budget_spent",
            "exit",
        ]
    );
}

// ---------------------------------------------------------------------------
// The acceptance test: the slice's reason to exist.
// ---------------------------------------------------------------------------

#[test]
fn an_acp_client_drives_the_real_binary_and_the_session_lands_on_the_chain() {
    let provider = StubProvider::serving(vec![
        reply("the answer is 42", "stop", 25),
        reply("and 43", "stop", 11),
    ]);
    let (_dir, root) = temp_root();

    let updates: Arc<Mutex<Vec<SessionUpdate>>> = Arc::default();
    let collected = updates.clone();
    let root_flag = root_arg(&root);

    // The base URL and the root are passed as flags rather than left to the
    // environment because `AcpAgentConfig::envs` only *adds* to the inherited
    // environment: unlike `cli_chat.rs`'s `env_remove`, there is no way to
    // unset a stray `$HEDDLE_MODEL_BASE_URL` for the child.
    let transport = AcpAgent::new(AcpAgentConfig::new(env!("CARGO_BIN_EXE_heddle")).args([
        "acp-agent",
        "--root",
        &root_flag,
        "--silo",
        "alpha",
        "--model",
        "llama3.1",
        "--base-url",
        &provider.base_url,
        "--timeout-secs",
        "10",
    ]));

    let (session_id, stops) = run_with_timeout(move || {
        futures::executor::block_on(
            Client
                .builder()
                .name("test-client")
                .on_receive_notification(
                    async move |notification: SessionNotification, _cx| {
                        collected
                            .lock()
                            .expect("the update log")
                            .push(notification.update);
                        Ok(())
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .connect_with(transport, async move |cx: ConnectionTo<Agent>| {
                    cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                        .block_task()
                        .await?;
                    let session = cx
                        .send_request(NewSessionRequest::new(PathBuf::from(".")))
                        .block_task()
                        .await?;
                    let session_id = session.session_id;

                    let mut stops = Vec::new();
                    for text in ["what is the answer?", "and the next one?"] {
                        let response = cx
                            .send_request(PromptRequest::new(
                                session_id.clone(),
                                vec![ContentBlock::Text(TextContent::new(text))],
                            ))
                            .block_task()
                            .await?;
                        stops.push(response.stop_reason);
                    }
                    Ok((session_id, stops))
                }),
        )
        .expect("the ACP client ran to completion")
    });

    // Deterministic: the facade mints session ids from an AtomicU64 starting at
    // 1, and this is a fresh process.
    assert_eq!(session_id, SessionId::new("heddle-1"));
    assert_eq!(stops, vec![StopReason::EndTurn, StopReason::EndTurn]);

    // The answers came from the real OpenAiCompatClient against the stub, and
    // reached the client through `project_updates` reading the chain.
    let chunks = chunks(&updates);
    assert!(
        chunks.iter().any(|c| c == "the answer is 42") && chunks.iter().any(|c| c == "and 43"),
        "both answers must reach the client, got: {chunks:?}"
    );

    // A second process, after the connection closed: persistence across a
    // process boundary, not an in-memory Ledger read back by its writer.
    let expected = vec![
        "iteration_boundary",
        "llm_request",
        "wire_exchange",
        "llm_response",
        "budget_spent",
        "exit",
    ];
    assert_eq!(logged_kinds(&root, "alpha", "heddle-1#1"), expected);
    assert_eq!(logged_kinds(&root, "alpha", "heddle-1#2"), expected);

    let verify = heddle(&["ledger", "verify", "--root", &root_flag, "--silo", "alpha"]);
    assert_eq!(
        verify.status.code(),
        Some(0),
        "stderr:\n{}",
        stderr(&verify)
    );
    assert_eq!(
        stdout(&verify),
        "heddle-1#1\tok\t6 steps\nheddle-1#2\tok\t6 steps\n"
    );
}

#[test]
fn acp_agent_refuses_a_non_loopback_base_url_before_serving() {
    let (_dir, root) = temp_root();

    // No ACP client: nothing should get as far as a handshake.
    let out = heddle(&[
        "acp-agent",
        "--root",
        &root_arg(&root),
        "--silo",
        "delta",
        "--model",
        "llama3.1",
        "--base-url",
        "http://192.168.1.10:11434/v1",
    ]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "");
    assert!(
        stderr(&out).contains("not a loopback address"),
        "stderr:\n{}",
        stderr(&out)
    );
    // The refusal happens before the silo is touched, the same ordering
    // `chat_refuses_a_non_loopback_base_url` pins for the other command.
    assert!(
        !Silo::open(&root, "delta")
            .expect("a silo path")
            .ledger_path()
            .exists(),
        "a refused endpoint must not open a chain"
    );
}

#[test]
fn acp_agent_refuses_an_unresolvable_redaction_reference_before_serving() {
    let (_dir, root) = temp_root();
    let missing = format!("keychain://heddle-acp-absent-{}/cli", std::process::id());

    // No ACP client: a redaction reference that resolves to nothing must stop
    // the command before the handshake, exactly as a non-loopback base URL does.
    let out = heddle(&[
        "acp-agent",
        "--root",
        &root_arg(&root),
        "--silo",
        "kappa",
        "--model",
        "llama3.1",
        "--redact",
        &missing,
    ]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stdout(&out),
        "",
        "stdout is the protocol; nothing may go there"
    );
    assert!(
        stderr(&out).contains(&missing),
        "stderr:
{}",
        stderr(&out)
    );
    assert!(
        !Silo::open(&root, "kappa")
            .expect("a silo path")
            .ledger_path()
            .exists(),
        "an unresolvable reference must not open a chain"
    );
}

#[test]
fn acp_agent_exits_zero_when_its_client_disconnects() {
    let (_dir, root) = temp_root();

    // Closing an editor is not an error. Immediately-closed stdin is what the
    // agent sees when its client goes away.
    let child = Command::new(env!("CARGO_BIN_EXE_heddle"))
        .args([
            "acp-agent",
            "--root",
            &root_arg(&root),
            "--silo",
            "omega",
            "--model",
            "llama3.1",
            "--base-url",
            "http://127.0.0.1:11434/v1",
        ])
        .env_remove("HEDDLE_ROOT")
        .env_remove("HEDDLE_MODEL_BASE_URL")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the heddle binary runs");
    let out = child.wait_with_output().expect("the child finishes");

    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    // stdout is the protocol: one stray byte would corrupt the JSON-RPC stream,
    // and with no client there is no frame to write either.
    assert_eq!(stdout(&out), "");
}

// ---- the fs connector (spec 016) ----

#[test]
fn acp_agent_accepts_an_fs_root_and_still_serves_a_session() {
    let provider = StubProvider::serving(vec![reply("read and answered", "stop", 14)]);
    let (_dir, root) = temp_root();
    let files = TempDir::new().expect("a temp fs root");
    std::fs::write(files.path().join("notes.txt"), "hello from disk")
        .expect("a file under the fs root");

    let updates: Arc<Mutex<Vec<SessionUpdate>>> = Arc::default();
    let collected = updates.clone();
    let root_flag = root_arg(&root);
    let fs_root_flag = root_arg(files.path());

    // The risk this test exists for is not the flag. It is that a session
    // builds its connector inside `HeddleAgent::open`, which runs under
    // `futures::executor::block_on` -- and `Runtime::block_on` panics inside an
    // entered *tokio* runtime. Only a real handshake against the real binary
    // shows that the distinction holds.
    let transport = AcpAgent::new(AcpAgentConfig::new(env!("CARGO_BIN_EXE_heddle")).args([
        "acp-agent",
        "--root",
        &root_flag,
        "--silo",
        "sigma",
        "--model",
        "llama3.1",
        "--base-url",
        &provider.base_url,
        "--fs-root",
        &fs_root_flag,
        "--timeout-secs",
        "10",
    ]));

    let stop = run_with_timeout(move || {
        futures::executor::block_on(
            Client
                .builder()
                .name("test-client")
                .on_receive_notification(
                    async move |notification: SessionNotification, _cx| {
                        collected
                            .lock()
                            .expect("the update log")
                            .push(notification.update);
                        Ok(())
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .connect_with(transport, async move |cx: ConnectionTo<Agent>| {
                    cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                        .block_task()
                        .await?;
                    let session = cx
                        .send_request(NewSessionRequest::new(PathBuf::from(".")))
                        .block_task()
                        .await?;
                    let response = cx
                        .send_request(PromptRequest::new(
                            session.session_id,
                            vec![ContentBlock::Text(TextContent::new(
                                "what is in notes.txt?",
                            ))],
                        ))
                        .block_task()
                        .await?;
                    Ok(response.stop_reason)
                }),
        )
        .expect("the ACP client ran to completion")
    });

    assert_eq!(stop, StopReason::EndTurn);
    assert!(
        chunks(&updates).iter().any(|c| c == "read and answered"),
        "the answer must reach the client, got: {:?}",
        chunks(&updates)
    );

    // The session really had tools, and it had the *agent's* list: `fs_write`
    // is allowlisted and approved here, unlike in `heddle chat`, because there
    // is a human behind the editor for `AcpPermissionTransport` to ask.
    let body = provider.request_body();
    let advertised: Vec<&str> = body["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("the request must carry a tools array: {body}"))
        .iter()
        .map(|t| t["function"]["name"].as_str().expect("a tool name"))
        .collect();
    assert_eq!(advertised, vec!["fs_read", "fs_list", "fs_write"]);

    assert_eq!(
        logged_kinds(&root, "sigma", "heddle-1#1"),
        vec![
            "iteration_boundary",
            "llm_request",
            "wire_exchange",
            "llm_response",
            "budget_spent",
            "exit"
        ]
    );
}

#[test]
fn acp_agent_refuses_an_fs_root_that_does_not_exist_before_serving() {
    let (_dir, root) = temp_root();
    let missing = root.join("no-such-directory");

    // No ACP client: a root that does not exist must stop the command before
    // the handshake, exactly as a non-loopback base URL does.
    let out = heddle(&[
        "acp-agent",
        "--root",
        &root_arg(&root),
        "--silo",
        "tau",
        "--model",
        "llama3.1",
        "--fs-root",
        missing.to_str().expect("a utf-8 temp path"),
    ]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stdout(&out),
        "",
        "stdout is the protocol; nothing may go there"
    );
    assert!(
        stderr(&out).contains("no-such-directory"),
        "stderr:\n{}",
        stderr(&out)
    );
    assert!(
        !Silo::open(&root, "tau")
            .expect("a silo path")
            .ledger_path()
            .exists(),
        "a refused fs root must not open a chain"
    );
}

#[test]
fn acp_agent_documents_the_fs_root_flag() {
    let help = heddle(&["acp-agent", "--help"]);

    assert_eq!(help.status.code(), Some(0), "stderr:\n{}", stderr(&help));
    assert!(
        stdout(&help).contains("--fs-root"),
        "an opt-in capability an operator cannot discover is not opt-in, got:\n{}",
        stdout(&help)
    );
}

/// The second opt-in (spec 019 SC-011), on `acp-agent` and **nowhere else**.
///
/// `heddle chat` deliberately does not carry it: `proc_run` is `Mutating`, that
/// command is non-interactive, and `wiring::ToolArgs::chat_policy` already
/// records why a mutating tool that could only ever be denied should be
/// *absent* rather than listed.
#[test]
fn acp_agent_documents_the_allow_run_flag_and_chat_does_not() {
    let agent = heddle(&["acp-agent", "--help"]);
    let chat = heddle(&["chat", "--help"]);

    assert_eq!(
        agent.status.code(),
        Some(0),
        "stderr:
{}",
        stderr(&agent)
    );
    assert!(
        stdout(&agent).contains("--allow-run"),
        "an opt-in capability an operator cannot discover is not opt-in, got:
{}",
        stdout(&agent)
    );
    assert!(
        stdout(&agent).contains("Windows"),
        "and the platform limit belongs where the operator meets the flag, got:
{}",
        stdout(&agent)
    );

    assert_eq!(
        chat.status.code(),
        Some(0),
        "stderr:
{}",
        stderr(&chat)
    );
    assert!(
        !stdout(&chat).contains("--allow-run"),
        "`heddle chat` has nobody to ask for permission and must not offer it, got:
{}",
        stdout(&chat)
    );
}

// ---- the git connector (spec 017) ----

#[test]
fn acp_agent_over_a_git_repository_advertises_the_git_tools_too() {
    let provider = StubProvider::serving(vec![reply("nothing has changed", "stop", 11)]);
    let (_dir, root) = temp_root();
    let files = TempDir::new().expect("a temp fs root");
    let repo = Repository::init(files.path()).expect("a repository is initialised");
    std::fs::write(files.path().join("tracked.txt"), "committed\n").expect("a file to commit");
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

    let root_flag = root_arg(&root);
    let fs_root_flag = root_arg(files.path());

    // `heddle-cli` has no `lib` target, so `agent_policy` is observable through
    // nothing but the binary: this is the only way to prove the ACP command's
    // allowlist gained the git names alongside `heddle chat`'s.
    let transport = AcpAgent::new(AcpAgentConfig::new(env!("CARGO_BIN_EXE_heddle")).args([
        "acp-agent",
        "--root",
        &root_flag,
        "--silo",
        "psi",
        "--model",
        "llama3.1",
        "--base-url",
        &provider.base_url,
        "--fs-root",
        &fs_root_flag,
        "--timeout-secs",
        "10",
    ]));

    let stop = run_with_timeout(move || {
        futures::executor::block_on(Client.builder().name("test-client").connect_with(
            transport,
            async move |cx: ConnectionTo<Agent>| {
                cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                let session = cx
                    .send_request(NewSessionRequest::new(PathBuf::from(".")))
                    .block_task()
                    .await?;
                let response = cx
                    .send_request(PromptRequest::new(
                        session.session_id,
                        vec![ContentBlock::Text(TextContent::new(
                            "what has changed in this repository?",
                        ))],
                    ))
                    .block_task()
                    .await?;
                Ok(response.stop_reason)
            },
        ))
        .expect("the ACP client ran to completion")
    });

    assert_eq!(stop, StopReason::EndTurn);

    // `fs_write` keeps its place and the git names are appended: both tools are
    // `ReadOnly`, so there is no `fs_write`-style approval asymmetry to make —
    // this slice built nothing to confirm.
    let body = provider.request_body();
    let advertised: Vec<&str> = body["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("the request must carry a tools array: {body}"))
        .iter()
        .map(|t| t["function"]["name"].as_str().expect("a tool name"))
        .collect();
    assert_eq!(
        advertised,
        vec!["fs_read", "fs_list", "fs_write", "git_status", "git_log"]
    );
}

// ---- the ACP permission gate (spec 018) ----

/// A model turn that asks for one tool call. Copied from `cli_chat.rs` for the
/// reason this file's header already records.
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

/// The last message of the request the child sent, which is what the model was
/// told about the tool it asked for. Copied from `cli_chat.rs`.
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

/// What one answered session leaves behind: how the turn ended, every
/// permission request the **client** actually saw, and the updates it was
/// notified of.
struct Answered {
    stop: StopReason,
    asked: Vec<RequestPermissionRequest>,
    updates: Arc<Mutex<Vec<SessionUpdate>>>,
}

/// One prompt against the real binary, from a client that answers every
/// permission request by selecting the offered option whose kind is `answer`.
///
/// Selecting an **offered** option rather than a hand-built id is what a real
/// editor does, and it is the only way the two id constants
/// `AcpPermissionTransport::call` matches on stay honestly under test.
fn run_answering(
    root: &Path,
    silo: &str,
    fs_root: &Path,
    base_url: &str,
    answer: PermissionOptionKind,
) -> Answered {
    run_answering_with_args(root, silo, fs_root, base_url, answer, &[])
}

/// [`run_answering`] plus extra flags for the child, so slice 019 can pass
/// `--allow-run` without editing either of slice 018's two call sites — which
/// stay this slice's controls for the un-flagged behaviour.
fn run_answering_with_args(
    root: &Path,
    silo: &str,
    fs_root: &Path,
    base_url: &str,
    answer: PermissionOptionKind,
    extra: &[&str],
) -> Answered {
    let updates: Arc<Mutex<Vec<SessionUpdate>>> = Arc::default();
    let asked: Arc<Mutex<Vec<RequestPermissionRequest>>> = Arc::default();
    let collected = updates.clone();
    let recorded = asked.clone();

    let transport = AcpAgent::new(
        AcpAgentConfig::new(env!("CARGO_BIN_EXE_heddle"))
            .args([
                "acp-agent",
                "--root",
                &root_arg(root),
                "--silo",
                silo,
                "--model",
                "llama3.1",
                "--base-url",
                base_url,
                "--fs-root",
                &root_arg(fs_root),
                "--timeout-secs",
                "10",
            ])
            .args(extra.iter().copied()),
    );

    let stop = run_with_timeout(move || {
        futures::executor::block_on(
            Client
                .builder()
                .name("test-client")
                .on_receive_notification(
                    async move |notification: SessionNotification, _cx| {
                        collected
                            .lock()
                            .expect("the update log")
                            .push(notification.update);
                        Ok(())
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                // Records and responds, and does nothing else. It runs *inside*
                // the dispatch loop, which must stay free to deliver the answer
                // the child's loop thread is blocked on — so it never calls
                // `block_task()`. Without this handler the request goes
                // unanswered and the child hangs forever on an untimed `recv()`.
                .on_receive_request(
                    async move |request: RequestPermissionRequest, responder, _cx| {
                        recorded
                            .lock()
                            .expect("the permission log")
                            .push(request.clone());
                        let option = request
                            .options
                            .iter()
                            .find(|o| o.kind == answer)
                            .expect("the answered option kind is offered");
                        responder.respond(RequestPermissionResponse::new(
                            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                                option.option_id.clone(),
                            )),
                        ))
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .connect_with(transport, async move |cx: ConnectionTo<Agent>| {
                    cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                        .block_task()
                        .await?;
                    let session = cx
                        .send_request(NewSessionRequest::new(PathBuf::from(".")))
                        .block_task()
                        .await?;
                    let response = cx
                        .send_request(PromptRequest::new(
                            session.session_id,
                            vec![ContentBlock::Text(TextContent::new("plant a file for me"))],
                        ))
                        .block_task()
                        .await?;
                    Ok(response.stop_reason)
                }),
        )
        .expect("the ACP client ran to completion")
    });

    let asked = asked.lock().expect("the permission log").clone();
    Answered {
        stop,
        asked,
        updates,
    }
}

#[test]
fn an_acp_client_that_allows_lets_a_real_fs_write_execute() {
    let provider = StubProvider::serving(vec![
        tool_call_reply(
            "fs_write",
            serde_json::json!({"path": "planted.txt", "content": "planted by the model"}),
        ),
        reply("written", "stop", 7),
    ]);
    let (_dir, root) = temp_root();
    let files = TempDir::new().expect("a temp fs root");

    let answered = run_answering(
        &root,
        "phi",
        files.path(),
        &provider.base_url,
        PermissionOptionKind::AllowOnce,
    );

    assert_eq!(answered.stop, StopReason::EndTurn);
    assert!(
        chunks(&answered.updates).iter().any(|c| c == "written"),
        "the answer must reach the client, got: {:?}",
        chunks(&answered.updates)
    );

    // What the client was actually asked, off the wire. Nothing on `dev`
    // asserts these two option ids, and they are the strings
    // `AcpPermissionTransport::call` matches on: a typo in either would turn
    // every Allow into a silent denial.
    assert_eq!(answered.asked.len(), 1, "{:?}", answered.asked);
    let request = &answered.asked[0];
    assert_eq!(request.session_id, SessionId::new("heddle-1"));
    assert_eq!(request.tool_call.tool_call_id, ToolCallId::new("fs_write"));
    assert_eq!(request.tool_call.fields.title.as_deref(), Some("fs_write"));
    assert_eq!(
        request
            .options
            .iter()
            .map(|o| (o.option_id.0.as_ref(), o.kind))
            .collect::<Vec<_>>(),
        vec![
            ("heddle.allow-once", PermissionOptionKind::AllowOnce),
            ("heddle.reject-once", PermissionOptionKind::RejectOnce),
        ]
    );

    // The effect itself: the real connector wrote the model's exact bytes
    // through the real binary, because a human said yes.
    assert_eq!(
        std::fs::read_to_string(files.path().join("planted.txt")).expect("the planted file"),
        "planted by the model"
    );

    // And the model was told the truth about it.
    let _first = provider.request_body();
    let told = last_message(&provider.request_body());
    assert!(told.contains("wrote 20 bytes to planted.txt"), "{told}");

    assert_eq!(
        logged_kinds(&root, "phi", "heddle-1#1"),
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
        "phi",
        "--run",
        "heddle-1#1",
    ]);
    assert_eq!(
        verify.status.code(),
        Some(0),
        "stderr:\n{}",
        stderr(&verify)
    );
    assert_eq!(stdout(&verify), "heddle-1#1\tok\t14 steps\n");
}

#[test]
fn an_acp_client_that_rejects_stops_the_fs_write_and_the_run_survives() {
    let provider = StubProvider::serving(vec![
        tool_call_reply(
            "fs_write",
            serde_json::json!({"path": "planted.txt", "content": "planted by the model"}),
        ),
        reply("written", "stop", 7),
    ]);
    let (_dir, root) = temp_root();
    let files = TempDir::new().expect("a temp fs root");

    let answered = run_answering(
        &root,
        "chi",
        files.path(),
        &provider.base_url,
        PermissionOptionKind::RejectOnce,
    );

    // A refusal is still an ask, and it is the same ask.
    assert_eq!(answered.asked.len(), 1, "{:?}", answered.asked);
    let request = &answered.asked[0];
    assert_eq!(request.session_id, SessionId::new("heddle-1"));
    assert_eq!(request.tool_call.tool_call_id, ToolCallId::new("fs_write"));
    assert_eq!(request.tool_call.fields.title.as_deref(), Some("fs_write"));
    assert_eq!(
        request
            .options
            .iter()
            .map(|o| (o.option_id.0.as_ref(), o.kind))
            .collect::<Vec<_>>(),
        vec![
            ("heddle.allow-once", PermissionOptionKind::AllowOnce),
            ("heddle.reject-once", PermissionOptionKind::RejectOnce),
        ]
    );

    // Constitution VI, proven by an effect rather than by a counter: the test
    // above makes this exact call under this exact fixture create this exact
    // file. Its absence here is the effect the server would have had.
    assert!(
        !files.path().join("planted.txt").exists(),
        "a client's refusal must have had no effect whatsoever"
    );

    // The client's answer reaches the model verbatim, inside the payload the
    // next `llm_request` step records.
    let _first = provider.request_body();
    let told = last_message(&provider.request_body());
    assert!(
        told.contains("acp client declined permission") && told.contains("heddle.reject-once"),
        "the model must be told plainly who refused and why, got: {told}"
    );

    // The same shape `an_unlisted_write_never_reaches_the_server` pins for a
    // policy denial, at a different refusing layer: the attempt and the verdict
    // are on the chain and there is no `ToolResult`, because nothing ran.
    assert_eq!(
        logged_kinds(&root, "chi", "heddle-1#1"),
        vec![
            "iteration_boundary",
            "llm_request",
            "wire_exchange",
            "llm_response",
            "budget_spent",
            "tool_call",
            "approval",
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
        "chi",
        "--run",
        "heddle-1#1",
    ]);
    assert_eq!(
        verify.status.code(),
        Some(0),
        "stderr:\n{}",
        stderr(&verify)
    );
    assert_eq!(stdout(&verify), "heddle-1#1\tok\t13 steps\n");

    // A governed refusal is history the run survives, not an error.
    assert_eq!(answered.stop, StopReason::EndTurn);
    assert!(
        chunks(&answered.updates).iter().any(|c| c == "written"),
        "the run must go on to answer, got: {:?}",
        chunks(&answered.updates)
    );

    // The client saw the tool call and was never told it completed. Only the
    // absence is asserted: the projection leaves an ACP-denied call `Pending`
    // forever, which is a recorded residual this slice does not endorse.
    let seen = answered.updates.lock().expect("the update log");
    assert!(
        seen.iter()
            .any(|u| matches!(u, SessionUpdate::ToolCall(call) if call.title == "fs_write")),
        "the refused tool call must still be visible to the client: {seen:?}"
    );
    assert!(
        !seen.iter().any(|u| matches!(
            u,
            SessionUpdate::ToolCallUpdate(update)
                if update.fields.status == Some(ToolCallStatus::Completed)
        )),
        "nothing ran, so nothing may be reported completed: {seen:?}"
    );
}

// ---- the sandboxed process launcher (spec 019) ----

/// The whole chain, end to end, on the one platform that has a launcher: the
/// real binary, the real ACP protocol, a real human answer, a real process, and
/// the chain read back in a second process (SC-009).
///
/// Slice 018's harness verbatim, plus `--allow-run`. `cmd.exe /c type seed.txt`
/// because its output is a file's real bytes, so the assertion is about
/// something that had to actually run rather than about a status string.
#[cfg(windows)]
#[test]
fn an_acp_client_that_allows_lets_a_real_proc_run_execute() {
    let provider = StubProvider::serving(vec![
        tool_call_reply(
            "proc_run",
            serde_json::json!({"command": "cmd.exe", "args": ["/c", "type", "seed.txt"]}),
        ),
        reply("ran", "stop", 7),
    ]);
    let (_dir, root) = temp_root();
    let files = TempDir::new().expect("a temp fs root");
    std::fs::write(
        files.path().join("seed.txt"),
        "bytes only a real process could read",
    )
    .expect("a file for the sandboxed process to read");
    let _pruned = guard::PrunedOnDrop::of_root(files.path());

    let answered = run_answering_with_args(
        &root,
        "psi",
        files.path(),
        &provider.base_url,
        PermissionOptionKind::AllowOnce,
        &["--allow-run"],
    );

    assert_eq!(answered.stop, StopReason::EndTurn);

    // The ask, off the wire, in the shape slice 018 pinned for `fs_write`.
    assert_eq!(answered.asked.len(), 1, "{:?}", answered.asked);
    let request = &answered.asked[0];
    assert_eq!(request.session_id, SessionId::new("heddle-1"));
    assert_eq!(request.tool_call.tool_call_id, ToolCallId::new("proc_run"));
    assert_eq!(request.tool_call.fields.title.as_deref(), Some("proc_run"));
    assert_eq!(
        request
            .options
            .iter()
            .map(|o| (o.option_id.0.as_ref(), o.kind))
            .collect::<Vec<_>>(),
        vec![
            ("heddle.allow-once", PermissionOptionKind::AllowOnce),
            ("heddle.reject-once", PermissionOptionKind::RejectOnce),
        ]
    );

    // The effect: a process really ran inside the sandbox, and what the model
    // was told is the file's own bytes rather than a summary of them.
    let _first = provider.request_body();
    let told = last_message(&provider.request_body());
    assert!(
        told.contains("exit 0") && told.contains("bytes only a real process could read"),
        "{told}"
    );

    assert_eq!(
        logged_kinds(&root, "psi", "heddle-1#1"),
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
        "psi",
        "--run",
        "heddle-1#1",
    ]);
    assert_eq!(
        verify.status.code(),
        Some(0),
        "stderr:\n{}",
        stderr(&verify)
    );
    assert_eq!(stdout(&verify), "heddle-1#1\tok\t14 steps\n");
}

/// The same chain, refused (SC-010).
///
/// The command is chosen so its *effect* would be visible: the allow test above
/// proves a sandboxed `cmd.exe` can read and write inside this root, so
/// `planted.txt` not existing is the effect the launcher would have had.
#[cfg(windows)]
#[test]
fn an_acp_client_that_rejects_stops_the_proc_run_and_the_run_survives() {
    let provider = StubProvider::serving(vec![
        tool_call_reply(
            "proc_run",
            serde_json::json!({"command": "cmd.exe", "args": ["/c", "copy", "seed.txt", "planted.txt"]}),
        ),
        reply("ran", "stop", 7),
    ]);
    let (_dir, root) = temp_root();
    let files = TempDir::new().expect("a temp fs root");
    std::fs::write(files.path().join("seed.txt"), "the source of the copy")
        .expect("a file to copy");
    let _pruned = guard::PrunedOnDrop::of_root(files.path());

    let answered = run_answering_with_args(
        &root,
        "omega",
        files.path(),
        &provider.base_url,
        PermissionOptionKind::RejectOnce,
        &["--allow-run"],
    );

    // A refusal is still an ask, and it is the same ask.
    assert_eq!(answered.asked.len(), 1, "{:?}", answered.asked);
    let request = &answered.asked[0];
    assert_eq!(request.tool_call.tool_call_id, ToolCallId::new("proc_run"));
    assert_eq!(
        request
            .options
            .iter()
            .map(|o| (o.option_id.0.as_ref(), o.kind))
            .collect::<Vec<_>>(),
        vec![
            ("heddle.allow-once", PermissionOptionKind::AllowOnce),
            ("heddle.reject-once", PermissionOptionKind::RejectOnce),
        ]
    );

    // Constitution VI, proven by an effect rather than by a counter — and this
    // time the effect that did not happen is a whole process.
    assert!(
        !files.path().join("planted.txt").exists(),
        "a client's refusal must mean no process ran at all"
    );

    let _first = provider.request_body();
    let told = last_message(&provider.request_body());
    assert!(
        told.contains("acp client declined permission") && told.contains("heddle.reject-once"),
        "the model must be told plainly who refused and why, got: {told}"
    );

    // The same shape slice 018 pinned for a denied `fs_write`: the attempt and
    // the verdict are on the chain and there is no `ToolResult`.
    assert_eq!(
        logged_kinds(&root, "omega", "heddle-1#1"),
        vec![
            "iteration_boundary",
            "llm_request",
            "wire_exchange",
            "llm_response",
            "budget_spent",
            "tool_call",
            "approval",
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
        "omega",
        "--run",
        "heddle-1#1",
    ]);
    assert_eq!(
        verify.status.code(),
        Some(0),
        "stderr:\n{}",
        stderr(&verify)
    );
    assert_eq!(stdout(&verify), "heddle-1#1\tok\t13 steps\n");

    // A governed refusal is history the run survives, not an error.
    assert_eq!(answered.stop, StopReason::EndTurn);
    assert!(
        chunks(&answered.updates).iter().any(|c| c == "ran"),
        "the answer must still reach the client, got: {:?}",
        chunks(&answered.updates)
    );
}

/// The third opt-in (spec 020 SC-008), on `acp-agent` and **nowhere else**.
///
/// `heddle chat` does not carry `--allow-run`, so it must not carry the flag
/// that needs it either — `wiring::ToolArgs::chat_policy` records why a
/// mutating tool that could only ever be denied belongs absent from a
/// non-interactive command.
#[test]
fn acp_agent_documents_the_run_dir_flag_and_chat_does_not() {
    let agent = heddle(&["acp-agent", "--help"]);
    let chat = heddle(&["chat", "--help"]);

    assert_eq!(agent.status.code(), Some(0), "stderr:\n{}", stderr(&agent));
    assert!(
        stdout(&agent).contains("--run-dir"),
        "an opt-in capability an operator cannot discover is not opt-in, got:\n{}",
        stdout(&agent)
    );
    // The flag changes a directory's permissions and the change outlives the
    // run, so the operator has to meet that where they meet the flag.
    assert!(
        stdout(&agent).contains("read-and-execute"),
        "and what saying yes costs belongs in the flag's own help, got:\n{}",
        stdout(&agent)
    );

    assert_eq!(chat.status.code(), Some(0), "stderr:\n{}", stderr(&chat));
    assert!(
        !stdout(&chat).contains("--run-dir"),
        "`heddle chat` has nobody to ask for permission and must not offer it, got:\n{}",
        stdout(&chat)
    );
}

/// Deny-by-default at the operator boundary, against the real binary.
///
/// A run directory without run access is meaningless, and the two flags have to
/// be named together or an operator cannot tell which one they forgot.
#[test]
fn run_dir_without_allow_run_is_an_exit_code_naming_both_flags() {
    let (_dir, root) = temp_root();

    let out = heddle(&[
        "acp-agent",
        "--root",
        &root_arg(&root),
        "--silo",
        "tau",
        "--model",
        "llama3.1",
        "--fs-root",
        &root_arg(&root),
        "--run-dir",
        &root_arg(&root),
    ]);

    assert_ne!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "",
        "stdout is the protocol; nothing may go there"
    );
    let told = stderr(&out);
    assert!(
        told.contains("--run-dir") && told.contains("--allow-run"),
        "the refusal must name both flags: {told}"
    );
}

/// A mistyped `--run-dir` is an exit code before a chain is opened, for the
/// reason `--fs-root`'s own refusal documents: an operator wants to hear about
/// it before a model does.
#[test]
fn acp_agent_refuses_a_run_dir_that_does_not_exist_before_serving() {
    let (_dir, root) = temp_root();
    let missing = root.join("no-such-toolchain");

    let out = heddle(&[
        "acp-agent",
        "--root",
        &root_arg(&root),
        "--silo",
        "tau",
        "--model",
        "llama3.1",
        "--fs-root",
        &root_arg(&root),
        "--allow-run",
        "--run-dir",
        missing.to_str().expect("a utf-8 temp path"),
    ]);

    assert_ne!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "",
        "stdout is the protocol; nothing may go there"
    );
    assert!(
        stderr(&out).contains("no-such-toolchain"),
        "the refusal must name the path the operator gave, got:\n{}",
        stderr(&out)
    );
    assert!(
        !Silo::open(&root, "tau")
            .expect("a silo path")
            .ledger_path()
            .exists(),
        "a refused run directory must not open a chain"
    );
}

// ---- cancelling a tool call in flight (spec 027) ----

/// The composition root's own test, and the only one that can catch its one
/// silent mistake.
///
/// `heddle acp-agent`'s session factory mints one `Arc<AtomicBool>` and must
/// hand the **same** one to the session and to the tool transport. Wire two and
/// nothing fails loudly: `session/cancel` still reaches `CancellableModel`, the
/// next turn is still refused, and the prompt is still answered `Cancelled` —
/// thirty seconds later, once `RUN_TIMEOUT` expires and the child dies of its
/// own clock. So the assertion that matters here is **elapsed wall clock**, not
/// the stop reason, and reverting `acp.rs` to two flags is what proves it.
///
/// The command is the grandchild loop `heddle-sandbox`'s `tests/cancel.rs`
/// measured as the one thing an AppContainer with zero capability SIDs lets
/// keep running.
#[cfg(windows)]
#[test]
fn acp_agent_cancelling_a_proc_run_kills_it_without_waiting_for_its_timeout() {
    let provider = StubProvider::serving(vec![
        tool_call_reply(
            "proc_run",
            serde_json::json!({
                "command": "cmd.exe",
                "args": ["/c", "cmd.exe", "/c", "for", "/l", "%i", "in", "(1,1,2000000000)", "do", "@rem"]
            }),
        ),
        reply("never reached", "stop", 7),
    ]);
    let (_dir, root) = temp_root();
    let files = TempDir::new().expect("a temp fs root");
    let _pruned = guard::PrunedOnDrop::of_root(files.path());

    let opened: Arc<Mutex<Option<SessionId>>> = Arc::default();
    let known = opened.clone();
    let recorded = opened.clone();

    let transport = AcpAgent::new(AcpAgentConfig::new(env!("CARGO_BIN_EXE_heddle")).args([
        "acp-agent",
        "--root",
        &root_arg(&root),
        "--silo",
        "kappa",
        "--model",
        "llama3.1",
        "--base-url",
        &provider.base_url,
        "--fs-root",
        &root_arg(files.path()),
        "--timeout-secs",
        "60",
        "--allow-run",
    ]));

    let started = std::time::Instant::now();
    let stop = run_with_timeout(move || {
        futures::executor::block_on(
            Client
                .builder()
                .name("test-client")
                // Approve, then immediately press stop. The permission answer
                // is what unblocks the child's loop thread into the launch, so
                // this is the earliest moment a client could cancel a tool call
                // — and the flag survives until the launcher polls it, because
                // the run resets it once, before the first turn.
                .on_receive_request(
                    async move |request: RequestPermissionRequest, responder, cx| {
                        let option = request
                            .options
                            .iter()
                            .find(|o| o.kind == PermissionOptionKind::AllowOnce)
                            .expect("allow-once is offered");
                        let answered = responder.respond(RequestPermissionResponse::new(
                            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                                option.option_id.clone(),
                            )),
                        ));
                        let session = recorded.lock().expect("the session cell").clone();
                        cx.send_notification(CancelNotification::new(
                            session.expect("the session was opened before it was prompted"),
                        ))?;
                        answered
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .connect_with(transport, async move |cx: ConnectionTo<Agent>| {
                    cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                        .block_task()
                        .await?;
                    let session = cx
                        .send_request(NewSessionRequest::new(PathBuf::from(".")))
                        .block_task()
                        .await?;
                    *known.lock().expect("the session cell") = Some(session.session_id.clone());
                    let response = cx
                        .send_request(PromptRequest::new(
                            session.session_id,
                            vec![ContentBlock::Text(TextContent::new("run something long"))],
                        ))
                        .block_task()
                        .await?;
                    Ok(response.stop_reason)
                }),
        )
        .expect("the ACP client ran to completion")
    });
    let elapsed = started.elapsed();

    assert_eq!(stop, StopReason::Cancelled);
    // The assertion the slice exists for. A composition root holding two flags
    // reaches this line too — after `RUN_TIMEOUT`.
    assert!(
        elapsed < heddle_connectors::RUN_TIMEOUT,
        "the child must die on the flag and not on its own {:?} clock; the prompt took {elapsed:?}",
        heddle_connectors::RUN_TIMEOUT
    );
}
