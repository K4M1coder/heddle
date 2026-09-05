//! Spec 002, User Story 1, Acceptance Scenario 3: *"Given an `Approval` node,
//! when execution reaches it, then it waits for human validation before
//! continuing"* — and the negative half the prose implies but does not state: a
//! human who says no stops the run.
//!
//! "Waits" is deliberately not implemented as a blocked thread. A parked thread
//! is a *paused* process, not a resumable one, and the property the spec needs is
//! that a run survives its process being killed. So waiting here means "`run`
//! returns `AwaitingApproval` and the chain remembers why", which a new process
//! can pick up — and which the tests below exercise by using a **different
//! engine instance** to record the decision from the one that asked for it.

mod common;

use common::{
    approval_decisions, completed_nodes, engine, outcome_of, says, snapshot, RecordingTransport,
    ScriptedModel,
};
use skein_core::{Ledger, Message};
use skein_workflow::{Node, Workflow, WorkflowEngine, WorkflowExit};

const RUN: &str = "run-approval";

fn gated_workflow() -> Workflow {
    Workflow::new(
        "draft-signoff-ship",
        vec![
            Node::Agent {
                id: "draft".into(),
                prompt: Message::user_text("draft the release notes"),
            },
            Node::Approval {
                id: "sign-off".into(),
                message: "ship this release?".into(),
            },
            Node::Agent {
                id: "ship".into(),
                prompt: Message::user_text("ship it"),
            },
        ],
    )
}

/// An engine scripted for **one** turn only. If the gate ever let execution run
/// past it, node 3 would ask for a second turn and the script would panic — so
/// "the next node did not run" cannot pass by accident.
fn one_turn_engine() -> WorkflowEngine<ScriptedModel, RecordingTransport> {
    engine(
        ScriptedModel::new(vec![says("drafted")]),
        RecordingTransport::forbidden(),
    )
}

#[test]
fn execution_stops_at_the_approval_node_and_names_it() {
    let mut e = one_turn_engine();
    let mut ledger = Ledger::new();

    let run = e
        .run(RUN, &gated_workflow(), &mut ledger)
        .expect("stopping on a human is a normal exit, not an error");

    assert_eq!(
        run.exit,
        WorkflowExit::AwaitingApproval {
            node_id: "sign-off".into()
        },
        "the exit names which node is waiting, so a caller need not recompute it"
    );
    assert_eq!(
        e.client.calls, 1,
        "node 1 ran; node 3's executor was never entered"
    );
    assert_eq!(
        completed_nodes(&ledger, RUN),
        vec!["draft"],
        "and the gate logged no completion for itself while undecided"
    );
}

#[test]
fn reaching_the_gate_records_a_pending_decision_on_the_chain() {
    let mut e = one_turn_engine();
    let mut ledger = Ledger::new();

    e.run(RUN, &gated_workflow(), &mut ledger).unwrap();

    assert_eq!(
        approval_decisions(&ledger, RUN),
        vec![("sign-off".to_string(), "pending".to_string())],
        "the question a human has to answer is on the chain, not only in memory"
    );
}

#[test]
fn re_polling_an_undecided_gate_repeats_the_answer_without_growing_the_ledger() {
    let mut ledger = Ledger::new();
    one_turn_engine()
        .run(RUN, &gated_workflow(), &mut ledger)
        .unwrap();
    let after_first = snapshot(&ledger, RUN);

    // A second process polls. Its model would panic if asked for any turn at
    // all, so this also proves the completed node 1 is not re-run on the way to
    // the gate.
    let mut second = engine(ScriptedModel::forbidden(), RecordingTransport::forbidden());
    let run = second.run(RUN, &gated_workflow(), &mut ledger).unwrap();

    assert_eq!(
        run.exit,
        WorkflowExit::AwaitingApproval {
            node_id: "sign-off".into()
        }
    );
    assert_eq!(
        snapshot(&ledger, RUN),
        after_first,
        "a slow human must not cost one Ledger step per poll"
    );
}

#[test]
fn an_approval_lets_the_run_continue_in_the_very_next_call() {
    let mut ledger = Ledger::new();
    one_turn_engine()
        .run(RUN, &gated_workflow(), &mut ledger)
        .unwrap();

    // The decision is recorded by a *different* engine from the one that asked,
    // which is the shape a CLI would have: `skein workflow decide` is not the
    // process that was running the workflow.
    let mut approver = engine(ScriptedModel::forbidden(), RecordingTransport::forbidden());
    approver
        .decide(RUN, "sign-off", true, &mut ledger)
        .expect("recording a decision is an append");

    let mut resumed = engine(
        ScriptedModel::new(vec![says("shipped")]),
        RecordingTransport::forbidden(),
    );
    let run = resumed.run(RUN, &gated_workflow(), &mut ledger).unwrap();

    assert_eq!(run.exit, WorkflowExit::Completed);
    assert_eq!(run.final_outcome.as_deref(), Some("shipped"));
    assert_eq!(
        resumed.client.calls, 1,
        "exactly node 3's turn — node 1 was skipped, and the gate needed no turn"
    );
    assert_eq!(
        completed_nodes(&ledger, RUN),
        vec!["draft", "sign-off", "ship"],
        "an approved gate lands its own completion step, so a later resume skips it too"
    );
    assert_eq!(
        outcome_of(&ledger, RUN, "sign-off").as_deref(),
        Some("approved")
    );
}

