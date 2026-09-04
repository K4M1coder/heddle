//! The live half of a session's transcript: assistant text pushed to the ACP
//! client as the provider produces it, rather than derived from the chain once
//! the run has ended.
//!
//! This is the one update in the facade that is **not** a view of a Ledger step,
//! and the exception is the point: a step exists only after the turn it records,
//! which is exactly the latency being removed. The chain is not weakened by it —
//! the same text lands as `StepKind::LlmResponse` when the turn ends, and
//! [`crate::project_updates`] still derives the complete transcript from the
//! chain alone. What changes is only whether the client had to wait for it.

use agent_client_protocol::schema::v1::{
    ContentBlock, ContentChunk, SessionId, SessionNotification, SessionUpdate, TextContent,
};
use agent_client_protocol::{Client, ConnectionTo};
use skein_core::{Redactor, TextSink};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Sends one `AgentMessageChunk` per delta, scrubbed.
///
/// `emitted` is what tells the session, once the run has ended, that this text
/// has already been delivered — so the chain-derived projection must not send it
/// a second time.
pub struct AcpTextSink {
    connection: ConnectionTo<Client>,
    session_id: SessionId,
    redactor: Redactor,
    emitted: Arc<AtomicU64>,
}

impl AcpTextSink {
    pub fn new(
        connection: ConnectionTo<Client>,
        session_id: SessionId,
        redactor: Redactor,
        emitted: Arc<AtomicU64>,
    ) -> Self {
        AcpTextSink {
            connection,
            session_id,
            redactor,
            emitted,
        }
    }
}

impl TextSink for AcpTextSink {
    /// `redact`, not `redact_wire`: a delta is the model's plain text, not a
    /// serialized JSON body, so there is no escaped form of the secret to match
    /// here — the escaped form only exists once the text is inside a payload,
    /// which is where `NativeLoop` applies `redact_wire` to the chain's copy.
    ///
    /// Known and recorded in `spec.md`: a secret split across two deltas cannot
    /// be matched by a per-delta scrub, so it can reach this transcript. The
    /// chain is unaffected. Buffering deltas to close it would reintroduce the
    /// latency this file exists to remove.
    fn on_text(&mut self, delta: &str) {
        self.emitted.fetch_add(1, Ordering::SeqCst);
        // Dropped rather than propagated, for the reason the prompt handler
        // drops its own: a client that has gone away ends the session on its
        // own terms, and failing the model's turn over an undeliverable
        // notification would turn a disconnect into a governed-run failure.
        let _ = self.connection.send_notification(SessionNotification::new(
            self.session_id.clone(),
            SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new(self.redactor.redact(delta)),
            ))),
        ));
    }
}
