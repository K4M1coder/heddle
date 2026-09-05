//! Spec 002, User Story 1, Acceptance Scenario 1: *"Given a workflow with
//! sequential nodes, when I run it, then each step produces a `Step` in the
//! Ledger and the final result is reached."*

mod common;

use common::{
    completed_nodes, engine, outcome_of, read_file, says, says_and_calls, RecordingTransport,
    ScriptedModel,
};
use skein_core::{Ledger, Message, StepKind};
use skein_workflow::{Node, Workflow, WorkflowExit};

fn three_node_workflow() -> Workflow {
    Workflow::new(
        "plan-code-package",
        vec![
            Node::Agent {
                id: "plan".into(),
                prompt: Message::user_text("draft a plan"),
            },
            Node::Tool {
                id: "read-spec".into(),
                call: read_file("spec.md"),
            },
            Node::Agent {
                id: "package".into(),
                prompt: Message::user_text("package it"),
            },
        ],
    )
}

#[test]
fn a_three_node_workflow_runs_every_node_in_order_and_reaches_its_final_result() {
    let mut engine = engine(
        ScriptedModel::new(vec![says("a plan"), says("packaged")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();

    let run = engine
        .run("run-seq", &three_node_workflow(), &mut ledger)
        .expect("a fully scripted sequential workflow must complete");

    assert_eq!(run.exit, WorkflowExit::Completed);
    assert_eq!(
        run.final_outcome.as_deref(),
        Some("packaged"),
        "the run's final result is the last node's outcome"
    );
}

#[test]
fn every_node_lands_exactly_one_workflow_node_step_in_graph_order() {
    let mut engine = engine(
        ScriptedModel::new(vec![says("a plan"), says("packaged")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();

    engine
        .run("run-seq", &three_node_workflow(), &mut ledger)
        .expect("a fully scripted sequential workflow must complete");

    assert_eq!(
        completed_nodes(&ledger, "run-seq"),
        vec!["plan", "read-spec", "package"],
        "one WorkflowNode step per node, in node order and no more"
    );
}

#[test]
fn each_node_records_what_it_actually_produced() {
    let mut engine = engine(
        ScriptedModel::new(vec![says("a plan"), says("packaged")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();

    engine
        .run("run-seq", &three_node_workflow(), &mut ledger)
        .expect("a fully scripted sequential workflow must complete");

    assert_eq!(
        outcome_of(&ledger, "run-seq", "plan").as_deref(),
        Some("a plan")
    );
    assert_eq!(
        outcome_of(&ledger, "run-seq", "read-spec").as_deref(),
        Some("the spec's bytes"),
        "a tool node records what the tool returned, not what a model said about it"
    );
    assert_eq!(
        outcome_of(&ledger, "run-seq", "package").as_deref(),
        Some("packaged")
    );
}

#[test]
fn a_tool_node_leaves_the_gateways_own_governed_triple_on_the_chain() {
    let mut engine = engine(
        ScriptedModel::new(vec![says("a plan"), says("packaged")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();

    engine
        .run("run-seq", &three_node_workflow(), &mut ledger)
        .expect("a fully scripted sequential workflow must complete");

    let kinds: Vec<StepKind> = ledger
        .log("run-seq")
        .iter()
        .map(|s| s.kind.clone())
        .collect();
    assert_eq!(
        kinds,
        vec![
            // plan: an agent node's exact model I/O is captured, exactly as
            // NativeLoop captures a turn's. A workflow is not a way to make a
            // model call the chain does not hold (Constitution V).
            StepKind::LlmRequest,
            StepKind::LlmResponse,
            StepKind::WorkflowNode,
            // read-spec: the gateway's own governed triple, unchanged.
            StepKind::ToolCall,
            StepKind::Approval,
            StepKind::ToolResult,
            StepKind::WorkflowNode,
            // package
            StepKind::LlmRequest,
            StepKind::LlmResponse,
            StepKind::WorkflowNode,
        ],
        "the workflow wraps ToolGateway::call_captured rather than re-implementing it, \
         so the governed triple is still there and still in that order"
    );
    ledger
        .verify_chain("run-seq")
        .expect("the chain a workflow writes is an ordinary hash-chained run");
}

#[test]
fn an_agent_node_is_told_about_the_tools_the_policy_allows() {
    let mut engine = engine(
        ScriptedModel::new(vec![says("a plan"), says("packaged")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();

    engine
        .run("run-seq", &three_node_workflow(), &mut ledger)
        .expect("a fully scripted sequential workflow must complete");

    let advertised: Vec<&str> = engine.client.seen[0]
        .tools
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(
        advertised,
        vec!["read_file"],
        "a node's turn is handed ToolGateway::advertise's output, already policy-filtered"
    );
}

#[test]
fn an_agent_node_that_asks_for_a_tool_has_it_mediated_by_the_gateway() {
    // D4 defines a node as one bounded turn *plus tool mediation for that turn*.
    // The tool runs and is governed and recorded; there is no second turn in
    // this slice for its result to be fed back into, which is a stated limit of
    // the one-turn definition and not an accident.
    let mut engine = engine(
        ScriptedModel::new(vec![says_and_calls(
            "reading first",
            vec![read_file("spec.md")],
        )]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();

    engine
        .run(
            "run-mediate",
            &Workflow::new(
                "one-agent",
                vec![Node::Agent {
                    id: "solo".into(),
                    prompt: Message::user_text("go"),
                }],
            ),
            &mut ledger,
        )
        .expect("the node completes");

    assert_eq!(
        engine.gateway.transport.calls, 1,
        "the tool the model asked for was actually run"
    );
    assert_eq!(
        completed_nodes(&ledger, "run-mediate"),
        vec!["solo"],
        "and the node still lands exactly one completion step"
    );
}
