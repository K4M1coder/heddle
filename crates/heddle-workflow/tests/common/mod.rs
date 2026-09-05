//! Shared test doubles for the workflow acceptance tests.
//!
//! The shape is `crates/heddle-core/tests/native_loop.rs`'s, deliberately: a
//! `ScriptedModel` that replays a fixed script and **panics loudly when the
//! script is exhausted**, and a `RecordingTransport` whose `Forbidden` mode
//! panics if it is reached at all. Both panics are the point rather than an
//! inconvenience — this crate's central claim is "a resumed run does not
//! re-execute a logged node", and the strongest way to state it is a fixture in
//! which re-execution cannot silently succeed.
//!
//! It lives in `tests/common/` rather than being copied into each test file,
//! which is a departure from every other crate in this workspace. Those crates
//! each have **one** test file that needs a model double; this one has four, and
//! four copies of a fixture whose exhaustion behaviour is load-bearing would
//! make the resume proof harder to audit, not easier.

#![allow(dead_code)]

use heddle_core::{
    HeddleError, Ledger, Message, ModelClient, Redactor, Result, Step, StepKind, ToolAccess,
    ToolCall, ToolGateway, ToolOutcome, ToolPolicy, ToolSpec, ToolTransport, TurnRequest,
    TurnResponse,
};
use heddle_workflow::WorkflowEngine;
use serde_json::json;

/// A model whose every turn is decided in advance. `calls` is what the resume
/// test reads to prove a node's executor was never entered.
pub struct ScriptedModel {
    script: Vec<TurnResponse>,
    pub calls: usize,
    fail_at: Option<usize>,
    /// Every request as the client received it.
    pub seen: Vec<TurnRequest>,
}

impl ScriptedModel {
    pub fn new(script: Vec<TurnResponse>) -> Self {
        ScriptedModel {
            script,
            calls: 0,
            fail_at: None,
            seen: Vec::new(),
        }
    }

    /// A model that dies on the `fail_at`-th turn — how this crate simulates a
    /// process interrupted part-way through a node, without a second mechanism
    /// for "interrupted" that the product does not have.
    pub fn failing_at(script: Vec<TurnResponse>, fail_at: usize) -> Self {
        ScriptedModel {
            fail_at: Some(fail_at),
            ..ScriptedModel::new(script)
        }
    }

    /// A model that must never be asked for a turn at all.
    pub fn forbidden() -> Self {
        ScriptedModel::new(Vec::new())
    }
}

impl ModelClient for ScriptedModel {
    fn turn(&mut self, req: &TurnRequest) -> Result<TurnResponse> {
        let i = self.calls;
        self.calls += 1;
        self.seen.push(req.clone());
        if self.fail_at == Some(i) {
            return Err(HeddleError::Model(format!("scripted failure on turn {i}")));
        }
        Ok(self
            .script
            .get(i)
            .unwrap_or_else(|| panic!("script exhausted: the engine asked for turn {i}"))
            .clone())
    }
}

/// A turn that says one thing and asks for nothing.
pub fn says(text: &str) -> TurnResponse {
    TurnResponse {
        message: Message::assistant_text(text),
        tokens_used: 1,
        final_output: true,
        tool_calls: Vec::new(),
    }
}

/// A turn that says something *and* asks for a tool.
pub fn says_and_calls(text: &str, calls: Vec<ToolCall>) -> TurnResponse {
    TurnResponse {
        tool_calls: calls,
        ..says(text)
    }
}

pub enum TransportMode {
    Reply(String),
    Forbidden,
}

pub struct RecordingTransport {
    pub calls: usize,
    pub seen: Vec<ToolCall>,
    mode: TransportMode,
    catalogue: Vec<ToolSpec>,
}

impl RecordingTransport {
    pub fn new(reply: &str) -> Self {
        RecordingTransport {
            calls: 0,
            seen: Vec::new(),
            mode: TransportMode::Reply(reply.to_string()),
            catalogue: vec![spec("read_file")],
        }
    }

    /// A transport that explodes if it is reached. Used wherever a test's claim
    /// is that a node was *not* executed.
    pub fn forbidden() -> Self {
        RecordingTransport {
            mode: TransportMode::Forbidden,
            ..RecordingTransport::new("")
        }
    }
}

