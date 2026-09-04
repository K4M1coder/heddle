//! An ACP facade over the native Skein loop (ADR-0003 decision 2, ADR-0004 D3).
//!
//! This is the only crate in the product that names the Agent Client Protocol.
//! `skein-core` reaches the outside world through the ports it defines and never
//! depends on this crate, exactly as `skein-mcp` relates to MCP.
//!
//! The core is synchronous and ACP is async. The bridge is a plain OS thread:
//! `session/prompt` moves its `Responder` into one and returns immediately, so
//! the connection's single dispatch task stays free to deliver the permission
//! answers the loop thread blocks on. See [`AcpPermissionTransport`].

pub mod cancel;
pub mod permission;
pub mod stream;

pub use cancel::CancellableModel;
pub use permission::AcpPermissionTransport;
pub use stream::AcpTextSink;

use agent_client_protocol::schema::v1::{
    AgentCapabilities, CancelNotification, Content as ToolContent, ContentBlock, ContentChunk,
    InitializeRequest, InitializeResponse, NewSessionRequest, NewSessionResponse, PromptRequest,
    PromptResponse, SessionId, SessionNotification, SessionUpdate, StopReason, TextContent,
    ToolCall as AcpToolCall, ToolCallContent, ToolCallId, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields, ToolKind,
};
use agent_client_protocol::{Agent, Client, ConnectTo, ConnectionTo, Error, Stdio};
use skein_core::{
    CapturedResult, Exit, Ledger, LoopBudget, LoopController, Message, ModelClient, ProgressProbe,
    Redactor, Result, StepKind, ToolCall, ToolGateway, ToolPolicy, ToolTransport, TurnResponse,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// The collaborators one ACP session runs with. The operator supplies the
/// undecorated ports; the facade wraps them in [`CancellableModel`] and
/// [`AcpPermissionTransport`] itself, so neither gate can be skipped by a caller.
///
/// The `Ledger` is injected like everything else, so a session can be given a
/// silo-backed chain (`skein_silo::Silo::ledger`) instead of an in-memory one.
pub struct SessionParts<C, P, T> {
    pub client: C,
    pub probe: P,
    pub transport: T,
    pub policy: ToolPolicy,
    pub redactor: Redactor,
    pub budget: LoopBudget,
    pub ledger: Ledger,
}

/// One ACP session: a governed loop, its chain, and its prompt counter.
///
/// A session owns N Skein runs, one per `session/prompt`, with run ids
/// `{session_id}#{n}`. Reusing the session id as the run id would put several
/// `Exit` steps in one chain and redefine what `verify_chain` verifies.
pub struct SkeinSession<C: ModelClient, P: ProgressProbe, T: ToolTransport> {
    id: SessionId,
    engine: NativeLoop<C, P, T>,
    ledger: Ledger,
    budget: LoopBudget,
    prompts: u32,
    cancelled: Arc<AtomicBool>,
    /// How many deltas this session's model pushed to the client during the
    /// run that just ended. Non-zero means the transcript is already live, and
    /// the chain-derived projection must not repeat it.
    streamed: Arc<AtomicU64>,
}

type NativeLoop<C, P, T> =
    skein_core::NativeLoop<CancellableModel<C>, P, AcpPermissionTransport<T>>;

impl<C: ModelClient, P: ProgressProbe, T: ToolTransport> SkeinSession<C, P, T> {
    fn new(id: SessionId, parts: SessionParts<C, P, T>, connection: ConnectionTo<Client>) -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        let streamed = Arc::new(AtomicU64::new(0));
        // Installed here because this is the one place holding both the
        // connection and the session id, and installed *through*
        // `CancellableModel` so the decorator's forward is the path in use
        // rather than a claim about one. A session cannot be built without a
        // sink.
        let mut client = CancellableModel::new(parts.client, cancelled.clone());
        client.set_text_sink(Box::new(AcpTextSink::new(
            connection.clone(),
            id.clone(),
            parts.redactor.clone(),
            streamed.clone(),
            cancelled.clone(),
        )));
        // Cloned rather than split: the gateway and the loop both write into
        // this session's one chain, so they must scrub the one secret set the
        // operator configured.
        let gateway = ToolGateway::new(
            AcpPermissionTransport::new(parts.transport, connection, id.clone()),
            parts.policy,
            parts.redactor.clone(),
        );
        SkeinSession {
            id,
            engine: skein_core::NativeLoop::new(client, parts.probe, gateway, parts.redactor),
            ledger: parts.ledger,
            budget: parts.budget,
            prompts: 0,
            cancelled,
            streamed,
        }
    }

    /// Whether the run that just ended delivered its text live. The prompt
    /// handler asks before projecting the chain, so a client is never told the
    /// same words twice.
    pub fn streamed(&self) -> bool {
        self.streamed.load(Ordering::SeqCst) > 0
    }

    /// The chain every run of this session appended to, for inspection and
    /// `verify_chain`.
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// Runs one prompt to completion and returns its run id and stop reason.
    fn run(&mut self, prompt: Message) -> Result<(String, StopReason)> {
        // A cancellation applies to the turn it arrived during, and a delta
        // count to the run it was produced by; neither carries into the next.
        self.cancelled.store(false, Ordering::SeqCst);
        self.streamed.store(0, Ordering::SeqCst);
        self.prompts += 1;
        let run_id = format!("{}#{}", self.id, self.prompts);

        let mut ctl = LoopController::new(self.budget.clone());
        let outcome = self.engine.run(&run_id, prompt, &mut self.ledger, &mut ctl);

        if self.cancelled.load(Ordering::SeqCst) {
            return Ok((run_id, StopReason::Cancelled));
        }
        Ok((run_id, stop_reason(&outcome?.exit)))
    }
}

