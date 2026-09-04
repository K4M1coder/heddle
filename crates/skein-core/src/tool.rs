//! The governed Tool Gateway (design §4.3/§4.11, Constitution IV/V/VI): the one
//! path from an agent's intent to an external tool. Policy decides, the Ledger
//! records, secrets are scrubbed before the record — and the core reaches the
//! outside world only through the `ToolTransport` port, never a named protocol.

use crate::error::{Result, SkeinError};
use crate::ledger::{Ledger, StepKind};
use crate::secret::{SecretProvider, SecretRef, SecretValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One tool invocation as the caller means it: raw arguments, secrets and all.
///
/// `id` is what a later `Role::Tool` message answers. It is defaulted for the
/// same reason [`crate::model::TurnResponse::tool_calls`] is, and the reason is
/// load-bearing rather than ergonomic: `skein-acp` reconstructs a `ToolCall`
/// out of old `StepKind::ToolCall` payloads inside a `let Ok(…) else
/// { continue }`, so a required field would make every chain recorded before
/// this field existed lose its tool-call updates **without erroring**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    #[serde(default)]
    pub id: String,
    pub tool: String,
    pub args: Value,
}

impl ToolCall {
    /// A call nobody has to answer by id — the shape a trusted caller
    /// constructs directly. Every call that arrives *from a provider* comes
    /// through [`ToolCall::with_id`], because `skein-gateway` supplies an id
    /// even when the provider omits one.
    pub fn new(tool: impl Into<String>, args: Value) -> Self {
        ToolCall {
            id: String::new(),
            tool: tool.into(),
            args,
        }
    }

    pub fn with_id(id: impl Into<String>, tool: impl Into<String>, args: Value) -> Self {
        ToolCall {
            id: id.into(),
            ..ToolCall::new(tool, args)
        }
    }
}

/// One tool as a model is told about it: the name it must use, what the tool is
/// for, and the JSON Schema its arguments have to match.
///
/// `parameters` is an opaque [`Value`] rather than a typed schema. The schema is
/// the server's document — derived from the real parameter type at the far end
/// of the transport — and the core never interprets it, so a typed mirror here
/// would be a second source of truth for something nothing in this crate reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl ToolSpec {
    pub fn new(name: impl Into<String>, description: impl Into<String>, parameters: Value) -> Self {
        ToolSpec {
            name: name.into(),
            description: description.into(),
            parameters,
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

    /// The tools this transport can actually reach — MCP's `tools/list`, named
    /// in the port's own vocabulary. [`ToolGateway::advertise`] filters it.
    ///
    /// Defaulted, where [`crate::native_loop::NativeLoop::new`]'s `Redactor` is
    /// deliberately not, and the difference is the whole justification: **there
    /// the silent default was the unsafe one** — a loop with no redactor records
    /// a conversation in cleartext — **and here it is the safe one.** A
    /// transport that does not override this advertises nothing, which is
    /// deny-by-default (Constitution VI). Overriding it is how a transport opts
    /// a tool *in*.
    fn list(&mut self) -> Result<Vec<ToolSpec>> {
        Ok(Vec::new())
    }
}

/// What the governor decided about one call, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow { reason: String },
    Deny { reason: String },
}

/// How an allowlisted tool is classified. Access is configuration, not
/// discovery: deriving it from a server's tool annotations is a later slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAccess {
    ReadOnly,
    Mutating,
}

/// Which tools may run at all, and how. Deny-by-default (Constitution VI): a
/// name absent from `allowed` never reaches a transport, whatever it is — the
/// tool name is model-chosen, so omission may not mean permission.
pub struct ToolPolicy {
    allowed: Vec<(String, ToolAccess)>,
    approved: Vec<String>,
}

impl ToolPolicy {
    pub fn new(allowed: Vec<(String, ToolAccess)>, approved: Vec<String>) -> Self {
        ToolPolicy { allowed, approved }
    }

