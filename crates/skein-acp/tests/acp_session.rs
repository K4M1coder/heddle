//! One real ACP client and one real ACP agent, over a real byte-stream
//! transport, driving the existing governed loop.

use agent_client_protocol::schema::v1::{
    ContentBlock, InitializeRequest, NewSessionRequest, PermissionOptionKind, PromptRequest,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionId, SessionNotification, SessionUpdate, StopReason,
    TextContent, ToolCallStatus,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{Agent, ByteStreams, Client, ConnectionTo};
use skein_acp::{project_updates, CancellableModel, SessionParts, SkeinAgent};
use skein_core::{
    CapturedResult, Ledger, LoopBudget, Message, ModelClient, ProgressProbe, Redactor, Result,
    Role, StepKind, ToolAccess, ToolCall, ToolOutcome, ToolPolicy, ToolSpec, ToolTransport,
    TurnRequest, TurnResponse,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

// ---------------------------------------------------------------------------
// Doubles. Modelled on the private ones in skein-core's and skein-mcp's test
// binaries; copied rather than moved, so those stay this slice's controls.
// ---------------------------------------------------------------------------

/// Replays a fixed script, counting calls. The last entry repeats.
struct ScriptedModel {
    script: Vec<TurnResponse>,
    calls: Arc<AtomicUsize>,
    /// Set for the cancellation test: blocks each turn until the test releases it.
    gate: Option<std::sync::mpsc::Receiver<()>>,
    started: Option<std::sync::mpsc::Sender<()>>,
}

impl ModelClient for ScriptedModel {
    fn turn(&mut self, _req: &TurnRequest) -> Result<TurnResponse> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(started) = &self.started {
            let _ = started.send(());
        }
        if let Some(gate) = &self.gate {
            let _ = gate.recv();
        }
        Ok(self.script[n.min(self.script.len() - 1)].clone())
    }
}

struct StaticProbe(bool);

impl ProgressProbe for StaticProbe {
    fn observe(&mut self) -> bool {
        self.0
    }
}

/// The ground truth for "did the tool actually run".
struct CountingTransport {
    calls: Arc<AtomicUsize>,
    content: String,
}

impl ToolTransport for CountingTransport {
    fn call(&mut self, _call: &ToolCall) -> Result<ToolOutcome> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ToolOutcome {
            content: self.content.clone(),
        })
    }
}

fn asks_for(tool: &str) -> TurnResponse {
    TurnResponse {
        message: Message::assistant_text("working"),
        tokens_used: 1,
        final_output: false,
        tool_calls: vec![ToolCall::with_id(
            "call_1",
            tool,
            serde_json::json!({"path": "x"}),
        )],
    }
}