fn stop_reason(exit: &Exit) -> StopReason {
    match exit {
        Exit::FinalOutput => StopReason::EndTurn,
        Exit::MaxTokens => StopReason::MaxTokens,
        Exit::MaxIters => StopReason::MaxTurnRequests,
        // An engine-forced stall stop is not a success; EndTurn would claim one.
        Exit::NoProgress | Exit::HumanReject => StopReason::Refusal,
    }
}

/// What the ACP client is told about a run, derived from the run's own chain.
///
/// There is no second event record: every update below is computed from a
/// Ledger step, which is why "a view, not a record" is structural rather than a
/// promise (Constitution V). The ACP tool-call id *is* the chain id of the
/// `ToolCall` step, so a client's correlation key is the chain's own identity.
pub fn project_updates(ledger: &Ledger, run_id: &str) -> Vec<SessionUpdate> {
    let mut updates = Vec::new();
    let mut current: Option<ToolCallId> = None;

    for step in ledger.log(run_id) {
        match step.kind {
            StepKind::LlmResponse => {
                let Ok(response) = serde_json::from_str::<TurnResponse>(&step.payload) else {
                    continue;
                };
                updates.push(SessionUpdate::AgentMessageChunk(ContentChunk::new(
                    ContentBlock::Text(TextContent::new(response.message.text())),
                )));
            }
            StepKind::ToolCall => {
                let Ok(call) = serde_json::from_str::<ToolCall>(&step.payload) else {
                    continue;
                };
                let id = ToolCallId::new(step.id.clone());
                current = Some(id.clone());
                updates.push(SessionUpdate::ToolCall(
                    // The name only, and it comes off the chain, so it is
                    // redacted for the same reason the content below is. The
                    // arguments are the model's raw text and stay off the
                    // transcript.
                    AcpToolCall::new(id, call.tool).kind(ToolKind::Other),
                ));
            }
            StepKind::Approval => {
                let (Some(id), Ok(record)) = (
                    current.clone(),
                    serde_json::from_str::<serde_json::Value>(&step.payload),
                ) else {
                    continue;
                };
                let status = match record.get("decision").and_then(|d| d.as_str()) {
                    Some("allowed") => ToolCallStatus::Pending,
                    _ => ToolCallStatus::Failed,
                };
                updates.push(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                    id,
                    ToolCallUpdateFields::new().status(status),
                )));
            }
            StepKind::ToolResult => {
                let (Some(id), Ok(captured)) = (
                    current.clone(),
                    serde_json::from_str::<CapturedResult>(&step.payload),
                ) else {
                    continue;
                };
                updates.push(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                    id,
                    ToolCallUpdateFields::new()
                        .status(ToolCallStatus::Completed)
                        // Straight from the chain, so it is redacted for the
                        // same reason the chain is.
                        .content(vec![ToolCallContent::Content(ToolContent::new(
                            ContentBlock::Text(TextContent::new(captured.content)),
                        ))]),
                )));
            }
            _ => {}
        }
    }
    updates
}

