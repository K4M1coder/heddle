//! Event-sourced Ledger (design §4.11): append-only, hash-chained steps that
//! capture exact model I/O, tool calls, and state — inspectable & tamper-evident.
//! The chain is held in memory and, when a [`LedgerStore`] is supplied, written
//! through to a durable silo backing it — one hash function and one chaining
//! rule for both shapes.

use crate::error::{HeddleError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    LlmRequest,
    /// The literal bytes of one provider round trip, between the request that
    /// caused it and whatever the core made of the answer.
    WireExchange,
    LlmResponse,
    ToolCall,
    ToolResult,
    StateChange,
    Reflection,
    IterationBoundary,
    BudgetSpent,
    Exit,
    Approval,
    /// One workflow node completed. Additive, and additive is the whole point:
    /// no existing variant's meaning changes, and both match sites on this enum
    /// already tolerate a new one — `heddle-acp`'s projection has a catch-all
    /// arm, and `heddle-cli` names a kind through serde rather than a second
    /// mapping. The payload lives in `heddle-workflow`, beside the code that
    /// writes it, exactly as `ApprovalRecord` lives in `tool.rs`.
    WorkflowNode,
}

/// One immutable ledger step. `id` is the content hash chained onto `parent`,
/// so any tampering with an earlier payload breaks every later id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    pub parent: Option<String>,
    pub seq: u64,
    pub run_id: String,
    pub kind: StepKind,
    pub payload: String,
}

fn hash(parent: Option<&str>, run_id: &str, seq: u64, kind: &StepKind, payload: &str) -> String {
    let mut h = Sha256::new();
    h.update(parent.unwrap_or("").as_bytes());
    h.update(run_id.as_bytes());
    h.update(seq.to_le_bytes());
    h.update(serde_json::to_string(kind).unwrap_or_default().as_bytes());
    h.update(payload.as_bytes());
    format!("{:x}", h.finalize())
}

/// A durable backing for the chain (design §4.8: the silo). The core never
/// names a database: a store arrives already opened, so `heddle-core` keeps its
/// dependency list to four crates and Constitution IV holds by construction.
///
/// `Send` because a `Ledger` crosses to a worker thread — `heddle-acp` runs each
/// prompt on one.
pub trait LedgerStore: Send {
    /// Persist one already-hashed step. Must be append-only.
    fn append(&mut self, step: &Step) -> Result<()>;
    /// Every step this store holds, in original append order across all runs.
    fn load(&self) -> Result<Vec<Step>>;
}

#[derive(Default)]
pub struct Ledger {
    /// The read model. Reads never touch the store, so `log`/`show` stay
    /// infallible and borrow-returning whichever shape the chain has.
    steps: Vec<Step>,
    store: Option<Box<dyn LedgerStore>>,
}

impl Ledger {
    /// An in-memory chain with no durable backing.
    pub fn new() -> Self {
        Ledger {
            steps: Vec::new(),
            store: None,
        }
    }

    /// A chain backed by `store`, resuming whatever the store already holds.
    pub fn open(store: Box<dyn LedgerStore>) -> Result<Self> {
        Ok(Ledger {
            steps: store.load()?,
            store: Some(store),
        })
    }

    /// Append a step for `run_id`; returns its content-chained id.
    ///
    /// The step is persisted *before* it is mirrored, and `seq`/`parent` are
    /// derived from the mirror. So a store that refuses leaves the chain exactly
    /// where it was: the next append recomputes the same step rather than
    /// silently skipping a sequence number.
    pub fn append(
        &mut self,
        run_id: &str,
        kind: StepKind,
        payload: impl Into<String>,
    ) -> Result<String> {
        let payload = payload.into();
        let parent = self
            .steps
            .iter()
            .rev()
            .find(|s| s.run_id == run_id)
            .map(|s| s.id.clone());
        let seq = self.steps.iter().filter(|s| s.run_id == run_id).count() as u64;
        let id = hash(parent.as_deref(), run_id, seq, &kind, &payload);
        let step = Step {
            id: id.clone(),
            parent,
            seq,
            run_id: run_id.to_string(),
            kind,
            payload,
        };
        if let Some(store) = self.store.as_mut() {
            store.append(&step)?;
        }
        self.steps.push(step);
        Ok(id)
    }

    /// All steps of a run, in order.
    pub fn log(&self, run_id: &str) -> Vec<&Step> {
        self.steps.iter().filter(|s| s.run_id == run_id).collect()
    }

    /// Every distinct `run_id` in the chain, at the position of its first
    /// append.
    ///
    /// [`Ledger::log`] needs a run id, so without this the CLI — the core's
    /// authoritative client (Constitution I) — could only serve someone who
    /// already knew one. Like `log` and `show`, it reads the mirror and never
    /// touches the store.
    pub fn runs(&self) -> Vec<&str> {
        let mut seen: Vec<&str> = Vec::new();
        for s in &self.steps {
            if !seen.contains(&s.run_id.as_str()) {
                seen.push(&s.run_id);
            }
        }
        seen
    }

    pub fn show(&self, id: &str) -> Result<&Step> {
        self.steps
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| HeddleError::NotFound(format!("step {id}")))
    }

    /// Verify the hash chain of a run: recompute each id and check the parent link.
    /// Returns Err at the first inconsistency (tamper-evidence).
    pub fn verify_chain(&self, run_id: &str) -> Result<()> {
        let mut prev: Option<String> = None;
        for s in self.log(run_id) {
            let expected = hash(prev.as_deref(), &s.run_id, s.seq, &s.kind, &s.payload);
            if s.parent.as_deref() != prev.as_deref() {
                return Err(HeddleError::LedgerIntegrity {
                    seq: s.seq,
                    detail: "parent link mismatch".into(),
                });
            }
            if s.id != expected {
                return Err(HeddleError::LedgerIntegrity {
                    seq: s.seq,
                    detail: "id/payload mismatch".into(),
                });
            }
            prev = Some(s.id.clone());
        }
        Ok(())
    }

    /// Test/inspection aid: mutable access to a payload by seq (to simulate tampering).
    #[doc(hidden)]
    pub fn tamper_payload_for_test(&mut self, run_id: &str, seq: u64, new_payload: &str) {
        if let Some(s) = self
            .steps
            .iter_mut()
            .find(|s| s.run_id == run_id && s.seq == seq)
        {
            s.payload = new_payload.to_string();
        }
    }
}