#[test]
fn a_rejection_ends_the_run_without_executing_the_next_node() {
    let mut ledger = Ledger::new();
    one_turn_engine()
        .run(RUN, &gated_workflow(), &mut ledger)
        .unwrap();

    let mut approver = engine(ScriptedModel::forbidden(), RecordingTransport::forbidden());
    approver
        .decide(RUN, "sign-off", false, &mut ledger)
        .unwrap();

    // A model that panics on any turn: if a rejection fell through to node 3,
    // this would not merely fail an assertion, it could not run at all.
    let mut resumed = engine(ScriptedModel::forbidden(), RecordingTransport::forbidden());
    let run = resumed.run(RUN, &gated_workflow(), &mut ledger).unwrap();

    assert_eq!(
        run.exit,
        WorkflowExit::Rejected {
            node_id: "sign-off".into()
        },
        "a refusal is a normal, named exit — not an error and not an Unsupported"
    );
    assert_eq!(resumed.client.calls, 0);
    assert_eq!(
        completed_nodes(&ledger, RUN),
        vec!["draft"],
        "a rejected gate completes nothing, so nothing downstream is ever skipped past"
    );
}

#[test]
fn a_rejected_run_stays_rejected_and_stops_growing_the_chain() {
    let mut ledger = Ledger::new();
    one_turn_engine()
        .run(RUN, &gated_workflow(), &mut ledger)
        .unwrap();
    engine(ScriptedModel::forbidden(), RecordingTransport::forbidden())
        .decide(RUN, "sign-off", false, &mut ledger)
        .unwrap();
    engine(ScriptedModel::forbidden(), RecordingTransport::forbidden())
        .run(RUN, &gated_workflow(), &mut ledger)
        .unwrap();
    let after_rejection = snapshot(&ledger, RUN);

    let run = engine(ScriptedModel::forbidden(), RecordingTransport::forbidden())
        .run(RUN, &gated_workflow(), &mut ledger)
        .unwrap();

    assert_eq!(
        run.exit,
        WorkflowExit::Rejected {
            node_id: "sign-off".into()
        }
    );
    assert_eq!(snapshot(&ledger, RUN), after_rejection);
}

#[test]
fn a_change_of_mind_is_a_later_step_not_an_edit() {
    // The chain is append-only, so "the human reconsidered" has to be readable
    // as a second decision that supersedes the first. This is why the scan takes
    // the *last* decision per node rather than the first.
    let mut ledger = Ledger::new();
    one_turn_engine()
        .run(RUN, &gated_workflow(), &mut ledger)
        .unwrap();

    let mut approver = engine(ScriptedModel::forbidden(), RecordingTransport::forbidden());
    approver
        .decide(RUN, "sign-off", false, &mut ledger)
        .unwrap();
    approver.decide(RUN, "sign-off", true, &mut ledger).unwrap();

    let mut resumed = engine(
        ScriptedModel::new(vec![says("shipped")]),
        RecordingTransport::forbidden(),
    );
    let run = resumed.run(RUN, &gated_workflow(), &mut ledger).unwrap();

    assert_eq!(run.exit, WorkflowExit::Completed);
    assert_eq!(
        approval_decisions(&ledger, RUN),
        vec![
            ("sign-off".to_string(), "pending".to_string()),
            ("sign-off".to_string(), "rejected".to_string()),
            ("sign-off".to_string(), "approved".to_string()),
        ],
        "every decision stays on the chain; the last one is the one that governs"
    );
    ledger.verify_chain(RUN).unwrap();
}

/// The gateway writes `StepKind::Approval` steps too, about *tool* calls decided
/// by policy. This proves the workflow gate's scan is not confused by them —
/// the two payloads share a `StepKind` and no field, so they are told apart by
/// parse rather than by hoping they never co-occur.
#[test]
fn a_tool_nodes_policy_approval_is_not_mistaken_for_a_human_decision() {
    let mut e = engine(
        ScriptedModel::new(vec![says("drafted")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();
    let workflow = Workflow::new(
        "tool-then-gate",
        vec![
            Node::Tool {
                id: "read".into(),
                call: common::read_file("spec.md"),
            },
            Node::Approval {
                id: "sign-off".into(),
                message: "ok?".into(),
            },
            Node::Agent {
                id: "ship".into(),
                prompt: Message::user_text("ship it"),
            },
        ],
    );

    let run = e.run("run-mixed", &workflow, &mut ledger).unwrap();

    assert_eq!(
        run.exit,
        WorkflowExit::AwaitingApproval {
            node_id: "sign-off".into()
        },
        "the tool's own policy approval must not read as the gate's human decision"
    );
    assert_eq!(
        approval_decisions(&ledger, "run-mixed"),
        vec![("sign-off".to_string(), "pending".to_string())],
        "and only the workflow-level payload is picked up by the gate's scan"
    );
    assert_eq!(e.client.calls, 0, "node 3 never ran");
}
