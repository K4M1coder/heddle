//! Acceptance tests for the Tauri shell's ACP client (spec 021).
//!
//! Same shape as `crates/heddle-cli/tests/cli_acp_agent.rs`, one role over: that
//! file proves an ACP client can drive the real `heddle` binary, this one proves
//! that **the client the desktop app actually ships** does. The model is a
//! `std::net::TcpListener` in this test process, so nothing here needs a running
//! Ollama, and no test opens a window — `session.rs` names no Tauri type, which
//! is what makes that possible.
//!
//! The stub provider is gated: it does not answer a turn until the test says
//! so. That is what makes the cancellation test a fact about ordering rather
//! than about how fast a runner happens to be.

use agent_client_protocol::schema::v1::{SessionUpdate, StopReason};
use heddle_ui::session::{AgentLaunch, SessionHandle};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

/// Long enough that a slow runner never trips it, short enough that a child
/// which silently sends nothing fails as a failure rather than as a hang.
const OBSERVE_TIMEOUT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// The model: a real HTTP/1.1 server on loopback, answering a fixed script.
// Copied from `cli_acp_agent.rs` rather than shared — `heddle-cli` has no lib
// target to share it through, and this crate must not grow a dependency on the
// CLI to borrow a test double.
// ---------------------------------------------------------------------------

/// A local provider answering a fixed script, one connection per turn, which
/// waits for `answer` before writing each response.
struct StubProvider {
    base_url: String,
    requests: Receiver<String>,
    gate: Sender<()>,
}

impl StubProvider {
    fn serving(bodies: Vec<String>) -> StubProvider {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let addr = listener.local_addr().expect("the bound address");
        let (tx, requests) = mpsc::channel();
        let (gate, wait) = mpsc::channel::<()>();
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
                // The turn hangs here until the test opens the gate, so a test
                // can act while the loop thread is provably mid-turn.
                if wait.recv().is_err() {
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
            gate,
        }
    }

    /// Blocks until the child has sent its next request, and returns its body.
    fn awaited_request(&self) -> String {
        match self.requests.recv_timeout(OBSERVE_TIMEOUT) {
            Ok(body) => body,
            Err(RecvTimeoutError::Timeout) => {
                panic!("the child sent no request within {OBSERVE_TIMEOUT:?}")
            }
            Err(RecvTimeoutError::Disconnected) => {
                panic!("the stub provider stopped before a request arrived")
            }
        }
    }

    /// Lets the provider answer `turns` turns.
    fn answer(&self, turns: usize) {
        for _ in 0..turns {
            self.gate.send(()).expect("the stub provider is still up");
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
/// a terminating `[DONE]`.
fn sse(events: Vec<serde_json::Value>) -> String {
    let mut raw = String::new();
    for event in events {
        raw.push_str(&format!("data: {event}\n\n"));
    }
    raw.push_str("data: [DONE]\n\n");
    raw
}

fn reply(content: &str) -> String {
    sse(vec![
        serde_json::json!({
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": content}}]
        }),
        serde_json::json!({
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
        }),
        serde_json::json!({"choices": [], "usage": {"total_tokens": 25}}),
    ])
}

/// A model turn that asks for a tool. With no `--fs-root` the policy allows
/// nothing, so the call is refused and captured, and the run goes round again —
/// which is exactly the second turn the cancellation test needs.
fn tool_call_reply(tool: &str) -> String {
    sse(vec![
        serde_json::json!({
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_1",
                        "function": {"name": tool, "arguments": "{}"}
                    }]
                }
            }]
        }),
        serde_json::json!({
            "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]
        }),
        serde_json::json!({"choices": [], "usage": {"total_tokens": 12}}),
    ])
}

// ---------------------------------------------------------------------------
// The subject.
// ---------------------------------------------------------------------------

