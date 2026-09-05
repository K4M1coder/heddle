//! Spec 037 acceptance (c): a recipe file, loaded from disk, compiled, and run
//! through the **real** [`WorkflowEngine`] — reaching its final result and
//! leaving one Ledger step per node behind it.
//!
//! This file is deliberately a near-copy of
//! `crates/heddle-workflow/tests/sequential.rs`, and the near is the whole
//! argument. Same engine, same doubles, same assertions, same expected chain —
//! the only difference is that the `Workflow` under test came out of
//! [`heddle_skills::compile`] instead of a hand-written `Workflow::new(...)`
//! literal. If both files pass, then compiling a recipe changes nothing about
//! how a workflow behaves, which is what "a recipe is a declarative
//! `Workflow`" (design §4.12) has to mean to be worth saying.
//!
//! The doubles below are re-derived locally rather than imported.
//! `heddle-workflow`'s `tests/common/mod.rs` is compiled into that crate's own
//! test binaries and is not an exported item, so there is nothing to depend on;
//! its own header records that every workflow-adjacent crate re-derives this
//! shape for exactly this reason. Keeping them local also keeps
//! `heddle-skills` free of a dev-dependency on another crate's test internals
//! (Constitution IV).

use heddle_core::{
    Ledger, Message, ModelClient, Redactor, Result, StepKind, ToolAccess, ToolCall, ToolGateway,
    ToolOutcome, ToolPolicy, ToolSpec, ToolTransport, TurnRequest, TurnResponse,
};
use heddle_skills::{compile, Recipe};
use heddle_workflow::{WorkflowEngine, WorkflowExit};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Test doubles. The shape is `crates/heddle-workflow/tests/common/mod.rs`'s.
// ---------------------------------------------------------------------------

/// A model whose every turn is decided in advance, and which **panics when its
/// script runs out**. The panic is the point: this test's claim is that the
/// compiled graph asks for exactly the turns the recipe describes, and a double
/// that quietly invented an extra answer could not state that.
struct ScriptedModel {
    script: Vec<TurnResponse>,
    calls: usize,
    seen: Vec<TurnRequest>,
}

impl ScriptedModel {
    fn new(script: Vec<TurnResponse>) -> Self {
        ScriptedModel {
            script,
            calls: 0,
            seen: Vec::new(),
        }
    }
}

impl ModelClient for ScriptedModel {
    fn turn(&mut self, req: &TurnRequest) -> Result<TurnResponse> {
        let i = self.calls;
        self.calls += 1;
        self.seen.push(req.clone());
        Ok(self
            .script
            .get(i)
            .unwrap_or_else(|| panic!("script exhausted: the engine asked for turn {i}"))
            .clone())
    }
}

/// A turn that says one thing and asks for nothing.
fn says(text: &str) -> TurnResponse {
    TurnResponse {
        message: Message::assistant_text(text),
        tokens_used: 1,
        final_output: true,
        tool_calls: Vec::new(),
    }
}

struct RecordingTransport {
    calls: usize,
    seen: Vec<ToolCall>,
    reply: String,
}

impl RecordingTransport {
    fn new(reply: &str) -> Self {
        RecordingTransport {
            calls: 0,
            seen: Vec::new(),
            reply: reply.to_string(),
        }
    }
}

impl ToolTransport for RecordingTransport {
    fn call(&mut self, call: &ToolCall) -> Result<ToolOutcome> {
        self.calls += 1;
        self.seen.push(call.clone());
        Ok(ToolOutcome {
            content: self.reply.clone(),
        })
    }

    fn list(&mut self) -> Result<Vec<ToolSpec>> {
        Ok(vec![ToolSpec::new(
            "read_file",
            "what read_file does",
            json!({"type": "object", "properties": {"path": {"type": "string"}}}),
        )])
    }
}

/// `read_file` is read-only and so needs no approval of its own, which keeps the
/// *gateway's* policy approval out of the way of the recipe's own `approval`
/// step — two different deciders that share a `StepKind`.
fn gateway(transport: RecordingTransport) -> ToolGateway<RecordingTransport> {
    ToolGateway::new(
        transport,
        ToolPolicy::new(vec![("read_file".into(), ToolAccess::ReadOnly)], Vec::new()),
        Redactor::new(Vec::new()),
    )
}

