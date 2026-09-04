//! A `ModelClient` decorator that refuses once the session has been cancelled.
//!
//! `NativeLoop::run` propagates a provider error straight out, so a refusal here
//! ends the run at the next turn boundary with the chain intact — the same path
//! `provider_error_leaves_the_chain_verifiable` already covers.
//!
//! This is the reader that applies to a turn which has not started. There are
//! three others, all four on the **one** flag `SessionParts` carries:
//! [`AcpTextSink::wants_more`], which the model's own producer asks per line of
//! a stream, so a cancellation arriving mid-answer does not wait for that answer
//! to finish; `skein-sandbox`'s launcher, which polls it while a `proc_run`
//! child is executing, so a cancellation arriving mid-tool does not wait for
//! that tool's timeout; and [`AcpPermissionTransport`], which polls it while a
//! permission request is outstanding, so a cancellation arriving while the
//! question is open does not wait for a person to answer it.
//!
//! The last of those is the only wait among the four with no deadline of its
//! own. The other three are bounded by a turn, a stream and `RUN_TIMEOUT`; a
//! person deciding is bounded by nothing, which is why the flag is that wait's
//! only exit other than the answer itself.
//!
//! `skein-sandbox`'s launcher is why the flag is supplied to a session rather
//! than minted by it: the tool transport is built from the same `Arc`, by the
//! same caller, in the frame before the session exists.
//!
//! [`AcpTextSink::wants_more`]: crate::AcpTextSink
//! [`AcpPermissionTransport`]: crate::AcpPermissionTransport

use skein_core::{
    ModelClient, Result, SkeinError, TextSink, TurnRequest, TurnResponse, WireExchange,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct CancellableModel<C: ModelClient> {
    inner: C,
    cancelled: Arc<AtomicBool>,
}

impl<C: ModelClient> CancellableModel<C> {
    pub fn new(inner: C, cancelled: Arc<AtomicBool>) -> Self {
        CancellableModel { inner, cancelled }
    }
}

impl<C: ModelClient> ModelClient for CancellableModel<C> {
    fn turn(&mut self, req: &TurnRequest) -> Result<TurnResponse> {
        if self.cancelled.load(Ordering::SeqCst) {
            return Err(SkeinError::Model("session cancelled by client".into()));
        }
        self.inner.turn(req)
    }

    /// Forwarded rather than defaulted. The default is the honest answer for a
    /// client with no wire; this one is a decorator over a client that may have
    /// had one, and inheriting the default would drop the exchange without
    /// erroring — a traceability gap with nothing to notice it (Constitution V).
    fn take_wire_exchange(&mut self) -> Option<WireExchange> {
        self.inner.take_wire_exchange()
    }

    /// Forwarded for the same reason, and with a sharper consequence: the
    /// session installs its sink *through* this decorator, so inheriting the
    /// default would swallow it and leave the client waiting for the whole turn
    /// — the exact defect streaming exists to remove, and one nothing would
    /// report.
    fn set_text_sink(&mut self, sink: Box<dyn TextSink>) {
        self.inner.set_text_sink(sink)
    }
}