/// The `heddle` binary this test drives.
///
/// `CARGO_BIN_EXE_*` only covers the *current* package's binaries and `heddle`
/// belongs to `heddle-cli`, so the path is derived from this test binary's own
/// location (`target/<profile>/deps/`) instead. `cargo test --workspace` builds
/// it; `cargo test -p heddle-ui` alone does not, hence the explicit remedy.
fn heddle_binary() -> PathBuf {
    let mut path = std::env::current_exe().expect("the test binary's own path");
    path.pop(); // deps/
    path.pop(); // <profile>/
    path.push(format!("heddle{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.is_file(),
        "{} is missing: run `cargo build -p heddle-cli --bin heddle` \
         (or `cargo test --workspace`, which builds it)",
        path.display()
    );
    path
}

/// One started session against a stub provider, with everything the shell
/// relayed recorded.
struct Started {
    handle: SessionHandle,
    updates: Arc<Mutex<Vec<SessionUpdate>>>,
    exits: Receiver<String>,
    _root: TempDir,
}

fn start(provider: &StubProvider) -> Started {
    let root = TempDir::new().expect("a temp root");
    let launch = AgentLaunch::new(heddle_binary())
        .args([
            "acp-agent",
            "--root",
            root.path().to_str().expect("a utf-8 temp path"),
            "--silo",
            "alpha",
            "--model",
            "llama3.1",
            // Passed as a flag, not left to the environment: the child inherits
            // this process's environment and a stray `$HEDDLE_MODEL_BASE_URL`
            // could not be unset for it.
            "--base-url",
            &provider.base_url,
            "--timeout-secs",
            "10",
        ])
        .cwd(root.path());

    let updates: Arc<Mutex<Vec<SessionUpdate>>> = Arc::default();
    let collected = updates.clone();
    let (exit_tx, exits) = mpsc::channel();

    let handle = SessionHandle::start(
        launch,
        move |notification| {
            collected
                .lock()
                .expect("the update log")
                .push(notification.update);
        },
        move |reason| {
            let _ = exit_tx.send(reason);
        },
    )
    .expect("the shell started a session against the real binary");

    Started {
        handle,
        updates,
        exits,
        _root: root,
    }
}

/// The assistant texts the shell relayed, in order.
fn texts(updates: &Arc<Mutex<Vec<SessionUpdate>>>) -> Vec<String> {
    updates
        .lock()
        .expect("the update log")
        .iter()
        .filter_map(|update| match update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                serde_json::to_value(&chunk.content).ok().and_then(|value| {
                    value
                        .get("text")
                        .and_then(|text| text.as_str())
                        .map(str::to_string)
                })
            }
            _ => None,
        })
        .collect()
}

/// `block_on` has no timeout and `cargo test` has no per-test limit, so without
/// this a regression is a hung CI job on three operating systems instead of a
/// failure.
fn with_timeout<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(60))
        .expect("the shell finished within 60s")
}

// ---------------------------------------------------------------------------
// The acceptance tests.
// ---------------------------------------------------------------------------

#[test]
fn starting_a_session_spawns_the_real_agent_and_names_the_session() {
    let provider = StubProvider::serving(vec![]);
    let started = start(&provider);

    // Deterministic: the facade mints session ids from an AtomicU64 starting at
    // 1, and this is a fresh child process.
    assert_eq!(started.handle.session_id(), "heddle-1");
}

#[test]
fn a_prompt_is_answered_and_its_transcript_is_relayed_before_the_answer() {
    let provider = StubProvider::serving(vec![reply("the answer is 42")]);
    let started = start(&provider);

    let prompting = started.handle.clone();
    let answered = std::thread::spawn(move || {
        futures::executor::block_on(prompting.prompt("what is the answer?"))
    });

    provider.awaited_request();
    provider.answer(1);

    let stop = answered
        .join()
        .expect("the prompt thread finished")
        .expect("the prompt was answered");
    assert_eq!(stop, StopReason::EndTurn);

    // Sent before the response, so a client holding the answer has already been
    // told about the run that produced it (`crates/heddle-acp/src/lib.rs`).
    assert_eq!(
        texts(&started.updates),
        vec!["the answer is 42".to_string()]
    );
}

