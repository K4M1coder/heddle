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
    Role, SkeinError, StepKind, TextSink, ToolAccess, ToolCall, ToolOutcome, ToolPolicy, ToolSpec,
    ToolTransport, TurnRequest, TurnResponse,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// Long enough that a loaded CI runner never trips it, short enough that a
/// signal which never arrives fails as a failure rather than as a hang.
const OBSERVE_TIMEOUT: Duration = Duration::from_secs(10);

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
    /// Set for the streaming tests: pushed one at a time before the turn
    /// returns, standing in for what a real provider produces mid-turn.
    deltas: Vec<String>,
    /// Set for the mid-stream cancellation test: after the first delta, the
    /// turn waits once for the sink itself to stop wanting text. Waiting on the
    /// real signal rather than on a channel is what lets the test cancel over a
    /// real ACP connection without racing its delivery.
    awaits_stop: bool,
    sink: Option<Box<dyn TextSink>>,
}

impl ScriptedModel {
    /// The plain case: a script and a counter, with nothing gated and nothing
    /// streamed. The two sites that need more spell only what they need, with
    /// `..ScriptedModel::playing(…)`.
    fn playing(script: Vec<TurnResponse>, calls: Arc<AtomicUsize>) -> ScriptedModel {
        ScriptedModel {
            script,
            calls,
            gate: None,
            started: None,
            deltas: Vec::new(),
            awaits_stop: false,
            sink: None,
        }
    }
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
        // Before the return, which is the whole property under test: a client
        // that only learns the text from the value `turn` produces cannot have
        // shown it any earlier than `turn` returning.
        if let Some(sink) = &mut self.sink {
            for (i, delta) in self.deltas.iter().enumerate() {
                // Asked the way `skein-gateway`'s reader asks it — before each
                // piece — and answered as an error rather than as a short
                // answer, for the same reason.
                if !sink.wants_more() {
                    return Err(SkeinError::Model("cancelled mid-stream".into()));
                }
                sink.on_text(delta);
                if self.awaits_stop && i == 0 {
                    // Bounded, so a cancellation that never arrives fails the
                    // test's own assertion rather than hanging the suite.
                    let deadline = std::time::Instant::now() + OBSERVE_TIMEOUT;
                    while sink.wants_more() && std::time::Instant::now() < deadline {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
            }
        }
        Ok(self.script[n.min(self.script.len() - 1)].clone())
    }

    fn set_text_sink(&mut self, sink: Box<dyn TextSink>) {
        self.sink = Some(sink);
    }
}

/// The collaborators the doubles above compose into, and the agent built from
/// them. Named because the two fixtures below otherwise spell four type
/// parameters in full to say one thing.
type ScriptedParts = SessionParts<ScriptedModel, StaticProbe, CountingTransport>;
type ScriptedAgent<F> = SkeinAgent<ScriptedModel, StaticProbe, CountingTransport, F>;

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
    /// Waits until the client has been sent `n` chunks, so a test that acts
    /// *during* a turn acts on delivery rather than on a guess about timing.
    async fn wait_for_chunks(&self, n: usize) {
        let deadline = std::time::Instant::now() + OBSERVE_TIMEOUT;
        while self.chunks().len() < n {
            assert!(
                std::time::Instant::now() < deadline,
                "the client saw {} chunks within {OBSERVE_TIMEOUT:?}, expected {n}",
                self.chunks().len()
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

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
) -> impl FnMut() -> Result<ScriptedParts> + Send + 'static {
    move || {
        Ok(SessionParts {
            client: ScriptedModel::playing(script.clone(), model_calls.clone()),
            probe: StaticProbe(true),
            transport: CountingTransport {
                calls: tool_calls.clone(),
                content: "file contents".into(),
            },
            policy: ToolPolicy::new(allowed.clone(), approved.clone()),
            redactor: Redactor::new(Vec::new()),
            budget: LoopBudget::new(8, 10_000, 8),
            ledger: Ledger::new(),
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
            client: ScriptedModel::playing(
                vec![finishes("all done")],
                Arc::new(AtomicUsize::new(0)),
            ),
            probe: StaticProbe(true),
            transport: CountingTransport {
                calls: Arc::new(AtomicUsize::new(0)),
                content: "unused".into(),
            },
            policy: ToolPolicy::new(Vec::new(), Vec::new()),
            redactor: Redactor::new(Vec::new()),
            budget: LoopBudget::new(8, 10_000, 8),
            ledger: once.take().expect("one session only"),
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
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

/// A session whose model streams `deltas` before finishing with their
/// concatenation — which is what a real provider does, and what makes "the
/// client saw the answer twice" a failure this fixture can express.
fn streaming_agent(
    deltas: Vec<&str>,
    secrets: Vec<String>,
    awaits_stop: bool,
) -> ScriptedAgent<impl FnMut() -> Result<ScriptedParts> + Send + 'static> {
    let deltas: Vec<String> = deltas.into_iter().map(String::from).collect();
    SkeinAgent::new(move || {
        Ok(SessionParts {
            client: ScriptedModel {
                deltas: deltas.clone(),
                awaits_stop,
                ..ScriptedModel::playing(
                    vec![finishes(&deltas.concat())],
                    Arc::new(AtomicUsize::new(0)),
                )
            },
            probe: StaticProbe(true),
            transport: CountingTransport {
                calls: Arc::new(AtomicUsize::new(0)),
                content: "unused".into(),
            },
            policy: ToolPolicy::new(Vec::new(), Vec::new()),
            redactor: Redactor::new(secrets.clone()),
            budget: LoopBudget::new(8, 10_000, 8),
            ledger: Ledger::new(),
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a11_a_streaming_model_reaches_the_client_one_delta_at_a_time() {
    let deltas = vec!["The ", "answer ", "is ", "42."];
    let observed = Observed::default();

    let stop = with_facade(
        streaming_agent(deltas.clone(), Vec::new(), false),
        Answer::Allow,
        observed.clone(),
        async |cx: ConnectionTo<Agent>| {
            let session_id = open_session(&cx).await?;
            let response = cx
                .send_request(prompt(&session_id, "go"))
                .block_task()
                .await?;
            Ok(response.stop_reason)
        },
    )
    .await;

    assert_eq!(stop, StopReason::EndTurn);
    // Equality, not `len() > 1`: a fifth entry holding the whole answer would
    // be the projection re-sending text the client already has, which is what
    // an editor renders as the answer appearing twice.
    assert_eq!(
        observed.chunks(),
        deltas,
        "one chunk per delta, in order, and nothing after them"
    );
}

/// The one secret this session is configured to keep out of its chain.
const SECRET: &str = "sk-SECRET-abc123";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a12_a_secret_in_a_streamed_delta_reaches_the_client_redacted() {
    // The live transcript is a second path out of the process, and it does not
    // go through the chain — so the redaction it needs is its own, applied per
    // delta as the delta is sent.
    let observed = Observed::default();

    with_facade(
        streaming_agent(
            vec!["your key ", SECRET, " is fine"],
            vec![SECRET.into()],
            false,
        ),
        Answer::Allow,
        observed.clone(),
        async |cx: ConnectionTo<Agent>| {
            let session_id = open_session(&cx).await?;
            cx.send_request(prompt(&session_id, "remind me"))
                .block_task()
                .await?;
            Ok(())
        },
    )
    .await;

    // Per-delta equality, not a `contains` over the whole transcript: the
    // scrubbed *concatenation* would satisfy a looser assertion while arriving
    // in one lump after the turn, which is precisely the behaviour this slice
    // replaces. Only the exact three entries prove the redaction happened on
    // the streamed path.
    assert_eq!(observed.chunks(), vec!["your key ", "***", " is fine"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a10_a_secret_is_redacted_from_a_sessions_chain_and_from_the_client_transcript() {
    let observed = Observed::default();
    let agent = SkeinAgent::new(move || {
        Ok(SessionParts {
            client: ScriptedModel::playing(
                vec![finishes(&format!("your key {SECRET} is fine"))],
                Arc::new(AtomicUsize::new(0)),
            ),
            probe: StaticProbe(true),
            transport: CountingTransport {
                calls: Arc::new(AtomicUsize::new(0)),
                content: "unused".into(),
            },
            policy: ToolPolicy::new(Vec::new(), Vec::new()),
            redactor: Redactor::new(vec![SECRET.into()]),
            budget: LoopBudget::new(8, 10_000, 8),
            ledger: Ledger::new(),
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
            client: ScriptedModel::playing(
                vec![finishes("all done")],
                Arc::new(AtomicUsize::new(0)),
            ),
            probe: StaticProbe(true),
            transport: CountingTransport {
                calls: Arc::new(AtomicUsize::new(0)),
                content: "unused".into(),
            },
            policy: ToolPolicy::new(Vec::new(), Vec::new()),
            redactor: Redactor::new(Vec::new()),
            budget: LoopBudget::new(8, 10_000, 8),
            ledger: Ledger::new(),
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
                gate: Some(gate),
                started: Some(started),
                ..ScriptedModel::playing(script.clone(), calls.clone())
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
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a13_a_cancel_arriving_mid_stream_ends_the_turn_and_reports_cancelled() {
    let deltas = vec!["The ", "answer ", "is ", "42."];
    let observed = Observed::default();
    let agent = streaming_agent(deltas, Vec::new(), true);
    let inspect = agent.clone();
    let watching = observed.clone();

    let (session_id, stop) = with_facade(
        agent,
        Answer::Allow,
        observed.clone(),
        async |cx: ConnectionTo<Agent>| {
            let session_id = open_session(&cx).await?;
            let sent = cx.send_request(prompt(&session_id, "go"));
            // Cancelled only once the client has actually been shown text. Any
            // earlier and this would be the pre-turn refusal `x1` already
            // covers, rather than the cancellation of a turn in flight.
            watching.wait_for_chunks(1).await;
            cx.send_notification(agent_client_protocol::schema::v1::CancelNotification::new(
                session_id.clone(),
            ))?;
            let response = sent.block_task().await?;
            Ok((session_id, response.stop_reason))
        },
    )
    .await;

    assert_eq!(stop, StopReason::Cancelled);
    // Equality, not a length bound: it says both that no further delta was
    // pushed *and* that the chain-derived projection did not repeat the one the
    // client already has.
    assert_eq!(
        observed.chunks(),
        vec!["The "],
        "the delta sent before the cancellation, and nothing after it"
    );

    let session = inspect.session(&session_id).expect("session is registered");
    let session = session.lock().unwrap();
    session
        .ledger()
        .verify_chain(&format!("{session_id}#1"))
        .expect("a run cancelled mid-stream still leaves a verifiable chain");
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
        ScriptedModel::playing(vec![finishes("all done")], calls.clone()),
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

/// The flag the **caller** holds is the flag the session obeys.
///
/// `a7_…` proves the notification path end to end; it cannot notice a session
/// that mints its own flag, because `session/cancel` reaches whatever flag
/// `Registered` was given. This one never sends a notification: it sets the
/// `Arc` it put into `SessionParts` and requires the run to end.
///
/// That is the property slice 027 needs, and it is what lets one flag reach a
/// running child process: the tool transport is built by the same caller, from
/// the same `Arc`, long before a session exists.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a14_a_session_obeys_the_cancellation_flag_its_caller_supplied() {
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
    let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let supplied = cancelled.clone();
    let calls = model_calls.clone();
    let mut once = Some((started_tx, gate_rx));
    let agent = SkeinAgent::new(move || {
        let (started, gate) = once.take().expect("one session only");
        Ok(SessionParts {
            client: ScriptedModel {
                gate: Some(gate),
                started: Some(started),
                ..ScriptedModel::playing(script.clone(), calls.clone())
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
            cancelled: supplied.clone(),
        })
    });

    let started_rx = Arc::new(Mutex::new(started_rx));
    let (_session_id, stop) = with_facade(
        agent,
        Answer::Allow,
        observed.clone(),
        async |cx: ConnectionTo<Agent>| {
            let session_id = open_session(&cx).await?;
            let sent = cx.send_request(prompt(&session_id, "go"));

            let waiter = started_rx.clone();
            tokio::task::spawn_blocking(move || waiter.lock().unwrap().recv())
                .await
                .expect("join")
                .expect("the first turn started");
            // No `session/cancel`. The caller's own handle on the run, which is
            // the handle a tool transport is also holding.
            cancelled.store(true, Ordering::SeqCst);
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
}
