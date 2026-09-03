//! Acceptance tests for `skein acp-agent` (spec 013).
//!
//! The headline test is the one thing no prior slice proved: a **real ACP
//! client from the `agent-client-protocol` crate** spawning the **real `skein`
//! binary as a subprocess** and speaking to it over the child's actual stdio —
//! the same transport an editor uses. Slice 008's suite drives the same facade
//! in-process over a `tokio::io::duplex`, which cannot fail on argument
//! parsing, on an exit code, or on a stray byte written to stdout.
//!
//! The model behind it is a `std::net::TcpListener` serving real HTTP/1.1
//! chat-completions bytes from this test process, so no test here needs a
//! running Ollama and none needs an installed editor.

use agent_client_protocol::schema::v1::{
    ContentBlock, InitializeRequest, NewSessionRequest, PromptRequest, SessionId,
    SessionNotification, SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{AcpAgent, AcpAgentConfig, Agent, Client, ConnectionTo};
use skein_silo::Silo;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers. Copied from `cli_chat.rs` rather than shared — the same reasoning
// `skein-acp/tests/acp_session.rs` records for its own doubles, and `skein-cli`
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

fn skein(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_skein"))
        .args(args)
        .env_remove("SKEIN_ROOT")
        .env_remove("SKEIN_MODEL_BASE_URL")
        .output()
        .expect("the skein binary runs")
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

/// The step kinds `skein ledger log` reports for one run, in chain order.
fn logged_kinds(root: &Path, silo: &str, run_id: &str) -> Vec<String> {
    let log = skein(&[
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
    // unset a stray `$SKEIN_MODEL_BASE_URL` for the child.
    let transport = AcpAgent::new(AcpAgentConfig::new(env!("CARGO_BIN_EXE_skein")).args([
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
    assert_eq!(session_id, SessionId::new("skein-1"));
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
        "llm_response",
        "budget_spent",
        "exit",
    ];
    assert_eq!(logged_kinds(&root, "alpha", "skein-1#1"), expected);
    assert_eq!(logged_kinds(&root, "alpha", "skein-1#2"), expected);

    let verify = skein(&["ledger", "verify", "--root", &root_flag, "--silo", "alpha"]);
    assert_eq!(
        verify.status.code(),
        Some(0),
        "stderr:\n{}",
        stderr(&verify)
    );
    assert_eq!(
        stdout(&verify),
        "skein-1#1\tok\t5 steps\nskein-1#2\tok\t5 steps\n"
    );
}

#[test]
fn acp_agent_refuses_a_non_loopback_base_url_before_serving() {
    let (_dir, root) = temp_root();

    // No ACP client: nothing should get as far as a handshake.
    let out = skein(&[
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
    let missing = format!("keychain://skein-acp-absent-{}/cli", std::process::id());

    // No ACP client: a redaction reference that resolves to nothing must stop
    // the command before the handshake, exactly as a non-loopback base URL does.
    let out = skein(&[
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
    let child = Command::new(env!("CARGO_BIN_EXE_skein"))
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
        .env_remove("SKEIN_ROOT")
        .env_remove("SKEIN_MODEL_BASE_URL")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the skein binary runs");
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
    // builds its connector inside `SkeinAgent::open`, which runs under
    // `futures::executor::block_on` -- and `Runtime::block_on` panics inside an
    // entered *tokio* runtime. Only a real handshake against the real binary
    // shows that the distinction holds.
    let transport = AcpAgent::new(AcpAgentConfig::new(env!("CARGO_BIN_EXE_skein")).args([
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
    // is allowlisted and approved here, unlike in `skein chat`, because there
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
        logged_kinds(&root, "sigma", "skein-1#1"),
        vec![
            "iteration_boundary",
            "llm_request",
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
    let out = skein(&[
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
    let help = skein(&["acp-agent", "--help"]);

    assert_eq!(help.status.code(), Some(0), "stderr:\n{}", stderr(&help));
    assert!(
        stdout(&help).contains("--fs-root"),
        "an opt-in capability an operator cannot discover is not opt-in, got:\n{}",
        stdout(&help)
    );
}