#[test]
fn two_prompts_run_on_one_session_and_both_transcripts_arrive() {
    let provider = StubProvider::serving(vec![reply("the answer is 42"), reply("and 43")]);
    let started = start(&provider);

    for text in ["what is the answer?", "and the next one?"] {
        let prompting = started.handle.clone();
        let owned = text.to_string();
        let answered =
            std::thread::spawn(move || futures::executor::block_on(prompting.prompt(&owned)));
        provider.awaited_request();
        provider.answer(1);
        assert_eq!(
            answered.join().expect("the prompt thread finished"),
            Ok(StopReason::EndTurn)
        );
    }

    assert_eq!(
        texts(&started.updates),
        vec!["the answer is 42".to_string(), "and 43".to_string()]
    );
    assert_eq!(started.handle.session_id(), "heddle-1");
}

#[test]
fn a_cancel_stops_the_run_at_the_next_turn_boundary_and_says_so() {
    // Turn 1 asks for a tool. With no `--fs-root` the policy refuses it, the
    // refusal is captured, and the loop goes round — so there *is* a turn 2 for
    // the cancellation to land on.
    let provider = StubProvider::serving(vec![tool_call_reply("fs_write"), reply("unreachable")]);
    let started = start(&provider);

    let prompting = started.handle.clone();
    let answered =
        std::thread::spawn(move || futures::executor::block_on(prompting.prompt("write a file")));

    // The loop thread is now blocked inside turn 1's HTTP call, waiting on the
    // gate: nothing can advance until this test lets it.
    provider.awaited_request();

    started.handle.cancel().expect("the cancel was sent");
    // A round trip *after* the cancel. JSON-RPC dispatch is ordered, so an
    // answer to this proves the child already processed the cancel — which is
    // what makes this test about ordering rather than about timing.
    futures::executor::block_on(started.handle.ping()).expect("the agent is alive");

    provider.answer(1);

    let stop = answered
        .join()
        .expect("the prompt thread finished")
        .expect("the prompt was answered");
    assert_eq!(
        stop,
        StopReason::Cancelled,
        "a cancel that lands mid-run must be reported as cancelled, not as a normal end of turn"
    );
    // Turn 1 completed: cancellation is not mid-turn
    // (`crates/heddle-acp/src/cancel.rs`).
    assert!(
        !texts(&started.updates).contains(&"unreachable".to_string()),
        "the turn after the cancel must not have run"
    );
}

#[test]
fn cancelling_with_nothing_in_flight_is_not_an_error() {
    let provider = StubProvider::serving(vec![]);
    let started = start(&provider);

    assert_eq!(started.handle.cancel(), Ok(()));
    assert!(started.exits.try_recv().is_err(), "the agent is still up");
}

#[test]
fn dropping_the_handle_shuts_the_agent_down_and_reports_it_once() {
    let provider = StubProvider::serving(vec![]);
    let started = start(&provider);
    let exits = started.exits;

    drop(started.handle);

    // The child's stdin closes, `heddle acp-agent` exits zero, and the shell says
    // so exactly once — a window that keeps accepting messages into a dead pipe
    // is the failure this callback exists to prevent.
    let reason = exits
        .recv_timeout(OBSERVE_TIMEOUT)
        .expect("the shell reported the agent's exit");
    assert!(
        !reason.is_empty(),
        "the exit reason must be something a status line can show, got {reason:?}"
    );
}

#[test]
fn a_prompt_after_the_agent_is_gone_fails_instead_of_hanging() {
    let provider = StubProvider::serving(vec![]);
    let started = start(&provider);
    let handle = started.handle.clone();
    // `close`, not `drop`: a clone is still alive, and the session outliving one
    // of its handles is the whole point of the handle being cloneable.
    started.handle.close();

    let outcome = with_timeout(move || futures::executor::block_on(handle.prompt("anyone there?")));
    assert!(
        outcome.is_err(),
        "a prompt on a closed session must fail, got {outcome:?}"
    );
}
