//! The ACP client the desktop app runs on: one child `skein acp-agent`, one
//! session, and three things a window can ask of it.
//!
//! Wiring only, like every `skein` subcommand: the protocol is
//! `agent-client-protocol`'s, the loop and the chain are the child's, and this
//! module adds no capability of its own. `prompt` is `session/prompt`, `cancel`
//! is `session/cancel`, and starting up is `initialize` + `session/new` — the
//! same three calls `crates/skein-cli/tests/cli_acp_agent.rs` makes against the
//! same binary. That is Constitution I as code rather than as a promise.
//!
//! **No Tauri type appears here.** Updates leave through a caller-supplied
//! closure, so `main.rs` can hand it `AppHandle::emit` and a test can hand it a
//! `Vec`. The window is one implementation of the sink, not its definition.
//!
//! The connection lives inside one `connect_with` closure on one OS thread, and
//! every request is issued through `on_receiving_result` rather than awaited:
//! a `cancel` has to be deliverable *while* a prompt is in flight, which it
//! would not be if the closure sat awaiting the prompt's response. That is the
//! same reason `crates/skein-acp/src/permission.rs` registers a callback
//! instead of blocking its dispatch task.

use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock, InitializeRequest, NewSessionRequest, PermissionOptionKind,
    PromptRequest, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionId, SessionNotification, StopReason, TextContent,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{AcpAgent, AcpAgentConfig, Agent, Client, ConnectionTo};
use futures::channel::{mpsc, oneshot};
use futures::StreamExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// How the child `skein acp-agent` is launched: a program, its argv, and the
/// directory `session/new` is told about.
///
/// The flags are the CLI's own (`--root`, `--silo`, `--model`, `--base-url`,
/// `--fs-root`, …). The shell invents none of its own configuration surface;
/// it passes through what `skein acp-agent` already documents.
#[derive(Clone, Debug)]
pub struct AgentLaunch {
    command: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
}

impl AgentLaunch {
    /// A launch of `command` with no arguments, rooted at the current directory.
    pub fn new(command: impl Into<PathBuf>) -> Self {
        AgentLaunch {
            command: command.into(),
            args: Vec::new(),
            cwd: PathBuf::from("."),
        }
    }

    /// Appends command-line arguments for the child.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// The directory `session/new` names as the session's working directory.
    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = cwd.into();
        self
    }

    /// The executable that will be spawned. Named, like `AcpAgentConfig`'s own
    /// accessors, so a caller can assert on the launch instead of on its
    /// `Debug` rendering.
    pub fn command(&self) -> &Path {
        &self.command
    }

    /// The argv the child receives, in order.
    pub fn arguments(&self) -> &[String] {
        &self.args
    }

    /// The directory `session/new` will name.
    pub fn working_dir(&self) -> &Path {
        &self.cwd
    }
}

/// What the connection thread is asked to do. Every variant is one ACP message.
enum Request {
    Prompt {
        text: String,
        reply: oneshot::Sender<Result<StopReason, String>>,
    },
    Cancel,
    /// An `initialize` round trip. Ordered dispatch makes its answer proof that
    /// everything sent before it has already been processed by the child.
    Ping {
        reply: oneshot::Sender<Result<(), String>>,
    },
    Close,
}

struct Inner {
    session_id: String,
    requests: mpsc::UnboundedSender<Request>,
    /// Taken by whichever of `close` or `Drop` runs first, so the thread is
    /// joined exactly once.
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        shut_down(self);
    }
}

/// Ends the connection and waits for its thread. Closing the client's end of
/// the pipe is what stops the child: `skein acp-agent` exits zero when its
/// client disconnects, so there is nothing to kill and nothing to leak.
fn shut_down(inner: &Inner) {
    let _ = inner.requests.unbounded_send(Request::Close);
    inner.requests.close_channel();
    let worker = inner.worker.lock().expect("the worker slot").take();
    if let Some(worker) = worker {
        let _ = worker.join();
    }
}

