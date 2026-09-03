//! The second gate: a `ToolTransport` decorator that asks the ACP client
//! before reaching the transport it wraps.
//!
//! It is constructed *inside* [`skein_core::ToolGateway`], so
//! `call_captured` has already consulted [`skein_core::ToolPolicy`] by the time
//! `call` runs. A tool the policy refuses never becomes a permission request:
//! the client can only further restrict, never widen (Constitution VI).

use agent_client_protocol::schema::v1::{
    PermissionOption, PermissionOptionId, PermissionOptionKind, RequestPermissionOutcome,
    RequestPermissionRequest, SessionId, ToolCallId, ToolCallUpdate, ToolCallUpdateFields,
};
use agent_client_protocol::{Client, ConnectionTo};
use skein_core::{Result, SkeinError, ToolCall, ToolOutcome, ToolSpec, ToolTransport};

const ALLOW_ONCE: &str = "skein.allow-once";
const REJECT_ONCE: &str = "skein.reject-once";

pub struct AcpPermissionTransport<T: ToolTransport> {
    inner: T,
    connection: ConnectionTo<Client>,
    session_id: SessionId,
}

impl<T: ToolTransport> AcpPermissionTransport<T> {
    pub fn new(inner: T, connection: ConnectionTo<Client>, session_id: SessionId) -> Self {
        AcpPermissionTransport {
            inner,
            connection,
            session_id,
        }
    }

    /// Blocks this thread until the client answers. Legal because
    /// `send_request` is a synchronous `&self` method and `on_receiving_result`
    /// registers a callback rather than awaiting one: the connection's dispatch
    /// task stays free to deliver the answer.
    fn ask(&self, tool: &str) -> Result<RequestPermissionOutcome> {
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
            .map_err(|e| SkeinError::Tool(format!("acp permission request failed: {e}")))?;

        rx.recv()
            .map_err(|_| SkeinError::Tool("acp connection closed".into()))?
            .map_err(|e| SkeinError::Tool(format!("acp permission request failed: {e}")))
    }
}

impl<T: ToolTransport> ToolTransport for AcpPermissionTransport<T> {
    fn call(&mut self, call: &ToolCall) -> Result<ToolOutcome> {
        let denied = |reason: String| SkeinError::ToolDenied {
            tool: call.tool.clone(),
            reason,
        };
        match self.ask(&call.tool)? {
            RequestPermissionOutcome::Selected(selected)
                if selected.option_id.0.as_ref() == ALLOW_ONCE =>
            {
                self.inner.call(call)
            }
            RequestPermissionOutcome::Selected(selected) => Err(denied(format!(
                "acp client declined permission ({})",
                selected.option_id
            ))),
            _ => Err(denied("acp permission request cancelled".into())),
        }
    }

    /// Overridden rather than inherited, and this is the one line in the
    /// decorator that no compiler protects. `ToolTransport::list` defaults to an
    /// empty catalogue — the safe default everywhere else — so a decorator that
    /// left it alone would make `skein acp-agent` silently advertise nothing
    /// while `skein chat` worked.
    ///
    /// No permission is asked: the client governs each *call*, and enumerating
    /// what exists is not one. Restriction still only ever narrows, because
    /// every advertised tool must survive `call` to run.
    fn list(&mut self) -> Result<Vec<ToolSpec>> {
        self.inner.list()
    }
}