struct Registered<C: ModelClient, P: ProgressProbe, T: ToolTransport> {
    session: Arc<Mutex<SkeinSession<C, P, T>>>,
    /// Held beside the session, not inside it: `session/cancel` must be
    /// answerable while the loop thread holds the session's lock.
    cancelled: Arc<AtomicBool>,
}

impl<C: ModelClient, P: ProgressProbe, T: ToolTransport> Clone for Registered<C, P, T> {
    fn clone(&self) -> Self {
        Registered {
            session: self.session.clone(),
            cancelled: self.cancelled.clone(),
        }
    }
}

type Sessions<C, P, T> = Arc<Mutex<HashMap<SessionId, Registered<C, P, T>>>>;

/// Serves the ACP agent side: one `SkeinSession` per `session/new`, built from
/// `factory`. Cloneable, so a caller keeps a handle on the sessions it serves.
pub struct SkeinAgent<C: ModelClient, P: ProgressProbe, T: ToolTransport, F> {
    factory: Arc<Mutex<F>>,
    sessions: Sessions<C, P, T>,
    next_id: Arc<AtomicU64>,
}

impl<C: ModelClient, P: ProgressProbe, T: ToolTransport, F> Clone for SkeinAgent<C, P, T, F> {
    fn clone(&self) -> Self {
        SkeinAgent {
            factory: self.factory.clone(),
            sessions: self.sessions.clone(),
            next_id: self.next_id.clone(),
        }
    }
}

