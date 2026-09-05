//! Spec 002, User Story 3, Acceptance Scenario 1 and SC-003: *"Given a running
//! workflow, when a node completes, then the corresponding task moves to the
//! appropriate status in the resolved TaskTracker"*.
//!
//! The engine is handed a tracker through `TaskTracker` and never learns what is
//! behind it (Constitution IV) — the double below is in-memory, and a
//! silo-backed `LocalTracker` would satisfy the same assertions without this
//! crate gaining a dependency on it.
//!
//! Two properties carry most of the weight and are worth naming up front:
//!
//! 1. **Which task belongs to which node is recorded on the chain**, not
//!    derived. A remote tracker assigns its own ids (`PROJ-123`), so a derived
//!    key would only ever work for the local one. Reading the binding back off
//!    the Ledger is the same discipline the completed-node scan already uses
//!    (Constitution V).
//! 2. **A resumed run does not re-open a task it already opened.** That falls
//!    out of the binding above rather than needing a second mechanism.

mod common;

use common::{
    engine, read_file, says, task_bindings, tracked_engine, RecordingTracker, RecordingTransport,
    ScriptedModel,
};
use heddle_core::{Ledger, Message, StepKind, TaskQuery, TaskStatus, TaskTracker};
use heddle_workflow::{Node, Workflow, WorkflowEngine, WorkflowExit};

const RUN: &str = "run-tracked";

fn two_node_workflow() -> Workflow {
    Workflow::new(
        "plan-and-read",
        vec![
            Node::Agent {
                id: "plan".into(),
                prompt: Message::user_text("draft a plan"),
            },
            Node::Tool {
                id: "read-spec".into(),
                call: read_file("spec.md"),
            },
        ],
    )
}

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

// ---- US3 scenario 1: a completed node moves its task ----

