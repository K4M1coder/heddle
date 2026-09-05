//! Spec 002, User Story 1, Acceptance Scenario 2 and SC-001: *"Given a workflow
//! interrupted after node 2, when I resume, then execution resumes at node 3
//! (idempotence of logged steps)"* — measurably, *"does not re-execute any
//! logged step"*.
//!
//! The interruption is real rather than simulated by editing the chain: the
//! model dies part-way through node 3, exactly as a provider outage or a killed
//! process would leave it. The resume then runs against a **second, independent**
//! engine whose model is scripted with node 3's turn and nothing else, and whose
//! transport panics if it is reached at all — so re-executing node 1 or node 2
//! is not merely unasserted, it is structurally impossible to do quietly.

mod common;

use common::{
    completed_nodes, engine, read_file, says, snapshot, RecordingTransport, ScriptedModel,
};
use skein_core::{Ledger, Message, SkeinError, StepKind};
use skein_workflow::{Node, Workflow, WorkflowExit};

const RUN: &str = "run-resume";

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

/// The first process: it completes nodes 1 and 2, then dies inside node 3.
/// Returns the chain it left behind.
fn interrupted_after_node_two() -> Ledger {
    let mut first = engine(
        // Turn 0 is node 1's and succeeds; turn 1 is node 3's and dies.
        ScriptedModel::failing_at(vec![says("a plan"), says("packaged")], 1),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();

    let err = first
        .run(RUN, &three_node_workflow(), &mut ledger)
        .expect_err("the first process must die inside node 3");
    assert!(
        matches!(err, SkeinError::Model(_)),
        "the interruption is a provider failure, not a workflow decision: {err:?}"
    );
    assert_eq!(
        completed_nodes(&ledger, RUN),
        vec!["plan", "read-spec"],
        "two nodes completed before the interruption, and node 3 logged none"
    );
    ledger
}

#[test]
fn resuming_continues_at_node_three_and_completes() {
    let mut ledger = interrupted_after_node_two();

    // A second, independent process: a fresh engine, a fresh model scripted with
    // exactly one turn, and a transport that explodes if it is called.
    let mut second = engine(
        ScriptedModel::new(vec![says("packaged")]),
        RecordingTransport::forbidden(),
    );

    let run = second
        .run(RUN, &three_node_workflow(), &mut ledger)
        .expect("the resumed run completes");

    assert_eq!(run.exit, WorkflowExit::Completed);
    assert_eq!(run.final_outcome.as_deref(), Some("packaged"));
    assert_eq!(
        completed_nodes(&ledger, RUN),
        vec!["plan", "read-spec", "package"],
        "the resumed run appends node 3's completion and re-logs nothing"
    );
}

#[test]
fn the_already_logged_nodes_executors_are_never_entered() {
    let mut ledger = interrupted_after_node_two();
    let mut second = engine(
        ScriptedModel::new(vec![says("packaged")]),
        RecordingTransport::forbidden(),
    );

    second
        .run(RUN, &three_node_workflow(), &mut ledger)
        .expect("the resumed run completes");

    // The point of SC-001, stated as a count rather than as an absence: the
    // workflow has two agent nodes and one tool node, and the resuming process
    // performed one turn and zero tool calls.
    assert_eq!(
        second.client.calls, 1,
        "node 1's executor was never entered — one turn made, for node 3"
    );
    assert_eq!(
        second.gateway.transport.calls, 0,
        "node 2's executor was never entered — no tool was called again"
    );
    assert_eq!(
        second.client.seen[0].messages[0],
        Message::user_text("package it"),
        "and the one turn that was made was node 3's, not node 1's"
    );
}

#[test]
fn resuming_appends_exactly_one_new_completion_step_and_rewrites_nothing() {
    let mut ledger = interrupted_after_node_two();
    let before = snapshot(&ledger, RUN);

    let mut second = engine(
        ScriptedModel::new(vec![says("packaged")]),
        RecordingTransport::forbidden(),
    );
    second
        .run(RUN, &three_node_workflow(), &mut ledger)
        .expect("the resumed run completes");

    let after = snapshot(&ledger, RUN);
    assert_eq!(
        after[..before.len()],
        before[..],
        "an append-only chain: resume adds to it and edits none of it"
    );

    let appended: Vec<StepKind> = after[before.len()..]
        .iter()
        .map(|s| s.kind.clone())
        .collect();
    assert_eq!(
        appended,
        vec![
            StepKind::LlmRequest,
            StepKind::LlmResponse,
            StepKind::WorkflowNode
        ],
        "exactly node 3's work, and exactly one new WorkflowNode step"
    );
}

#[test]
fn the_chain_a_second_process_appends_to_still_verifies() {
    let mut ledger = interrupted_after_node_two();
    let mut second = engine(
        ScriptedModel::new(vec![says("packaged")]),
        RecordingTransport::forbidden(),
    );
    second
        .run(RUN, &three_node_workflow(), &mut ledger)
        .expect("the resumed run completes");

    // `Ledger::append` derives seq and parent by scanning the chain for the run,
    // so a second process continues the sequence rather than restarting it. This
    // is the property resume rests on, asserted rather than assumed.
    ledger
        .verify_chain(RUN)
        .expect("resume must not break the hash chain");
    let seqs: Vec<u64> = ledger.log(RUN).iter().map(|s| s.seq).collect();
    assert_eq!(seqs, (0..seqs.len() as u64).collect::<Vec<_>>());
}

#[test]
fn a_run_with_nothing_left_to_do_is_idempotent() {
    let mut ledger = interrupted_after_node_two();
    let mut second = engine(
        ScriptedModel::new(vec![says("packaged")]),
        RecordingTransport::forbidden(),
    );
    second
        .run(RUN, &three_node_workflow(), &mut ledger)
        .expect("the resumed run completes");
    let after_first_resume = snapshot(&ledger, RUN);

    // A third process re-runs an already-complete workflow: nothing to execute,
    // nothing to append, and it still reports the same result. A model and a
    // transport that both explode on use make "nothing to execute" structural.
    let mut third = engine(ScriptedModel::forbidden(), RecordingTransport::forbidden());
    let run = third
        .run(RUN, &three_node_workflow(), &mut ledger)
        .expect("re-running a complete workflow is not an error");

    assert_eq!(run.exit, WorkflowExit::Completed);
    assert_eq!(
        run.final_outcome.as_deref(),
        Some("packaged"),
        "the result is read back off the chain, not recomputed"
    );
    assert_eq!(
        snapshot(&ledger, RUN),
        after_first_resume,
        "a completed run that is run again grows the Ledger by nothing"
    );
    assert_eq!(third.client.calls, 0);
}

/// The positive control for the four tests above.
///
/// Every one of them proves an *absence* — a turn not taken, a step not
/// appended. Absences pass for the wrong reason if the fixture is simply inert,
/// so this measures the same engine shape against a **fresh** chain and shows
/// that both earlier nodes do execute when nothing on the chain says they
/// already did. The skip is therefore attributable to the Ledger scan and to
/// nothing else about the wiring.
#[test]
fn without_a_prior_chain_the_same_wiring_executes_every_node() {
    let mut fresh = engine(
        ScriptedModel::new(vec![says("a plan"), says("packaged")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();

    fresh
        .run("run-control", &three_node_workflow(), &mut ledger)
        .expect("a fresh run completes");

    assert_eq!(fresh.client.calls, 2, "both agent nodes ran");
    assert_eq!(fresh.gateway.transport.calls, 1, "the tool node ran");
    assert_eq!(
        completed_nodes(&ledger, "run-control"),
        vec!["plan", "read-spec", "package"]
    );
}