impl ToolTransport for RecordingTransport {
    fn call(&mut self, call: &ToolCall) -> Result<ToolOutcome> {
        self.calls += 1;
        self.seen.push(call.clone());
        match &self.mode {
            TransportMode::Reply(reply) => Ok(ToolOutcome {
                content: reply.clone(),
            }),
            TransportMode::Forbidden => {
                panic!("this node's executor must never be entered, yet it reached a transport")
            }
        }
    }

    fn list(&mut self) -> Result<Vec<ToolSpec>> {
        Ok(self.catalogue.clone())
    }
}

fn spec(name: &str) -> ToolSpec {
    ToolSpec::new(
        name,
        format!("what {name} does"),
        json!({"type": "object", "properties": {name: {"type": "string"}}}),
    )
}

/// `read_file` is read-only and therefore allowed without an approval, which
/// keeps the *tool* gateway's own approval out of the way of the tests about the
/// *workflow* approval node.
pub fn gateway(transport: RecordingTransport) -> ToolGateway<RecordingTransport> {
    ToolGateway::new(
        transport,
        ToolPolicy::new(vec![("read_file".into(), ToolAccess::ReadOnly)], Vec::new()),
        Redactor::new(Vec::new()),
    )
}

/// The engine under test, wired the way every test here wires it. The redactor
/// is empty because these tests are about sequencing and resume, not redaction —
/// which `heddle-core`'s own suite already covers at the layer that owns it.
pub fn engine(
    model: ScriptedModel,
    transport: RecordingTransport,
) -> WorkflowEngine<ScriptedModel, RecordingTransport> {
    WorkflowEngine::new(model, gateway(transport), Redactor::new(Vec::new()))
}

pub fn read_file(path: &str) -> ToolCall {
    ToolCall::new("read_file", json!({ "path": path }))
}

/// The `node_id`s of a run's completed-node steps, in chain order. Reads the
/// payload rather than trusting position, so "one step per node, in node order"
/// is asserted against what a future reader of the chain would actually see.
pub fn completed_nodes(ledger: &Ledger, run_id: &str) -> Vec<String> {
    ledger
        .log(run_id)
        .iter()
        .filter(|s| s.kind == StepKind::WorkflowNode)
        .map(|s| {
            serde_json::from_str::<serde_json::Value>(&s.payload)
                .expect("a WorkflowNode payload must be JSON")["node_id"]
                .as_str()
                .expect("a WorkflowNode payload must carry a node_id")
                .to_string()
        })
        .collect()
}

/// The recorded outcome of a completed node.
pub fn outcome_of(ledger: &Ledger, run_id: &str, node_id: &str) -> Option<String> {
    ledger
        .log(run_id)
        .iter()
        .filter(|s| s.kind == StepKind::WorkflowNode)
        .filter_map(|s| serde_json::from_str::<serde_json::Value>(&s.payload).ok())
        .find(|v| v["node_id"] == node_id)
        .and_then(|v| v["outcome"].as_str().map(str::to_string))
}

/// Every workflow-level approval decision on the chain, in order, as
/// `(node_id, decision)`. Filters out the gateway's own `ApprovalRecord`s, which
/// share the `StepKind` and carry a `tool` where these carry a `node_id`.
pub fn approval_decisions(ledger: &Ledger, run_id: &str) -> Vec<(String, String)> {
    ledger
        .log(run_id)
        .iter()
        .filter(|s| s.kind == StepKind::Approval)
        .filter_map(|s| serde_json::from_str::<serde_json::Value>(&s.payload).ok())
        .filter_map(|v| {
            Some((
                v["node_id"].as_str()?.to_string(),
                v["decision"].as_str()?.to_string(),
            ))
        })
        .collect()
}

/// Every step of a run, cloned — so a test can compare a chain before and after
/// a call and state "nothing was appended" as an equality rather than a count.
pub fn snapshot(ledger: &Ledger, run_id: &str) -> Vec<Step> {
    ledger.log(run_id).into_iter().cloned().collect()
}