fn finishes(text: &str) -> TurnResponse {
    TurnResponse {
        message: Message::assistant_text(text),
        tokens_used: 1,
        final_output: true,
        tool_calls: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Fixture: a real client and a real agent over an in-process duplex.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Answer {
    Allow,
    Reject,
}

/// What the ACP client observed, for assertions the agent side cannot make.
#[derive(Clone, Default)]
struct Observed {
    updates: Arc<Mutex<Vec<SessionUpdate>>>,
    permission_requests: Arc<AtomicUsize>,
}

impl Observed {
    fn chunks(&self) -> Vec<String> {
        self.updates
            .lock()
            .unwrap()
            .iter()
            .filter_map(|u| match u {
                SessionUpdate::AgentMessageChunk(chunk) => match &chunk.content {
                    ContentBlock::Text(text) => Some(text.text.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }
}

/// Serves `agent` on one half of a duplex and drives `main` from a real ACP
/// client on the other, answering every permission request with `answer`.
async fn with_facade<C, P, T, F, R>(
    agent: SkeinAgent<C, P, T, F>,
    answer: Answer,
    observed: Observed,
    main: impl AsyncFnOnce(ConnectionTo<Agent>) -> agent_client_protocol::Result<R>,
) -> R
where
    C: ModelClient + Send + 'static,
    P: ProgressProbe + Send + 'static,
    T: ToolTransport + Send + 'static,
    F: FnMut() -> Result<SessionParts<C, P, T>> + Send + 'static,
{
    let (agent_side, client_side) = tokio::io::duplex(65536);
    let (agent_read, agent_write) = tokio::io::split(agent_side);
    let (client_read, client_write) = tokio::io::split(client_side);

    let served = tokio::spawn(agent.serve(ByteStreams::new(
        agent_write.compat_write(),
        agent_read.compat(),
    )));

    let updates = observed.updates.clone();
    let permissions = observed.permission_requests.clone();
    let out = Client
        .builder()
        .name("test-client")
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                updates.lock().unwrap().push(notification.update);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, _cx| {
                permissions.fetch_add(1, Ordering::SeqCst);
                let wanted = match answer {
                    Answer::Allow => PermissionOptionKind::AllowOnce,
                    Answer::Reject => PermissionOptionKind::RejectOnce,
                };
                let option = request
                    .options
                    .iter()
                    .find(|o| o.kind == wanted)
                    .expect("the facade offers an allow-once and a reject-once option");
                responder.respond(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                        option.option_id.clone(),
                    )),
                ))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(
            ByteStreams::new(client_write.compat_write(), client_read.compat()),
            main,
        )
        .await
        .expect("the client ran to completion");

    served.abort();
    out
}

/// `initialize` then `session/new`, returning the session id the facade minted.
async fn open_session(cx: &ConnectionTo<Agent>) -> agent_client_protocol::Result<SessionId> {
    cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
        .block_task()
        .await?;
    let session = cx
        .send_request(NewSessionRequest::new(PathBuf::from(".")))
        .block_task()
        .await?;
    Ok(session.session_id)
}

fn prompt(session_id: &SessionId, text: &str) -> PromptRequest {
    PromptRequest::new(
        session_id.clone(),
        vec![ContentBlock::Text(TextContent::new(text))],
    )
}

/// One session's worth of collaborators, with the tool policy under test.
fn factory(
    script: Vec<TurnResponse>,
    model_calls: Arc<AtomicUsize>,
    tool_calls: Arc<AtomicUsize>,
    allowed: Vec<(String, ToolAccess)>,
    approved: Vec<String>,
) -> impl FnMut() -> Result<SessionParts<ScriptedModel, StaticProbe, CountingTransport>> + Send + 'static
{
    move || {
        Ok(SessionParts {
            client: ScriptedModel {
                script: script.clone(),
                calls: model_calls.clone(),
                gate: None,
                started: None,
            },
            probe: StaticProbe(true),
            transport: CountingTransport {
                calls: tool_calls.clone(),
                content: "file contents".into(),
            },
            policy: ToolPolicy::new(allowed.clone(), approved.clone()),
            redactor: Redactor::new(Vec::new()),
            budget: LoopBudget::new(8, 10_000, 8),
            ledger: Ledger::new(),
        })
    }
}

fn read_only(tool: &str) -> Vec<(String, ToolAccess)> {
    vec![(tool.to_string(), ToolAccess::ReadOnly)]
}

// ---------------------------------------------------------------------------
// The acceptance test: the slice's reason to exist.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a1_one_acp_session_drives_one_governed_turn_end_to_end() {
    let tool_calls = Arc::new(AtomicUsize::new(0));
    let observed = Observed::default();
    let agent = SkeinAgent::new(factory(
        vec![asks_for("read_file"), finishes("all done")],
        Arc::new(AtomicUsize::new(0)),
        tool_calls.clone(),
        read_only("read_file"),
        Vec::new(),
    ));

    let inspect = agent.clone();
    let (session_id, stop) = with_facade(
        agent,
        Answer::Allow,
        observed.clone(),
        async |cx: ConnectionTo<Agent>| {
            let session_id = open_session(&cx).await?;
            let response = cx
                .send_request(prompt(&session_id, "go"))
                .block_task()
                .await?;
            Ok((session_id, response.stop_reason))
        },
    )
    .await;

    assert_eq!(tool_calls.load(Ordering::SeqCst), 1);
    assert_eq!(stop, StopReason::EndTurn);
    assert_eq!(observed.permission_requests.load(Ordering::SeqCst), 1);
    assert!(observed.chunks().iter().any(|c| c == "all done"));

    let session = inspect.session(&session_id).expect("session is registered");
    let session = session.lock().unwrap();
    let run_id = format!("{session_id}#1");
    session
        .ledger()
        .verify_chain(&run_id)
        .expect("the run's chain verifies");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a8_the_session_runs_in_the_ledger_the_operator_injected() {
    let observed = Observed::default();
    // Seeded before the session exists, so a chain that lacks this step is a
    // chain the facade made for itself.
    let mut seeded = Ledger::new();
    seeded
        .append("prior-run", StepKind::LlmRequest, "from an earlier process")
        .unwrap();
    let mut once = Some(seeded);
    let agent = SkeinAgent::new(move || {
        Ok(SessionParts {
            client: ScriptedModel {
                script: vec![finishes("all done")],
                calls: Arc::new(AtomicUsize::new(0)),
                gate: None,
                started: None,
            },
            probe: StaticProbe(true),
            transport: CountingTransport {
                calls: Arc::new(AtomicUsize::new(0)),
                content: "unused".into(),
            },
            policy: ToolPolicy::new(Vec::new(), Vec::new()),
            redactor: Redactor::new(Vec::new()),
            budget: LoopBudget::new(8, 10_000, 8),
            ledger: once.take().expect("one session only"),
        })
    });

    let inspect = agent.clone();
    let session_id = with_facade(
        agent,
        Answer::Allow,
        observed.clone(),
        async |cx: ConnectionTo<Agent>| {
            let session_id = open_session(&cx).await?;
            cx.send_request(prompt(&session_id, "go"))
                .block_task()
                .await?;
            Ok(session_id)
        },
    )
    .await;

    let session = inspect.session(&session_id).expect("session is registered");
    let session = session.lock().unwrap();
    assert_eq!(
        session.ledger().log("prior-run").len(),
        1,
        "the session adopted the injected chain rather than starting its own"
    );
    session
        .ledger()
        .verify_chain(&format!("{session_id}#1"))
        .expect("the run landed in that same chain and verifies");
}

/// The one secret this session is configured to keep out of its chain.
const SECRET: &str = "sk-SECRET-abc123";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a10_a_secret_is_redacted_from_a_sessions_chain_and_from_the_client_transcript() {
    let observed = Observed::default();
    let agent = SkeinAgent::new(move || {
        Ok(SessionParts {
            client: ScriptedModel {
                script: vec![finishes(&format!("your key {SECRET} is fine"))],
                calls: Arc::new(AtomicUsize::new(0)),
                gate: None,
                started: None,
            },
            probe: StaticProbe(true),
            transport: CountingTransport {
                calls: Arc::new(AtomicUsize::new(0)),
                content: "unused".into(),
            },
            policy: ToolPolicy::new(Vec::new(), Vec::new()),
            redactor: Redactor::new(vec![SECRET.into()]),
            budget: LoopBudget::new(8, 10_000, 8),
            ledger: Ledger::new(),
        })
    });

    let inspect = agent.clone();
    let session_id = with_facade(
        agent,
        Answer::Allow,
        observed.clone(),
        async |cx: ConnectionTo<Agent>| {
            let session_id = open_session(&cx).await?;
            cx.send_request(prompt(&session_id, &format!("my key is {SECRET}")))
                .block_task()
                .await?;
            Ok(session_id)
        },
    )
    .await;

    let session = inspect.session(&session_id).expect("session is registered");
    let session = session.lock().unwrap();
    let payloads: Vec<String> = session
        .ledger()
        .log(&format!("{session_id}#1"))
        .iter()
        .map(|s| s.payload.clone())
        .collect();
    assert!(
        payloads.iter().all(|p| !p.contains(SECRET)),
        "the redactor the operator injected governs the whole chain: {payloads:?}"
    );
    assert!(payloads.iter().any(|p| p.contains("***")));

    // The consequence of deriving the transcript from the chain, pinned as
    // intended behaviour: an editor shows the operator *** where a configured
    // secret was. `skein chat` is different on purpose - it prints the raw
    // final message, not the payload.
    let chunks = observed.chunks();
    assert_eq!(chunks, vec!["your key *** is fine".to_string()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a9_a_factory_that_fails_makes_session_new_fail_and_leaves_the_connection_usable() {
    // The first real caller opens a silo per session, which is a SQLite file
    // open plus a full replay of the chain — fallible on permissions, a locked
    // file, or a corrupt store. Panicking would poison the factory mutex and
    // take every other session with it; falling back to an in-memory `Ledger`
    // would run the session with nothing persisted. Neither is acceptable, so
    // the factory returns a `Result` and the refusal is a JSON-RPC error the
    // client can show its user.
    let mut fail_next = true;
    let agent = SkeinAgent::new(move || {
        if std::mem::replace(&mut fail_next, false) {
            return Err(skein_core::SkeinError::Storage("the silo is locked".into()));
        }
        Ok(SessionParts {
            client: ScriptedModel {
                script: vec![finishes("all done")],
                calls: Arc::new(AtomicUsize::new(0)),
                gate: None,
                started: None,
            },
            probe: StaticProbe(true),
            transport: CountingTransport {
                calls: Arc::new(AtomicUsize::new(0)),
                content: "unused".into(),
            },
            policy: ToolPolicy::new(Vec::new(), Vec::new()),
            redactor: Redactor::new(Vec::new()),
            budget: LoopBudget::new(8, 10_000, 8),
            ledger: Ledger::new(),
        })
    });

    let (refusal, second) = with_facade(
        agent,
        Answer::Allow,
        Observed::default(),
        async |cx: ConnectionTo<Agent>| {
            cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;
            let refused = cx
                .send_request(NewSessionRequest::new(PathBuf::from(".")))
                .block_task()
                .await;
            // On the same connection, after the refusal: a dead connection
            // would fail here too, and the point is that it does not.
            let second = cx
                .send_request(NewSessionRequest::new(PathBuf::from(".")))
                .block_task()
                .await?;
            Ok((refused.err(), second.session_id))
        },
    )
    .await;

    let refusal = refusal.expect("session/new is refused when the factory fails");
    assert!(
        format!("{refusal:?}").contains("the silo is locked"),
        "the client is told what failed, got: {refusal:?}"
    );
    // A refused open still consumes its id: ids are opaque and are never
    // reused, so uniqueness does not depend on the factory succeeding.
    assert_eq!(second, SessionId::new("skein-2"));
}

// ---------------------------------------------------------------------------
// Permission is gate 2, never gate 1 (Constitution VI).
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a2_unlisted_tool_never_produces_a_permission_request() {
    let tool_calls = Arc::new(AtomicUsize::new(0));
    let observed = Observed::default();
    let agent = SkeinAgent::new(factory(
        vec![asks_for("rm_rf"), finishes("all done")],
        Arc::new(AtomicUsize::new(0)),
        tool_calls.clone(),
        read_only("read_file"),
        Vec::new(),
    ));

    let inspect = agent.clone();
    let (session_id, stop) = with_facade(
        agent,
        Answer::Allow,
        observed.clone(),
        async |cx: ConnectionTo<Agent>| {
            let session_id = open_session(&cx).await?;
            let response = cx
                .send_request(prompt(&session_id, "go"))
                .block_task()
                .await?;
            Ok((session_id, response.stop_reason))
        },
    )
    .await;

    assert_eq!(observed.permission_requests.load(Ordering::SeqCst), 0);
    assert_eq!(tool_calls.load(Ordering::SeqCst), 0);
    assert_eq!(stop, StopReason::EndTurn);

    let session = inspect.session(&session_id).expect("session is registered");
    let session = session.lock().unwrap();
    let kinds: Vec<StepKind> = session
        .ledger()
        .log(&format!("{session_id}#1"))
        .iter()
        .map(|s| s.kind.clone())
        .collect();
    assert!(kinds.contains(&StepKind::ToolCall));
    assert!(kinds.contains(&StepKind::Approval));
    assert!(!kinds.contains(&StepKind::ToolResult));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a3_client_decline_stops_the_tool_and_the_run_survives() {
    let tool_calls = Arc::new(AtomicUsize::new(0));
    let observed = Observed::default();
    let agent = SkeinAgent::new(factory(
        vec![asks_for("read_file"), finishes("all done")],
        Arc::new(AtomicUsize::new(0)),
        tool_calls.clone(),
        read_only("read_file"),
        Vec::new(),
    ));

    let inspect = agent.clone();
    let (session_id, stop) = with_facade(
        agent,
        Answer::Reject,
        observed.clone(),
        async |cx: ConnectionTo<Agent>| {
            let session_id = open_session(&cx).await?;
            let response = cx
                .send_request(prompt(&session_id, "go"))
                .block_task()
                .await?;
            Ok((session_id, response.stop_reason))
        },
    )
    .await;

    assert_eq!(observed.permission_requests.load(Ordering::SeqCst), 1);
    assert_eq!(tool_calls.load(Ordering::SeqCst), 0);
    assert_eq!(stop, StopReason::EndTurn);

    let session = inspect.session(&session_id).expect("session is registered");
    let session = session.lock().unwrap();
    let log = session.ledger().log(&format!("{session_id}#1"));
    let requests: Vec<&String> = log
        .iter()
        .filter(|s| s.kind == StepKind::LlmRequest)
        .map(|s| &s.payload)
        .collect();
    let replayed: TurnRequest =
        serde_json::from_str(requests.last().expect("a second request was made"))
            .expect("the captured request parses");
    let told = replayed.messages.last().expect("a fed-back refusal");
    assert_eq!(told.role, Role::Tool);
    assert_eq!(told.tool_call_id.as_deref(), Some("call_1"));
    assert_eq!(
        told.text(),
        "the read_file tool call was refused: acp client declined permission (skein.reject-once)",
        "the model is told plainly who refused and why"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a4_mutating_tool_without_approval_never_reaches_the_client() {
    let tool_calls = Arc::new(AtomicUsize::new(0));
    let observed = Observed::default();
    let agent = SkeinAgent::new(factory(
        vec![asks_for("fs_write"), finishes("all done")],
        Arc::new(AtomicUsize::new(0)),
        tool_calls.clone(),
        vec![("fs_write".to_string(), ToolAccess::Mutating)],
        Vec::new(),
    ));

    let (_, stop) = with_facade(
        agent,
        Answer::Allow,
        observed.clone(),
        async |cx: ConnectionTo<Agent>| {
            let session_id = open_session(&cx).await?;
            let response = cx
                .send_request(prompt(&session_id, "go"))
                .block_task()
                .await?;
            Ok((session_id, response.stop_reason))
        },
    )
    .await;

    assert_eq!(observed.permission_requests.load(Ordering::SeqCst), 0);
    assert_eq!(tool_calls.load(Ordering::SeqCst), 0);
    assert_eq!(stop, StopReason::EndTurn);
}

// ---------------------------------------------------------------------------
// Traceability (Constitution V).
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a5_every_agent_message_chunk_corresponds_to_a_ledger_step() {
    let observed = Observed::default();
    let agent = SkeinAgent::new(factory(
        vec![asks_for("read_file"), finishes("all done")],
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
        read_only("read_file"),
        Vec::new(),
    ));

    let inspect = agent.clone();
    let session_id = with_facade(
        agent,
        Answer::Allow,
        observed.clone(),
        async |cx: ConnectionTo<Agent>| {
            let session_id = open_session(&cx).await?;
            cx.send_request(prompt(&session_id, "go"))
                .block_task()
                .await?;
            Ok(session_id)
        },
    )
    .await;

    let session = inspect.session(&session_id).expect("session is registered");
    let session = session.lock().unwrap();
    let responses: Vec<String> = session
        .ledger()
        .log(&format!("{session_id}#1"))
        .iter()
        .filter(|s| s.kind == StepKind::LlmResponse)
        .map(|s| {
            serde_json::from_str::<TurnResponse>(&s.payload)
                .expect("an LlmResponse payload is a TurnResponse")
                .message
                .text()
        })
        .collect();

    let chunks = observed.chunks();
    assert_eq!(chunks.len(), responses.len());
    for chunk in &chunks {
        assert!(
            responses.contains(chunk),
            "chunk {chunk:?} has no LlmResponse step behind it"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a6_two_prompts_in_one_session_produce_two_verifiable_runs() {
    let observed = Observed::default();
    let agent = SkeinAgent::new(factory(
        vec![finishes("all done")],
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
        read_only("read_file"),
        Vec::new(),
    ));

    let inspect = agent.clone();
    let session_id = with_facade(
        agent,
        Answer::Allow,
        observed.clone(),
        async |cx: ConnectionTo<Agent>| {
            let session_id = open_session(&cx).await?;
            cx.send_request(prompt(&session_id, "first"))
                .block_task()
                .await?;
            cx.send_request(prompt(&session_id, "second"))
                .block_task()
                .await?;
            Ok(session_id)
        },
    )
    .await;

    let session = inspect.session(&session_id).expect("session is registered");
    let session = session.lock().unwrap();
    for n in 1..=2 {
        let run_id = format!("{session_id}#{n}");
        session
            .ledger()
            .verify_chain(&run_id)
            .unwrap_or_else(|e| panic!("run {run_id} verifies: {e}"));
        let exits = session
            .ledger()
            .log(&run_id)
            .iter()
            .filter(|s| s.kind == StepKind::Exit)
            .count();
        assert_eq!(exits, 1, "run {run_id} has exactly one Exit step");
    }
}

// ---------------------------------------------------------------------------
// Cancellation.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a7_session_cancel_ends_the_run_and_reports_cancelled() {
    let model_calls = Arc::new(AtomicUsize::new(0));
    let observed = Observed::default();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (gate_tx, gate_rx) = std::sync::mpsc::channel();

    let script = vec![
        asks_for("read_file"),
        asks_for("read_file"),
        asks_for("read_file"),
        finishes("never reached"),
    ];
    let calls = model_calls.clone();
    let mut once = Some((started_tx, gate_rx));
    let agent = SkeinAgent::new(move || {
        let (started, gate) = once.take().expect("one session only");
        Ok(SessionParts {
            client: ScriptedModel {
                script: script.clone(),
                calls: calls.clone(),
                gate: Some(gate),
                started: Some(started),
            },
            probe: StaticProbe(true),
            transport: CountingTransport {
                calls: Arc::new(AtomicUsize::new(0)),
                content: "file contents".into(),
            },
            policy: ToolPolicy::new(read_only("read_file"), Vec::new()),
            redactor: Redactor::new(Vec::new()),
            budget: LoopBudget::new(8, 10_000, 8),
            ledger: Ledger::new(),
        })
    });

    let inspect = agent.clone();
    let started_rx = Arc::new(Mutex::new(started_rx));
    let (session_id, stop) = with_facade(
        agent,
        Answer::Allow,
        observed.clone(),
        async |cx: ConnectionTo<Agent>| {
            let session_id = open_session(&cx).await?;
            let sent = cx.send_request(prompt(&session_id, "go"));

            // The first turn is in flight and blocked: cancel now, then let it finish.
            let waiter = started_rx.clone();
            tokio::task::spawn_blocking(move || waiter.lock().unwrap().recv())
                .await
                .expect("join")
                .expect("the first turn started");
            cx.send_notification(agent_client_protocol::schema::v1::CancelNotification::new(
                session_id.clone(),
            ))?;
            for _ in 0..4 {
                let _ = gate_tx.send(());
            }

            let response = sent.block_task().await?;
            Ok((session_id, response.stop_reason))
        },
    )
    .await;

    assert_eq!(stop, StopReason::Cancelled);
    assert!(
        model_calls.load(Ordering::SeqCst) < 4,
        "the run stopped before the script ran out"
    );

    let session = inspect.session(&session_id).expect("session is registered");
    let session = session.lock().unwrap();
    session
        .ledger()
        .verify_chain(&format!("{session_id}#1"))
        .expect("a cancelled run still leaves a verifiable chain");
}

// ---------------------------------------------------------------------------
// Unit level.
// ---------------------------------------------------------------------------

/// Drives `AcpPermissionTransport::call` directly against a real ACP client
/// that answers with `outcome`.
async fn ask_permission(
    outcome: PermissionOutcome,
    tool_calls: Arc<AtomicUsize>,
) -> Result<ToolOutcome> {
    let (agent_side, client_side) = tokio::io::duplex(65536);
    let (agent_read, agent_write) = tokio::io::split(agent_side);
    let (client_read, client_write) = tokio::io::split(client_side);

    let client = tokio::spawn(async move {
        Client
            .builder()
            .on_receive_request(
                async move |request: RequestPermissionRequest, responder, _cx| {
                    let response = match outcome {
                        PermissionOutcome::Cancelled => {
                            RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)
                        }
                        PermissionOutcome::Selected(kind) => {
                            let option = request
                                .options
                                .iter()
                                .find(|o| o.kind == kind)
                                .expect("the option kind is offered");
                            RequestPermissionResponse::new(RequestPermissionOutcome::Selected(
                                SelectedPermissionOutcome::new(option.option_id.clone()),
                            ))
                        }
                    };
                    responder.respond(response)
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_to(ByteStreams::new(
                client_write.compat_write(),
                client_read.compat(),
            ))
            .await
    });

    let result = Agent
        .builder()
        .connect_with(
            ByteStreams::new(agent_write.compat_write(), agent_read.compat()),
            async |cx: ConnectionTo<Client>| {
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let mut transport = skein_acp::AcpPermissionTransport::new(
                        CountingTransport {
                            calls: tool_calls,
                            content: "file contents".into(),
                        },
                        cx,
                        SessionId::new("unit"),
                    );
                    let _ =
                        tx.send(transport.call(&ToolCall::new("read_file", serde_json::json!({}))));
                });
                Ok(
                    tokio::task::spawn_blocking(move || rx.recv().expect("answered"))
                        .await
                        .expect("join"),
                )
            },
        )
        .await
        .expect("the agent side ran");

    client.abort();
    result
}

#[derive(Clone, Copy)]
enum PermissionOutcome {
    Selected(PermissionOptionKind),
    Cancelled,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p1_an_allow_answer_reaches_the_inner_transport() {
    let calls = Arc::new(AtomicUsize::new(0));
    let outcome = ask_permission(
        PermissionOutcome::Selected(PermissionOptionKind::AllowOnce),
        calls.clone(),
    )
    .await
    .expect("the call was allowed");
    assert_eq!(outcome.content, "file contents");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p2_a_reject_answer_denies_without_reaching_the_transport() {
    let calls = Arc::new(AtomicUsize::new(0));
    let error = ask_permission(
        PermissionOutcome::Selected(PermissionOptionKind::RejectOnce),
        calls.clone(),
    )
    .await
    .expect_err("the call was declined");
    assert!(
        matches!(&error, skein_core::SkeinError::ToolDenied { tool, .. } if tool == "read_file"),
        "expected ToolDenied, got {error:?}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p3_a_cancelled_answer_denies_without_reaching_the_transport() {
    let calls = Arc::new(AtomicUsize::new(0));
    let error = ask_permission(PermissionOutcome::Cancelled, calls.clone())
        .await
        .expect_err("the request was cancelled");
    assert!(
        matches!(&error, skein_core::SkeinError::ToolDenied { tool, .. } if tool == "read_file"),
        "expected ToolDenied, got {error:?}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn x1_cancellable_model_stops_delegating_once_the_flag_is_set() {
    let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut model = CancellableModel::new(
        ScriptedModel {
            script: vec![finishes("all done")],
            calls: calls.clone(),
            gate: None,
            started: None,
        },
        cancelled.clone(),
    );
    let req = TurnRequest {
        run_id: "r#1".into(),
        messages: vec![Message::user_text("go")],
        tools: Vec::new(),
    };

    assert_eq!(
        model.turn(&req).expect("delegates").message.text(),
        "all done"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    cancelled.store(true, Ordering::SeqCst);
    let error = model.turn(&req).expect_err("refuses once cancelled");
    assert!(matches!(error, skein_core::SkeinError::Model(_)));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the inner client is not reached"
    );
}

#[test]
fn u1_project_updates_maps_each_ledger_step_kind() {
    let mut ledger = Ledger::new();
    let run_id = "s#1";
    ledger
        .append(run_id, StepKind::IterationBoundary, "1")
        .unwrap();
    ledger
        .append(
            run_id,
            StepKind::LlmResponse,
            serde_json::to_string(&finishes("hello")).unwrap(),
        )
        .unwrap();
    ledger
        .append(
            run_id,
            StepKind::ToolCall,
            serde_json::to_string(&ToolCall::new("read_file", serde_json::json!({}))).unwrap(),
        )
        .unwrap();
    ledger
        .append(
            run_id,
            StepKind::Approval,
            serde_json::json!({"tool": "read_file", "decision": "allowed", "reason": "allowed, read-only"})
                .to_string(),
        )
        .unwrap();
    ledger
        .append(
            run_id,
            StepKind::ToolResult,
            serde_json::to_string(&CapturedResult {
                tool: "read_file".into(),
                content: "token ***".into(),
            })
            .unwrap(),
        )
        .unwrap();
    ledger
        .append(run_id, StepKind::Exit, "FinalOutput")
        .unwrap();

    let updates = project_updates(&ledger, run_id);
    assert_eq!(updates.len(), 4, "boundary and exit steps emit nothing");

    match &updates[0] {
        SessionUpdate::AgentMessageChunk(chunk) => match &chunk.content {
            ContentBlock::Text(text) => assert_eq!(text.text, "hello"),
            other => panic!("expected text, got {other:?}"),
        },
        other => panic!("expected AgentMessageChunk, got {other:?}"),
    }
    let tool_call_id = match &updates[1] {
        SessionUpdate::ToolCall(call) => {
            assert_eq!(call.title, "read_file");
            call.tool_call_id.clone()
        }
        other => panic!("expected ToolCall, got {other:?}"),
    };
    match &updates[2] {
        SessionUpdate::ToolCallUpdate(update) => {
            assert_eq!(update.tool_call_id, tool_call_id);
            assert_eq!(update.fields.status, Some(ToolCallStatus::Pending));
        }
        other => panic!("expected ToolCallUpdate, got {other:?}"),
    }
    match &updates[3] {
        SessionUpdate::ToolCallUpdate(update) => {
            assert_eq!(update.tool_call_id, tool_call_id);
            assert_eq!(update.fields.status, Some(ToolCallStatus::Completed));
            let content = update.fields.content.as_ref().expect("captured content");
            let rendered = serde_json::to_string(content).unwrap();
            assert!(
                rendered.contains("token ***"),
                "the redacted capture stays redacted: {rendered}"
            );
        }
        other => panic!("expected ToolCallUpdate, got {other:?}"),
    }
}

/// A catalogue and nothing else. Separate from `CountingTransport` so the three
/// permission tests above stay this slice's controls.
struct CataloguedTransport(Vec<ToolSpec>);

impl ToolTransport for CataloguedTransport {
    fn call(&mut self, _call: &ToolCall) -> Result<ToolOutcome> {
        panic!("this test never calls a tool")
    }

    fn list(&mut self) -> Result<Vec<ToolSpec>> {
        Ok(self.0.clone())
    }
}

/// Drives `AcpPermissionTransport::list` directly against a real ACP client
/// that counts anything it is asked — so the test can assert that enumerating a
/// catalogue asks the human nothing.
async fn list_through_permission(asked: Arc<AtomicUsize>) -> Result<Vec<ToolSpec>> {
    let (agent_side, client_side) = tokio::io::duplex(65536);
    let (agent_read, agent_write) = tokio::io::split(agent_side);
    let (client_read, client_write) = tokio::io::split(client_side);

    let counter = asked.clone();
    let client = tokio::spawn(async move {
        Client
            .builder()
            .on_receive_request(
                async move |_request: RequestPermissionRequest, responder, _cx| {
                    counter.fetch_add(1, Ordering::SeqCst);
                    responder.respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Cancelled,
                    ))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_to(ByteStreams::new(
                client_write.compat_write(),
                client_read.compat(),
            ))
            .await
    });

    let result = Agent
        .builder()
        .connect_with(
            ByteStreams::new(agent_write.compat_write(), agent_read.compat()),
            async |cx: ConnectionTo<Client>| {
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let mut transport = skein_acp::AcpPermissionTransport::new(
                        CataloguedTransport(vec![
                            ToolSpec::new("fs_read", "read a file", serde_json::json!({})),
                            ToolSpec::new("fs_list", "list a directory", serde_json::json!({})),
                        ]),
                        cx,
                        SessionId::new("unit"),
                    );
                    let _ = tx.send(transport.list());
                });
                Ok(
                    tokio::task::spawn_blocking(move || rx.recv().expect("answered"))
                        .await
                        .expect("join"),
                )
            },
        )
        .await
        .expect("the agent side ran");

    client.abort();
    result
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn p4_the_permission_decorator_forwards_the_catalogue_it_wraps() {
    // The slice's highest-risk line, and the reason it gets a test of its own.
    // `ToolTransport::list` is defaulted to an empty catalogue, so a decorator
    // that forgot to override it would leave `skein acp-agent` silently
    // advertising nothing while `skein chat` worked — and nothing would fail to
    // compile.
    let asked = Arc::new(AtomicUsize::new(0));

    let advertised = list_through_permission(asked.clone())
        .await
        .expect("enumerating a catalogue is not a governed call");

    assert_eq!(
        advertised
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["fs_read", "fs_list"],
        "the inner transport's catalogue, in its own order"
    );
    assert_eq!(
        asked.load(Ordering::SeqCst),
        0,
        "permission is asked per call; enumerating what exists is not a call"
    );
}