fn engine(
    model: ScriptedModel,
    transport: RecordingTransport,
) -> WorkflowEngine<ScriptedModel, RecordingTransport> {
    WorkflowEngine::new(model, gateway(transport), Redactor::new(Vec::new()))
}

/// The `node_id`s of a run's completed-node steps, in chain order. Reads each
/// payload rather than trusting position, so "one step per node, in node order"
/// is asserted against what a future reader of the chain would actually see.
fn completed_nodes(ledger: &Ledger, run_id: &str) -> Vec<String> {
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

fn outcome_of(ledger: &Ledger, run_id: &str, node_id: &str) -> Option<String> {
    ledger
        .log(run_id)
        .iter()
        .filter(|s| s.kind == StepKind::WorkflowNode)
        .filter_map(|s| serde_json::from_str::<serde_json::Value>(&s.payload).ok())
        .find(|v| v["node_id"] == node_id)
        .and_then(|v| v["outcome"].as_str().map(str::to_string))
}

// ---------------------------------------------------------------------------
// The recipe under test.
// ---------------------------------------------------------------------------

fn fixture() -> Recipe {
    Recipe::from_path(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plan_and_package.toml"),
    )
    .expect("the shipped fixture recipe must parse")
}

fn params() -> HashMap<String, String> {
    HashMap::from([("project".to_string(), "Heddle".to_string())])
}

/// The fixture's first three steps: `agent -> tool -> agent`.
///
/// Its fourth step is an `approval`, and the engine stops at an undecided gate
/// and returns `AwaitingApproval` — correct behaviour, already proven by
/// `heddle-workflow`'s own suite, and not what acceptance (c) is about. Dropping
/// it here makes this test the exact `Completed` shape `sequential.rs` asserts;
/// `the_full_recipe_stops_at_its_human_gate` below covers the fourth step.
fn first_three_steps() -> Recipe {
    let mut recipe = fixture();
    recipe.instructions.truncate(3);
    recipe
}

// ---------------------------------------------------------------------------
// Acceptance (c).
// ---------------------------------------------------------------------------

#[test]
fn a_recipe_loaded_from_a_file_runs_every_node_in_order_and_reaches_its_final_result() {
    let workflow = compile(&first_three_steps(), &params(), &["read_file"])
        .expect("the fixture recipe's required extension is advertised");
    let mut engine = engine(
        ScriptedModel::new(vec![says("a plan"), says("packaged")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();

    let run = engine
        .run("run-recipe", &workflow, &mut ledger)
        .expect("a fully scripted compiled recipe must complete");

    assert_eq!(run.exit, WorkflowExit::Completed);
    assert_eq!(
        run.final_outcome.as_deref(),
        Some("packaged"),
        "the run's final result is the last node's outcome, exactly as for a hand-built workflow"
    );
}

#[test]
fn every_compiled_node_lands_exactly_one_workflow_node_step_in_graph_order() {
    let workflow = compile(&first_three_steps(), &params(), &["read_file"]).expect("must compile");
    let mut engine = engine(
        ScriptedModel::new(vec![says("a plan"), says("packaged")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();

    engine
        .run("run-recipe", &workflow, &mut ledger)
        .expect("must complete");

    assert_eq!(
        completed_nodes(&ledger, "run-recipe"),
        vec!["plan", "read-spec", "package"],
        "one WorkflowNode step per recipe step, in recipe order and no more"
    );
}

#[test]
fn each_compiled_node_records_what_it_actually_produced() {
    let workflow = compile(&first_three_steps(), &params(), &["read_file"]).expect("must compile");
    let mut engine = engine(
        ScriptedModel::new(vec![says("a plan"), says("packaged")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();

    engine
        .run("run-recipe", &workflow, &mut ledger)
        .expect("must complete");

    assert_eq!(
        outcome_of(&ledger, "run-recipe", "plan").as_deref(),
        Some("a plan")
    );
    assert_eq!(
        outcome_of(&ledger, "run-recipe", "read-spec").as_deref(),
        Some("the spec's bytes"),
        "a compiled tool step records what the tool returned, not what a model said about it"
    );
    assert_eq!(
        outcome_of(&ledger, "run-recipe", "package").as_deref(),
        Some("packaged")
    );
}

#[test]
fn a_compiled_recipe_leaves_the_same_chain_a_hand_built_workflow_would() {
    let workflow = compile(&first_three_steps(), &params(), &["read_file"]).expect("must compile");
    let mut engine = engine(
        ScriptedModel::new(vec![says("a plan"), says("packaged")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();

    engine
        .run("run-recipe", &workflow, &mut ledger)
        .expect("must complete");

    let kinds: Vec<StepKind> = ledger
        .log("run-recipe")
        .iter()
        .map(|s| s.kind.clone())
        .collect();
    assert_eq!(
        kinds,
        vec![
            // plan
            StepKind::LlmRequest,
            StepKind::LlmResponse,
            StepKind::WorkflowNode,
            // read-spec: the gateway's governed triple, untouched. A recipe is
            // not a way around the governor (Constitution VI) because it
            // compiles to the same node the governor already mediates.
            StepKind::ToolCall,
            StepKind::Approval,
            StepKind::ToolResult,
            StepKind::WorkflowNode,
            // package
            StepKind::LlmRequest,
            StepKind::LlmResponse,
            StepKind::WorkflowNode,
        ],
        "byte for byte the chain `heddle-workflow`'s own sequential.rs asserts for the \
         hand-built equivalent — the recipe changed where the graph came from and nothing else"
    );
    ledger
        .verify_chain("run-recipe")
        .expect("the chain a compiled recipe writes is an ordinary hash-chained run");
}

#[test]
fn the_compiled_tool_step_reaches_the_transport_with_the_arguments_the_recipe_wrote() {
    let workflow = compile(&first_three_steps(), &params(), &["read_file"]).expect("must compile");
    let mut engine = engine(
        ScriptedModel::new(vec![says("a plan"), says("packaged")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();

    engine
        .run("run-recipe", &workflow, &mut ledger)
        .expect("must complete");

    assert_eq!(engine.gateway.transport.calls, 1);
    assert_eq!(engine.gateway.transport.seen[0].tool, "read_file");
    assert_eq!(engine.gateway.transport.seen[0].args["path"], "spec.md");
}

#[test]
fn a_compiled_agent_node_is_handed_the_persona_and_the_policys_tools() {
    let workflow = compile(&first_three_steps(), &params(), &["read_file"]).expect("must compile");
    let mut engine = engine(
        ScriptedModel::new(vec![says("a plan"), says("packaged")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();

    engine
        .run("run-recipe", &workflow, &mut ledger)
        .expect("must complete");

    // The recipe's `prompt` reached the model as part of the turn, with its
    // `{{project}}` resolved and its `{{kind}}` filled from the declared
    // default — the whole substitution path, observed at the far end.
    assert_eq!(
        engine.client.seen[0].messages[0].text(),
        "You are drafting a plan for the project named Heddle.\n\nDraft a plan."
    );
    let advertised: Vec<&str> = engine.client.seen[0]
        .tools
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(
        advertised,
        vec!["read_file"],
        "a compiled node's turn gets ToolGateway::advertise's already-filtered output"
    );
}

#[test]
fn the_full_recipe_stops_at_its_human_gate() {
    // The fourth step, which the tests above slice off. One `run` call, not an
    // approve-and-resume round trip: the claim here is only that a compiled
    // `approval` step is a real gate the engine honours, which is what makes the
    // compile test's `Node::Approval` assertion more than structural.
    let workflow = compile(&fixture(), &params(), &["read_file"]).expect("must compile");
    let mut engine = engine(
        ScriptedModel::new(vec![says("a plan"), says("packaged")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();

    let run = engine
        .run("run-gated", &workflow, &mut ledger)
        .expect("reaching a gate is not an error");

    assert_eq!(
        run.exit,
        WorkflowExit::AwaitingApproval {
            node_id: "ship".into()
        },
        "the recipe's approval step compiles to a gate that actually stops the run"
    );
    assert_eq!(
        completed_nodes(&ledger, "run-gated"),
        vec!["plan", "read-spec", "package"],
        "the three steps before the gate ran; the gate itself is not a completed node"
    );
}
