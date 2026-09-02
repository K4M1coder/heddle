//! The governed Tool Gateway (design §4.3/§4.11, Constitution IV/V/VI): the one
//! path from an agent's intent to an external tool. Policy decides, the Ledger
//! records, secrets are scrubbed before the record — and the core reaches the
//! outside world only through the `ToolTransport` port, never a named protocol.

use crate::error::{Result, SkeinError};
use crate::ledger::{Ledger, StepKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One tool invocation as the caller means it: raw arguments, secrets and all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool: String,
    pub args: Value,
}

impl ToolCall {
    pub fn new(tool: impl Into<String>, args: Value) -> Self {
        ToolCall {
            tool: tool.into(),
            args,
        }
    }
}

/// What a tool returned, serialized by the transport. Raw: it is handed back to
/// the trusted caller, and redacted only on its way into the Ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolOutcome {
    pub content: String,
}

/// Synchronous, mirroring [`crate::model::ModelClient`]: a protocol-backed
/// transport owns its async runtime internally and blocks behind this boundary.
pub trait ToolTransport {
    fn call(&mut self, call: &ToolCall) -> Result<ToolOutcome>;
}

/// What the governor decided about one call, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow { reason: String },
    Deny { reason: String },
}

/// Which tools may run. Mutability is configuration, not discovery: deriving it
/// from a server's tool annotations is a later slice.
pub struct ToolPolicy {
    mutating: Vec<String>,
    approved: Vec<String>,
}

impl ToolPolicy {
    pub fn new(mutating: Vec<String>, approved: Vec<String>) -> Self {
        ToolPolicy { mutating, approved }
    }

    /// A mutating tool runs only with an explicit approval; anything else is
    /// treated as read-only.
    pub fn decide(&self, tool: &str) -> Decision {
        if !self.mutating.iter().any(|m| m == tool) {
            return Decision::Allow {
                reason: "not mutating".into(),
            };
        }
        if self.approved.iter().any(|a| a == tool) {
            Decision::Allow {
                reason: "approved".into(),
            }
        } else {
            Decision::Deny {
                reason: "mutating tool requires approval".into(),
            }
        }
    }
}

/// Scrubs known secret values out of anything on its way into the Ledger
/// (Constitution VI: a secret is never in the record by value). The values are
/// configuration today; they will come from `SecretProvider::resolve`
/// (design §7.13) once that lands.
pub struct Redactor {
    secrets: Vec<String>,
}

impl Redactor {
    pub fn new(secrets: Vec<String>) -> Self {
        Redactor {
            secrets: secrets.into_iter().filter(|s| !s.is_empty()).collect(),
        }
    }

    pub fn redact(&self, text: &str) -> String {
        let mut out = text.to_string();
        for secret in &self.secrets {
            out = out.replace(secret, "***");
        }
        out
    }

    /// Redacts the strings inside a JSON value, leaving its shape intact — so
    /// the captured payload stays parseable for replay.
    fn redact_value(&self, value: &Value) -> Value {
        match value {
            Value::String(s) => Value::String(self.redact(s)),
            Value::Array(items) => {
                Value::Array(items.iter().map(|v| self.redact_value(v)).collect())
            }
            Value::Object(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(k, v)| (self.redact(k), self.redact_value(v)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
}

/// The `Approval` step's payload: what was asked, what was decided, and why.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApprovalRecord {
    tool: String,
    decision: String,
    reason: String,
}

/// The `ToolResult` step's payload — the redacted record of one executed call,
/// and everything [`replay_tool_calls`] hands back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapturedResult {
    pub tool: String,
    pub content: String,
}

/// Mediates every tool call: record the attempt, decide, and only then — if at
/// all — reach the transport. The transport is public so a caller can inspect
/// the one it injected, as with [`crate::native_loop::NativeLoop`].
pub struct ToolGateway<T: ToolTransport> {
    pub transport: T,
    policy: ToolPolicy,
    redactor: Redactor,
}

impl<T: ToolTransport> ToolGateway<T> {
    pub fn new(transport: T, policy: ToolPolicy, redactor: Redactor) -> Self {
        ToolGateway {
            transport,
            policy,
            redactor,
        }
    }

    /// The governed path. The Ledger is borrowed, not owned: the caller inspects
    /// it afterwards, and the gateway's steps land in the same chain as the rest
    /// of the run.
    pub fn call(
        &mut self,
        run_id: &str,
        call: &ToolCall,
        ledger: &mut Ledger,
    ) -> Result<ToolOutcome> {
        // Recorded before the decision, so a refused attempt still names itself.
        let attempt = ToolCall {
            tool: call.tool.clone(),
            args: self.redactor.redact_value(&call.args),
        };
        ledger.append(run_id, StepKind::ToolCall, serde_json::to_string(&attempt)?);

        let decision = self.policy.decide(&call.tool);
        let (verdict, reason) = match &decision {
            Decision::Allow { reason } => ("allowed", reason.clone()),
            Decision::Deny { reason } => ("denied", reason.clone()),
        };
        let record = ApprovalRecord {
            tool: call.tool.clone(),
            decision: verdict.to_string(),
            reason: reason.clone(),
        };
        ledger.append(run_id, StepKind::Approval, serde_json::to_string(&record)?);

        if let Decision::Deny { .. } = decision {
            return Err(SkeinError::ToolDenied {
                tool: call.tool.clone(),
                reason,
            });
        }

        // The tool needs the real secret; only the record must not have it.
        let outcome = self.transport.call(call)?;

        let captured = CapturedResult {
            tool: call.tool.clone(),
            content: self.redactor.redact(&outcome.content),
        };
        ledger.append(
            run_id,
            StepKind::ToolResult,
            serde_json::to_string(&captured)?,
        );
        Ok(outcome)
    }
}

/// Reconstructs a run's captured tool results from the Ledger alone. It takes no
/// transport, so re-invoking a downstream tool is not merely avoided here — it is
/// unrepresentable.
pub fn replay_tool_calls(ledger: &Ledger, run_id: &str) -> Result<Vec<CapturedResult>> {
    ledger
        .log(run_id)
        .iter()
        .filter(|s| s.kind == StepKind::ToolResult)
        .map(|s| serde_json::from_str(&s.payload).map_err(SkeinError::from))
        .collect()
}