/// A live ACP session with a child `skein acp-agent`.
///
/// Cloneable, and the child outlives every clone: the session ends when the
/// last handle is dropped.
#[derive(Clone)]
pub struct SessionHandle(Arc<Inner>);

impl SessionHandle {
    /// Spawns `launch`, performs `initialize` + `session/new`, and returns once
    /// the session exists.
    ///
    /// `on_update` is called for every `session/update` notification the child
    /// sends, on the connection thread, in arrival order. `on_exit` is called
    /// exactly once, when the connection ends, with a message a status line can
    /// show — a window whose agent has died must say so rather than keep
    /// accepting messages into a dead pipe.
    pub fn start(
        launch: AgentLaunch,
        // `Sync` as well as `Send`: the protocol crate shares the handler
        // across its dispatch tasks rather than moving it into one.
        on_update: impl Fn(SessionNotification) + Send + Sync + 'static,
        on_exit: impl FnOnce(String) + Send + 'static,
    ) -> Result<SessionHandle, String> {
        let (requests, mut incoming) = mpsc::unbounded::<Request>();
        // Not a `futures` channel: `start` blocks the calling thread on this
        // until the session exists, and a `std` receiver is what that is.
        let (ready, started) = std::sync::mpsc::channel::<Result<String, String>>();

        let worker = std::thread::Builder::new()
            .name("skein-ui-acp".to_string())
            .spawn(move || {
                let cwd = launch.cwd.clone();
                let transport = AcpAgent::new(
                    AcpAgentConfig::new(launch.command.clone()).args(launch.args.clone()),
                );

                let outcome = futures::executor::block_on(
                    Client
                        .builder()
                        .name("skein-ui")
                        .on_receive_notification(
                            async move |notification: SessionNotification, _cx| {
                                on_update(notification);
                                Ok(())
                            },
                            agent_client_protocol::on_receive_notification!(),
                        )
                        .on_receive_request(
                            async move |request: RequestPermissionRequest, responder, _cx| {
                                responder.respond(decline(&request))
                            },
                            agent_client_protocol::on_receive_request!(),
                        )
                        .connect_with(transport, async move |cx: ConnectionTo<Agent>| {
                            let session_id = match handshake(&cx, cwd).await {
                                Ok(id) => {
                                    let _ = ready.send(Ok(id.to_string()));
                                    id
                                }
                                Err(error) => {
                                    let _ = ready.send(Err(error));
                                    return Ok(());
                                }
                            };
                            serve(&cx, &session_id, &mut incoming).await;
                            Ok(())
                        }),
                );

                on_exit(match outcome {
                    Ok(()) => "The agent session ended.".to_string(),
                    Err(error) => format!("The agent session ended: {error}"),
                });
            })
            .map_err(|error| format!("could not start the agent thread: {error}"))?;

        let session_id = match started.recv() {
            Ok(Ok(id)) => id,
            Ok(Err(error)) => return Err(error),
            // The thread ended before it could answer: the child never came up.
            Err(_) => return Err("the agent process did not start".to_string()),
        };

        Ok(SessionHandle(Arc::new(Inner {
            session_id,
            requests,
            worker: Mutex::new(Some(worker)),
        })))
    }

    /// The id the child minted for this session, as it appears on the chain.
    pub fn session_id(&self) -> &str {
        &self.0.session_id
    }

    /// Sends one `session/prompt` and resolves when the child answers it.
    ///
    /// Every `session/update` for the run has already been delivered to
    /// `on_update` by the time this resolves — `skein-acp` sends the batch
    /// before the response, deliberately.
    pub async fn prompt(&self, text: &str) -> Result<StopReason, String> {
        let (reply, answer) = oneshot::channel();
        self.dispatch(Request::Prompt {
            text: text.to_string(),
            reply,
        })?;
        answer.await.map_err(|_| closed())?
    }

    /// Sends one `session/cancel`.
    ///
    /// Takes effect at the **next** turn boundary: a model call already in
    /// flight always completes (`crates/skein-acp/src/cancel.rs`). With no
    /// prompt in flight it is a no-op, not an error.
    pub fn cancel(&self) -> Result<(), String> {
        self.dispatch(Request::Cancel)
    }

