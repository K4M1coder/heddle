//! A `ModelClient` decorator that refuses once the session has been cancelled.
//!
//! `NativeLoop::run` propagates a provider error straight out, so a refusal here
//! ends the run at the next turn boundary with the chain intact — the same path
//! `provider_error_leaves_the_chain_verifiable` already covers. A model call
//! already in flight completes: cancellation is not mid-turn.

use skein_core::{ModelClient, Result, SkeinError, TurnRequest, TurnResponse};
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
}