    /// Identity first: an unlisted tool is refused whatever its access class.
    /// Within the allowlist, a mutating tool still runs only with an explicit
    /// approval.
    pub fn decide(&self, tool: &str) -> Decision {
        let Some((_, access)) = self.allowed.iter().find(|(name, _)| name == tool) else {
            return Decision::Deny {
                reason: "tool is not in the allowlist".into(),
            };
        };
        match access {
            ToolAccess::ReadOnly => Decision::Allow {
                reason: "allowed, read-only".into(),
            },
            ToolAccess::Mutating if self.approved.iter().any(|a| a == tool) => Decision::Allow {
                reason: "approved".into(),
            },
            ToolAccess::Mutating => Decision::Deny {
                reason: "mutating tool requires approval".into(),
            },
        }
    }

    /// The advertisable subset of a transport's catalogue: allowlisted names
    /// only, in **allowlist** order rather than catalogue order, because the
    /// operator's list is the authority and a server does not get to influence
    /// the order a model reads its tools in.
    ///
    /// An allowlisted name the catalogue does not offer is simply absent. It is
    /// never synthesized from the policy — a spec built here would carry no
    /// schema, and hand-written schemas are the drift this whole path exists to
    /// avoid.
    ///
    /// Approval is deliberately not consulted: see [`ToolGateway::advertise`].
    fn advertisable(&self, catalogue: &[ToolSpec]) -> Vec<ToolSpec> {
        self.allowed
            .iter()
            .filter_map(|(name, _)| catalogue.iter().find(|spec| &spec.name == name).cloned())
            .collect()
    }
}

/// Scrubs known secret values out of anything on its way into the Ledger
/// (Constitution VI: a secret is never in the record by value). Values reach it
/// either literally, from a caller that already holds them, or — the shape a
/// config should use — by resolving `SecretRef`s through a
/// [`SecretProvider`](crate::secret::SecretProvider).
pub struct Redactor {
    secrets: Vec<SecretValue>,
}

impl Redactor {
    pub fn new(secrets: Vec<String>) -> Self {
        Redactor::from_values(secrets.into_iter().map(SecretValue::new))
    }

    /// Resolves each reference through the provider — the moment
    /// configuration-held *references* become in-memory values (design §7.13).
    ///
    /// One unresolvable reference fails the whole construction: a `Redactor`
    /// built from a misconfigured reference would scrub nothing, and would do it
    /// silently.
    pub fn resolve(provider: &dyn SecretProvider, refs: &[SecretRef]) -> Result<Redactor> {
        let values: Vec<SecretValue> = refs
            .iter()
            .map(|r| provider.resolve(r))
            .collect::<Result<_>>()?;
        Ok(Redactor::from_values(values))
    }

    /// An empty secret is dropped rather than stored: `str::replace` treats the
    /// empty needle as matching everywhere, so keeping one would splice `***`
    /// between every character of every payload.
    fn from_values(values: impl IntoIterator<Item = SecretValue>) -> Self {
        Redactor {
            secrets: values
                .into_iter()
                .filter(|s| !s.expose().is_empty())
                .collect(),
        }
    }

    pub fn redact(&self, text: &str) -> String {
        let mut out = text.to_string();
        for secret in &self.secrets {
            out = out.replace(secret.expose(), "***");
        }
        out
    }

    /// Serializes `value` and scrubs the strings inside it, leaving its shape
    /// intact — so the captured payload stays parseable for replay.
    ///
    /// Serialize *then* scrub, never the other way round: a secret containing a
    /// quote, a backslash or a newline is JSON-escaped inside a serialized
    /// payload, so the literal needle would not appear in it and the secret
    /// would be missed entirely.
    pub fn redact_json<T: Serialize + ?Sized>(&self, value: &T) -> Result<String> {
        Ok(self.redact_value(&serde_json::to_value(value)?).to_string())
    }