    /// A round trip to the child, for a caller that needs to know it is alive —
    /// or that everything sent before this has been processed.
    pub async fn ping(&self) -> Result<(), String> {
        let (reply, answer) = oneshot::channel();
        self.dispatch(Request::Ping { reply })?;
        answer.await.map_err(|_| closed())?
    }

    /// Ends the session now instead of at the last handle's drop.
    pub fn close(&self) {
        shut_down(&self.0);
    }

    fn dispatch(&self, request: Request) -> Result<(), String> {
        self.0
            .requests
            .unbounded_send(request)
            .map_err(|_| closed())
    }
}

fn closed() -> String {
    "the agent session is closed".to_string()
}

/// `initialize` then `session/new`, awaited because nothing else may happen
/// until they are done.
async fn handshake(cx: &ConnectionTo<Agent>, cwd: PathBuf) -> Result<SessionId, String> {
    cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
        .block_task()
        .await
        .map_err(|error| format!("the agent refused initialize: {error}"))?;
    let session = cx
        .send_request(NewSessionRequest::new(cwd))
        .block_task()
        .await
        .map_err(|error| format!("the agent refused session/new: {error}"))?;
    Ok(session.session_id)
}

/// Turns requests into ACP messages until the last handle is gone.
///
/// Nothing here is awaited to completion: each request is dispatched and the
/// loop goes straight back to the channel, which is what keeps `cancel`
/// answerable while a prompt is in flight.
async fn serve(
    cx: &ConnectionTo<Agent>,
    session_id: &SessionId,
    incoming: &mut mpsc::UnboundedReceiver<Request>,
) {
    while let Some(request) = incoming.next().await {
        match request {
            Request::Prompt { text, reply } => {
                let sent = cx
                    .send_request(PromptRequest::new(
                        session_id.clone(),
                        vec![ContentBlock::Text(TextContent::new(text))],
                    ))
                    .on_receiving_result(move |result| {
                        let _ = reply.send(
                            result
                                .map(|response| response.stop_reason)
                                .map_err(|error| format!("the prompt failed: {error}")),
                        );
                        async { Ok(()) }
                    });
                // On failure the callback — and with it the answer channel — was
                // dropped, so the caller already sees a closed session.
                if sent.is_err() {
                    return;
                }
            }
            Request::Cancel => {
                let _ = cx.send_notification(CancelNotification::new(session_id.clone()));
            }
            Request::Ping { reply } => {
                let sent = cx
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .on_receiving_result(move |result| {
                        let _ = reply.send(
                            result
                                .map(|_| ())
                                .map_err(|error| format!("the agent did not answer: {error}")),
                        );
                        async { Ok(()) }
                    });
                if sent.is_err() {
                    return;
                }
            }
            Request::Close => return,
        }
    }
}

/// The answer to every `session/request_permission`, in this slice.
///
/// A permission prompt is a screen this slice does not build, and the two
/// alternatives are worse than declining: allowing would make the UI grant what
/// the operator never approved, and not answering would hang the child's loop
/// thread forever. Declining is the only one of the three that a client is
/// *allowed* to choose unilaterally — a client may narrow what runs, never
/// widen it (Constitution VI). `docs/UI.md` says so in the user's words.
///
/// The option is picked out of the ones the agent offered, by protocol kind,
/// rather than by naming an id `skein-acp` happens to use today.
fn decline(request: &RequestPermissionRequest) -> RequestPermissionResponse {
    let rejection = request
        .options
        .iter()
        .find(|option| {
            matches!(
                option.kind,
                PermissionOptionKind::RejectOnce | PermissionOptionKind::RejectAlways
            )
        })
        .map(|option| {
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                option.option_id.clone(),
            ))
        })
        // No rejection was offered, so there is nothing to select: cancelling
        // the request is the protocol's own way of not granting it.
        .unwrap_or(RequestPermissionOutcome::Cancelled);
    RequestPermissionResponse::new(rejection)
}