impl<C, P, T, F> SkeinAgent<C, P, T, F>
where
    C: ModelClient + Send + 'static,
    P: ProgressProbe + Send + 'static,
    T: ToolTransport + Send + 'static,
    F: FnMut() -> Result<SessionParts<C, P, T>> + Send + 'static,
{
    pub fn new(factory: F) -> Self {
        SkeinAgent {
            factory: Arc::new(Mutex::new(factory)),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// The session behind an id, for inspecting its chain after a run.
    pub fn session(&self, id: &SessionId) -> Option<Arc<Mutex<SkeinSession<C, P, T>>>> {
        self.sessions
            .lock()
            .expect("sessions lock")
            .get(id)
            .map(|registered| registered.session.clone())
    }

    /// Fallible because the first real factory opens a silo per session. A
    /// panic here would poison the factory mutex and end every other session on
    /// a recoverable disk error; an in-memory fallback would run the session
    /// with nothing persisted.
    fn open(&self, connection: ConnectionTo<Client>) -> Result<SessionId> {
        let id = SessionId::new(format!(
            "skein-{}",
            self.next_id.fetch_add(1, Ordering::SeqCst)
        ));
        let parts = (self.factory.lock().expect("factory lock"))()?;
        let session = SkeinSession::new(id.clone(), parts, connection);
        let cancelled = session.cancelled.clone();
        self.sessions.lock().expect("sessions lock").insert(
            id.clone(),
            Registered {
                session: Arc::new(Mutex::new(session)),
                cancelled,
            },
        );
        Ok(id)
    }

    fn registered(&self, id: &SessionId) -> Option<Registered<C, P, T>> {
        self.sessions
            .lock()
            .expect("sessions lock")
            .get(id)
            .cloned()
    }

    /// Serves this agent on the process's own stdin/stdout until the client
    /// disconnects, blocking the calling thread.
    ///
    /// **stdout is the protocol.** Nothing else in the process may write to it:
    /// one stray byte corrupts the JSON-RPC stream.
    ///
    /// The executor lives here rather than in the caller for the same reason
    /// `skein-mcp`'s `RmcpToolTransport` owns its runtime — so a CLI does not
    /// have to name a protocol crate's types to run a protocol adapter. ACP is
    /// runtime-agnostic and its stdio transport does its blocking work on
    /// `blocking::Unblock`'s own threads, so polling the one connection future
    /// on this thread is a complete runtime for it.
    pub fn serve_stdio(self) -> Result<()> {
        futures::executor::block_on(self.serve(Stdio::new()))
            .map_err(|e| skein_core::SkeinError::Protocol(format!("acp stdio: {e}")))
    }

    /// Runs the ACP agent over `transport` until the connection closes.
    pub async fn serve(
        self,
        transport: impl ConnectTo<Agent> + 'static,
    ) -> std::result::Result<(), Error> {
        let opener = self.clone();
        let prompter = self.clone();
        let canceller = self;

        Agent
            .builder()
            .name("skein")
            .on_receive_request(
                async move |request: InitializeRequest, responder, _cx| {
                    responder.respond(
                        InitializeResponse::new(request.protocol_version)
                            .agent_capabilities(AgentCapabilities::new()),
                    )
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: NewSessionRequest, responder, cx| match opener.open(cx) {
                    Ok(id) => responder.respond(NewSessionResponse::new(id)),
                    Err(error) => responder.respond_with_internal_error(error),
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: PromptRequest, responder, cx: ConnectionTo<Client>| {
                    let Some(registered) = prompter.registered(&request.session_id) else {
                        return responder.respond_with_error(
                            Error::invalid_params().data(serde_json::json!("unknown session")),
                        );
                    };
                    let prompt = match user_message(&request.prompt) {
                        Ok(prompt) => prompt,
                        Err(error) => return responder.respond_with_error(error),
                    };

                    // Returns immediately: the dispatch task must stay free to
                    // deliver the permission answers the loop thread waits on.
                    let session_id = request.session_id.clone();
                    std::thread::spawn(move || {
                        let mut session = registered.session.lock().expect("session lock");
                        let outcome = session.run(prompt);
                        let _ = match outcome {
                            Ok((run_id, stop)) => {
                                // Sent before the response, so a client that has
                                // its answer has also seen the run that produced it.
                                // `project_updates` is deliberately left
                                // alone: it still means "the complete
                                // chain-derived transcript", so a model that
                                // does not stream produces exactly what it
                                // produced before. The filter is here, at the
                                // one call site where the two paths could
                                // collide.
                                let streamed = session.streamed();
                                for update in project_updates(session.ledger(), &run_id) {
                                    if streamed
                                        && matches!(update, SessionUpdate::AgentMessageChunk(_))
                                    {
                                        continue;
                                    }
                                    let _ = cx.send_notification(SessionNotification::new(
                                        session_id.clone(),
                                        update,
                                    ));
                                }
                                responder.respond(PromptResponse::new(stop))
                            }
                            Err(error) => responder.respond_with_internal_error(error),
                        };
                    });
                    Ok(())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_notification(
                async move |notification: CancelNotification, _cx| {
                    if let Some(registered) = canceller.registered(&notification.session_id) {
                        registered.cancelled.store(true, Ordering::SeqCst);
                    }
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .connect_to(transport)
            .await
    }
}

/// ACP prompts are a block list; `skein_core::Content` carries text only, so a
/// non-text block is refused rather than silently dropped.
fn user_message(blocks: &[ContentBlock]) -> std::result::Result<Message, Error> {
    let mut text = String::new();
    for block in blocks {
        match block {
            ContentBlock::Text(chunk) => text.push_str(&chunk.text),
            _ => {
                return Err(Error::invalid_params().data(serde_json::json!(
                    "skein accepts text content blocks only in this version"
                )))
            }
        }
    }
    Ok(Message::user_text(text))
}
