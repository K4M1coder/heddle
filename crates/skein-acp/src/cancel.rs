//! A `ModelClient` decorator that refuses once the session has been cancelled.
//!
//! `NativeLoop::run` propagates a provider error straight out, so a refusal here
//! ends the run at the next turn boundary with the chain intact — the same path
//! `provider_error_leaves_the_chain_verifiable` already covers.
//!
//! This is the half of cancellation that applies to a turn which has not
//! started. The other half is [`AcpTextSink::wants_more`], which the model's own
//! producer asks *during* a turn: the same flag, read from the streaming side,
//! so a cancellation arriving mid-answer does not wait for that answer to
//! finish.
//!
//! [`AcpTextSink::wants_more`]: crate::AcpTextSink

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