#[test]
fn every_node_that_completes_leaves_its_task_done() {
    let mut e = tracked_engine(
        ScriptedModel::new(vec![says("a plan")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();

    let run = e
        .run(RUN, &two_node_workflow(), &mut ledger)
        .expect("a fully scripted workflow completes");

    assert_eq!(run.exit, WorkflowExit::Completed);
    assert_eq!(
        e.tracker().expect("this engine has one").board(),
        vec![
            ("plan-and-read: plan".to_string(), TaskStatus::Done),
            ("plan-and-read: read-spec".to_string(), TaskStatus::Done),
        ],
        "one task per node, titled by workflow and node, all finished"
    );
}

#[test]
fn a_task_is_opened_in_progress_before_its_node_runs() {
    // "Moves to the appropriate status" needs a status to move *from*. The task
    // is opened before the executor is entered, so a run that dies inside a node
    // leaves that node's task visibly in progress rather than absent.
    let mut e = tracked_engine(
        ScriptedModel::failing_at(vec![says("never reached")], 0),
        RecordingTransport::forbidden(),
    );
    let mut ledger = Ledger::new();

    e.run(RUN, &two_node_workflow(), &mut ledger)
        .expect_err("the model dies inside node 1");

    assert_eq!(
        e.tracker().unwrap().board(),
        vec![("plan-and-read: plan".to_string(), TaskStatus::InProgress)],
        "the interrupted node's task is open, and the node after it has none"
    );
}

#[test]
fn a_tasks_links_name_the_run_and_the_node_it_tracks() {
    let mut e = tracked_engine(
        ScriptedModel::new(vec![says("a plan")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();
    e.run(RUN, &two_node_workflow(), &mut ledger).unwrap();

    let mine = e
        .tracker()
        .unwrap()
        .list(&TaskQuery::all().linked_to(RUN))
        .unwrap();

    assert_eq!(mine.len(), 2, "both tasks are findable by their run id");
    assert_eq!(mine[0].links, vec![RUN.to_string(), "plan".to_string()]);
}

// ---- the binding lives on the chain ----

#[test]
fn the_chain_records_which_task_each_node_opened() {
    let mut e = tracked_engine(
        ScriptedModel::new(vec![says("a plan")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();
    e.run(RUN, &two_node_workflow(), &mut ledger).unwrap();

    let bindings = task_bindings(&ledger, RUN);
    assert_eq!(
        bindings.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        vec!["plan", "read-spec"],
        "one binding per node, in node order"
    );
    for (node_id, task_id) in &bindings {
        let task = e
            .tracker()
            .unwrap()
            .task(&heddle_core::TaskId::new(task_id))
            .unwrap_or_else(|| panic!("node {node_id}'s recorded task must exist"));
        assert!(task.title.ends_with(node_id));
    }
    ledger
        .verify_chain(RUN)
        .expect("adding tracker bindings keeps the chain an ordinary hash-chained run");
}

#[test]
fn an_untracked_engine_writes_exactly_the_chain_it_wrote_before() {
    // The binding step is written only when there is a tracker to bind to.
    // Otherwise every existing workflow run would grow a step recording a
    // relationship that does not exist.
    let mut e = engine(
        ScriptedModel::new(vec![says("a plan")]),
        RecordingTransport::new("the spec's bytes"),
    );
    let mut ledger = Ledger::new();
    e.run(RUN, &two_node_workflow(), &mut ledger).unwrap();

    assert!(
        ledger
            .log(RUN)
            .iter()
            .all(|s| s.kind != StepKind::StateChange),
        "no tracker, no binding"
    );
}

// ---- resume ----

#[test]
fn a_resumed_run_does_not_open_a_second_task_for_a_node_it_already_finished() {
    let workflow = two_node_workflow();
    let mut ledger = Ledger::new();

    let mut first = tracked_engine(
        ScriptedModel::new(vec![says("a plan")]),
        RecordingTransport::new("the spec's bytes"),
    );
    first.run(RUN, &workflow, &mut ledger).unwrap();

    // A second, independent process: a fresh engine with a fresh tracker whose
    // counters start at zero, and a transport that explodes if it is reached.
    let mut second = tracked_engine(ScriptedModel::forbidden(), RecordingTransport::forbidden());
    let run = second
        .run(RUN, &workflow, &mut ledger)
        .expect("a run with nothing left to do still returns");

    assert_eq!(run.exit, WorkflowExit::Completed);
    assert_eq!(
        second.tracker().unwrap().creates,
        0,
        "every node was already on the chain, so no task was opened"
    );
    assert_eq!(
        task_bindings(&ledger, RUN).len(),
        2,
        "and no second binding was appended either"
    );
}

// ---- the approval gate's statuses ----

#[test]
fn a_node_waiting_on_a_human_has_its_task_blocked() {
    let mut e = tracked_engine(
        ScriptedModel::new(vec![says("drafted")]),
        RecordingTransport::forbidden(),
    );
    let mut ledger = Ledger::new();

    let run = e.run(RUN, &gated_workflow(), &mut ledger).unwrap();

    assert_eq!(
        run.exit,
        WorkflowExit::AwaitingApproval {
            node_id: "sign-off".into()
        }
    );
    assert_eq!(
        e.tracker().unwrap().board(),
        vec![
            ("draft-signoff-ship: draft".to_string(), TaskStatus::Done),
            (
                "draft-signoff-ship: sign-off".to_string(),
                TaskStatus::Blocked
            ),
        ],
        "the gate's own task is blocked, and the node behind it has none yet"
    );
}

#[test]
fn polling_a_pending_gate_re_asserts_the_status_without_re_opening_the_task() {
    let mut e = tracked_engine(
        ScriptedModel::new(vec![says("drafted")]),
        RecordingTransport::forbidden(),
    );
    let mut ledger = Ledger::new();
    e.run(RUN, &gated_workflow(), &mut ledger).unwrap();
    let creates_after_first = e.tracker().unwrap().creates;

    e.run(RUN, &gated_workflow(), &mut ledger)
        .expect("polling a pending gate is allowed");

    assert_eq!(
        e.tracker().unwrap().creates,
        creates_after_first,
        "a second poll reuses the task the chain already binds to this node"
    );
    assert_eq!(
        task_bindings(&ledger, RUN).len(),
        2,
        "and appends no second binding"
    );
}

#[test]
fn a_rejected_gate_cancels_its_task() {
    let mut e = tracked_engine(
        ScriptedModel::new(vec![says("drafted")]),
        RecordingTransport::forbidden(),
    );
    let mut ledger = Ledger::new();
    e.run(RUN, &gated_workflow(), &mut ledger).unwrap();

    e.decide(RUN, "sign-off", false, &mut ledger).unwrap();
    let run = e.run(RUN, &gated_workflow(), &mut ledger).unwrap();

    assert_eq!(
        run.exit,
        WorkflowExit::Rejected {
            node_id: "sign-off".into()
        }
    );
    assert_eq!(
        e.tracker().unwrap().board()[1].1,
        TaskStatus::Cancelled,
        "a human who said no closed the task rather than leaving it blocked forever"
    );
}

#[test]
fn an_approved_gate_finishes_its_task_and_the_run_carries_on() {
    let mut e = tracked_engine(
        ScriptedModel::new(vec![says("drafted"), says("shipped")]),
        RecordingTransport::forbidden(),
    );
    let mut ledger = Ledger::new();
    e.run(RUN, &gated_workflow(), &mut ledger).unwrap();

    e.decide(RUN, "sign-off", true, &mut ledger).unwrap();
    let run = e.run(RUN, &gated_workflow(), &mut ledger).unwrap();

    assert_eq!(run.exit, WorkflowExit::Completed);
    assert_eq!(
        e.tracker().unwrap().board(),
        vec![
            ("draft-signoff-ship: draft".to_string(), TaskStatus::Done),
            ("draft-signoff-ship: sign-off".to_string(), TaskStatus::Done),
            ("draft-signoff-ship: ship".to_string(), TaskStatus::Done),
        ]
    );
}

// ---- the tracker is discovered through the trait ----

#[test]
fn the_engine_holds_whatever_tracker_it_was_given() {
    // Constitution IV, stated as a compile-time fact: `with_tracker` accepts any
    // `TaskTracker`, and the engine names no backend. This test exists so the
    // generic parameter cannot be quietly replaced by a concrete type without a
    // test going red.
    fn accepts_any<K: TaskTracker>(
        tracker: K,
    ) -> WorkflowEngine<ScriptedModel, RecordingTransport, K> {
        WorkflowEngine::with_tracker(
            ScriptedModel::forbidden(),
            common::gateway(RecordingTransport::forbidden()),
            heddle_core::Redactor::new(Vec::new()),
            tracker,
        )
    }

    let e = accepts_any(RecordingTracker::new());
    assert!(!e.tracker().unwrap().requires_network());
}
