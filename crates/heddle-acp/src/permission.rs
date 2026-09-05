//! The second gate: a `ToolTransport` decorator that asks the ACP client
//! before reaching the transport it wraps.
//!
//! It is constructed *inside* [`heddle_core::ToolGateway`], so
//! `call_captured` has already consulted [`heddle_core::ToolPolicy`] by the time
//! `call` runs. A tool the policy refuses never becomes a permission request:
//! the client can only further restrict, never widen (Constitution VI).
//!
//! This is the fourth reader of the session's cancellation flag, and the only
//! wait in the product with no deadline of its own: the others are bounded by a
//! turn, a stream, or `RUN_TIMEOUT`, while this one is bounded by a person
//! deciding. It is therefore the one wait where the flag is the *only* way out
//! other than the answer itself.

use agent_client_protocol::schema::v1::{
    PermissionOption, PermissionOptionId, PermissionOptionKind, RequestPermissionOutcome,
    RequestPermissionRequest, SessionId, ToolCallId, ToolCallUpdate, ToolCallUpdateFields,
};
use agent_client_protocol::{Client, ConnectionTo};
use heddle_core::{HeddleError, Result, ToolCall, ToolOutcome, ToolSpec, ToolTransport};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Arc;
use std::time::Duration;

const ALLOW_ONCE: &str = "heddle.allow-once";
const REJECT_ONCE: &str = "heddle.reject-once";

/// How often the wait for an answer looks at the cancellation flag. The same
/// slice `heddle-sandbox`'s launcher polls its copy of the same flag at, for the
/// same trade: below what a person holding a stop button notices, and one
/// atomic load per slice on a thread whose only other activity is being blocked.
const POLL_SLICE: Duration = Duration::from_millis(50);

pub struct AcpPermissionTransport<T: ToolTransport> {
    inner: T,
    connection: ConnectionTo<Client>,
    session_id: SessionId,
    cancelled: Arc<AtomicBool>,
}

/// How a wait for permission ended.
///
/// `RequestPermissionOutcome` cannot express the second case: it is the
/// vocabulary for what a client *answered*, and its own `Cancelled` variant
/// already means something else — the client withdrawing its own question.
enum Answer {
    Client(RequestPermissionOutcome),
    /// The session was cancelled while the question was open, or before it was
    /// asked. Any answer given later is dropped with the channel.
    SessionCancelled,
}

impl<T: ToolTransport> AcpPermissionTransport<T> {
    pub fn new(
        inner: T,
        connection: ConnectionTo<Client>,
        session_id: SessionId,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        AcpPermissionTransport {
            inner,
            connection,
            session_id,
            cancelled,
        }
    }