    /// One call with everything model-authored about it scrubbed. The name is
    /// scrubbed for the same reason the arguments are: it is model-chosen text,
    /// so it can carry an echoed secret. The id is ours, never the model's
    /// words, so it passes through — and it must, or the answer that names it
    /// would dangle.
    pub fn redact_call(&self, call: &ToolCall) -> ToolCall {
        ToolCall {
            id: call.id.clone(),
            tool: self.redact(&call.tool),
            args: self.redact_value(&call.args),
        }
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

/// Hand-written, because `SecretValue` is deliberately not `Clone`: a run
/// configures **one** secret set and both the loop and the gateway must scrub
/// the same values, so this copies the material rather than widening
/// `secret.rs`'s public API. Both copies are `Zeroizing` and both zeroize on
/// drop. The empty-secret filter is not re-applied: the source is already
/// filtered.
impl Clone for Redactor {
    fn clone(&self) -> Self {
        Redactor {
            secrets: self
                .secrets
                .iter()
                .map(|s| SecretValue::new(s.expose()))
                .collect(),
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

    /// What a model may be told it can do: the transport's own catalogue,
    /// filtered to the policy's allowlist. The filter lives here, in the one
    /// place that owns both collaborators, so "you cannot advertise what the
    /// policy forbids" is structural rather than reviewed — the same move
    /// [`ToolGateway::call_captured`] makes for calls.
    ///
    /// Discovered, never hand-written: the schema a model is shown is the one
    /// the server will validate against, so the two cannot drift.
    ///
    /// An allowlisted `Mutating` tool with no approval **is** advertised. It is
    /// visible, and refused at call time with a reason the model is told.
    /// Withholding it here would make `skein-acp`'s permission decorator
    /// unreachable: `call_captured` consults the policy before the transport, so
    /// a tool the policy never allows never becomes a question for a human.
    pub fn advertise(&mut self) -> Result<Vec<ToolSpec>> {
        let catalogue = self.transport.list()?;
        Ok(self.policy.advertisable(&catalogue))
    }

    /// The governed path, returning only what the trusted caller needs.
    pub fn call(
        &mut self,
        run_id: &str,
        call: &ToolCall,
        ledger: &mut Ledger,
    ) -> Result<ToolOutcome> {
        self.call_captured(run_id, call, ledger)
            .map(|(outcome, _)| outcome)
    }

    /// The governed path, returning both the raw outcome for the trusted caller
    /// and the redacted capture that is safe to put back in front of a model.
    /// The Ledger is borrowed, not owned: the caller inspects it afterwards, and
    /// the gateway's steps land in the same chain as the rest of the run.
    pub fn call_captured(
        &mut self,
        run_id: &str,
        call: &ToolCall,
        ledger: &mut Ledger,
    ) -> Result<(ToolOutcome, CapturedResult)> {
        // Only the three recorded copies are scrubbed — the policy decides on
        // the raw name below, and the transport receives the raw call.
        //
        // Recorded before the decision, so a refused attempt still names itself.
        let attempt = self.redactor.redact_call(call);
        let tool = attempt.tool.clone();
        ledger.append(run_id, StepKind::ToolCall, serde_json::to_string(&attempt)?)?;

        let decision = self.policy.decide(&call.tool);
        let (verdict, reason) = match &decision {
            Decision::Allow { reason } => ("allowed", reason.clone()),
            Decision::Deny { reason } => ("denied", reason.clone()),
        };
        let record = ApprovalRecord {
            tool: tool.clone(),
            decision: verdict.to_string(),
            reason: reason.clone(),
        };
        ledger.append(run_id, StepKind::Approval, serde_json::to_string(&record)?)?;

        if let Decision::Deny { .. } = decision {
            return Err(SkeinError::ToolDenied {
                tool: call.tool.clone(),
                reason,
            });
        }

        // The tool needs the real secret; only the record must not have it.
        let outcome = self.transport.call(call)?;

        let captured = CapturedResult {
            tool,
            content: self.redactor.redact(&outcome.content),
        };
        ledger.append(
            run_id,
            StepKind::ToolResult,
            serde_json::to_string(&captured)?,
        )?;
        Ok((outcome, captured))
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
