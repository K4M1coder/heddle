//! Event-sourced Ledger (design §4.11): append-only, hash-chained steps that
//! capture exact model I/O, tool calls, and state — inspectable & tamper-evident.
//! v0 is in-memory; a durable silo-backed store lands with persistence.

use crate::error::{Result, SkeinError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    LlmRequest,
    LlmResponse,
    ToolCall,
    ToolResult,
    StateChange,
    Reflection,
    IterationBoundary,
    BudgetSpent,
    Exit,
    Approval,
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

#[derive(Default)]
pub struct Ledger {
    steps: Vec<Step>,
}

impl Ledger {
    pub fn new() -> Self {
        Ledger { steps: Vec::new() }
    }

    /// Append a step for `run_id`; returns its content-chained id.
    pub fn append(&mut self, run_id: &str, kind: StepKind, payload: impl Into<String>) -> String {
        let payload = payload.into();
        let parent = self
            .steps
            .iter()
            .rev()
            .find(|s| s.run_id == run_id)
            .map(|s| s.id.clone());
        let seq = self.steps.iter().filter(|s| s.run_id == run_id).count() as u64;
        let id = hash(parent.as_deref(), run_id, seq, &kind, &payload);
        self.steps.push(Step {
            id: id.clone(),
            parent,
            seq,
            run_id: run_id.to_string(),
            kind,
            payload,
        });
        id
    }

    /// All steps of a run, in order.
    pub fn log(&self, run_id: &str) -> Vec<&Step> {
        self.steps.iter().filter(|s| s.run_id == run_id).collect()
    }

    pub fn show(&self, id: &str) -> Result<&Step> {
        self.steps
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| SkeinError::NotFound(format!("step {id}")))
    }

    /// Verify the hash chain of a run: recompute each id and check the parent link.
    /// Returns Err at the first inconsistency (tamper-evidence).
    pub fn verify_chain(&self, run_id: &str) -> Result<()> {
        let mut prev: Option<String> = None;
        for s in self.log(run_id) {
            let expected = hash(prev.as_deref(), &s.run_id, s.seq, &s.kind, &s.payload);
            if s.parent.as_deref() != prev.as_deref() {
                return Err(SkeinError::LedgerIntegrity {
                    seq: s.seq,
                    detail: "parent link mismatch".into(),
                });
            }
            if s.id != expected {
                return Err(SkeinError::LedgerIntegrity {
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