    /// Blocks this thread until the client answers, the session is cancelled,
    /// or the connection closes. Legal because `send_request` is a synchronous
    /// `&self` method and `on_receiving_result` registers a callback rather
    /// than awaiting one: the connection's dispatch task stays free to deliver
    /// the answer.
    ///
    /// **There is no deadline, and that is a decision.** A person may take
    /// minutes over one of these questions. A clock on that decision would
    /// refuse tool calls nobody cancelled, and would need a configuration
    /// surface to be defensible.
    fn ask(&self, tool: &str) -> Result<Answer> {
        // Before the request, so a session that is already over does not put a
        // question in front of a person only to withdraw it a poll later. The
        // check inside the loop cannot do this one's job, and vice versa.
        if self.cancelled.load(Ordering::SeqCst) {
            return Ok(Answer::SessionCancelled);
        }

        let (tx, rx) = std::sync::mpsc::channel();
        self.connection
            .send_request(RequestPermissionRequest::new(
                self.session_id.clone(),
                // The tool *name* only. The policy is a name allowlist, so the
                // arguments could not inform the answer — and an out-of-process
                // client's transcript is not governed by the Redactor.
                ToolCallUpdate::new(
                    ToolCallId::new(tool.to_string()),
                    ToolCallUpdateFields::new().title(tool.to_string()),
                ),
                vec![
                    PermissionOption::new(
                        PermissionOptionId::new(ALLOW_ONCE),
                        "Allow once",
                        PermissionOptionKind::AllowOnce,
                    ),
                    PermissionOption::new(
                        PermissionOptionId::new(REJECT_ONCE),
                        "Reject once",
                        PermissionOptionKind::RejectOnce,
                    ),
                ],
            ))
            .on_receiving_result(move |result| {
                let _ = tx.send(result.map(|response| response.outcome));
                async { Ok(()) }
            })
            .map_err(|e| HeddleError::Tool(format!("acp permission request failed: {e}")))?;

        loop {
            if self.cancelled.load(Ordering::SeqCst) {
                return Ok(Answer::SessionCancelled);
            }
            match rx.recv_timeout(POLL_SLICE) {
                // Re-read here, and not only at the top of the loop, because
                // the loop's check alone does not give cancellation priority:
                // an answer landing inside the same poll slice as the
                // cancellation returns `Ok` from `recv_timeout` before the
                // loop can come round again, and the `Allow` would win. The
                // flag being already set means the session ended before this
                // answer was sent, so the answer is not one to act on.
                Ok(_) if self.cancelled.load(Ordering::SeqCst) => {
                    return Ok(Answer::SessionCancelled)
                }
                Ok(answered) => {
                    return answered.map(Answer::Client).map_err(|e| {
                        HeddleError::Tool(format!("acp permission request failed: {e}"))
                    })
                }
                // Nobody has answered yet: the normal case, twenty times a
                // second, for the whole life of the question.
                Err(RecvTimeoutError::Timeout) => continue,
                // The `Sender` went with the connection's callback, so no
                // answer is ever coming.
                //
                // Both arms are written out and there is no `_`, because the
                // two variants mean opposite things and a wildcard silently
                // picks one of two bugs: folded into `Timeout`'s arm, a dead
                // connection spins this thread forever; folded into this one,
                // every permission request in the product is refused 50 ms
                // after it is asked.
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(HeddleError::Tool("acp connection closed".into()))
                }
            }
        }
    }
}

impl<T: ToolTransport> ToolTransport for AcpPermissionTransport<T> {
    fn call(&mut self, call: &ToolCall) -> Result<ToolOutcome> {
        let denied = |reason: String| HeddleError::ToolDenied {
            tool: call.tool.clone(),
            reason,
        };
        match self.ask(&call.tool)? {
            Answer::Client(RequestPermissionOutcome::Selected(selected))
                if selected.option_id.0.as_ref() == ALLOW_ONCE =>
            {
                self.inner.call(call)
            }
            Answer::Client(RequestPermissionOutcome::Selected(selected)) => Err(denied(format!(
                "acp client declined permission ({})",
                selected.option_id
            ))),
            Answer::Client(_) => Err(denied("acp permission request cancelled".into())),
            // Deliberately not the sentence above. That one is a client
            // withdrawing its own question; this one is the session ending
            // while the question was open. They land on the chain as the same
            // shape, and a reader has to be able to tell whose behaviour to go
            // and look at.
            Answer::SessionCancelled => Err(denied(
                "session cancelled while awaiting acp permission".into(),
            )),
        }
    }

    /// Overridden rather than inherited, and this is the one line in the
    /// decorator that no compiler protects. `ToolTransport::list` defaults to an
    /// empty catalogue — the safe default everywhere else — so a decorator that
    /// left it alone would make `heddle acp-agent` silently advertise nothing
    /// while `heddle chat` worked.
    ///
    /// No permission is asked: the client governs each *call*, and enumerating
    /// what exists is not one. Restriction still only ever narrows, because
    /// every advertised tool must survive `call` to run.
    fn list(&mut self) -> Result<Vec<ToolSpec>> {
        self.inner.list()
    }
}
