//! `plan.md` D1: the `Node` enum names `spec.md`'s full node vocabulary from the
//! first slice, and the four kinds this slice does not execute refuse **before**
//! anything is logged for them.
//!
//! Both halves matter, and the second is the one worth a test. A refusal that
//! first appended a completion step would make the chain claim the node had run,
//! and the next slice's engine would then *skip* the very node it had just
//! learned how to execute. Failing before the append is what makes a retry after
//! that slice lands safe, which the last test here demonstrates rather than
//! asserts in prose.

mod common;

use common::{completed_nodes, engine, says, snapshot, RecordingTransport, ScriptedModel};
use skein_core::{Ledger, Message, SkeinError, StepKind};
use skein_workflow::{Node, Workflow, WorkflowExit};

fn workflow_ending_in(deferred: Node) -> Workflow {
    Workflow::new(
        "reaches-a-deferred-kind",
        vec![
            Node::Agent {
                id: "first".into(),
                prompt: Message::user_text("do the implemented thing"),
            },
            deferred,
        ],
    )
}

fn deferred_kinds() -> Vec<(Node, &'static str)> {
    vec![
        (
            Node::Loop {
                id: "refine".into(),
                body: "self-refine".into(),
            },
            "loop",
        ),
        (
            Node::Subagent {
                id: "refine".into(),
                workflow: "inner".into(),
            },
            "subagent",
        ),
        (
            Node::Condition {
                id: "refine".into(),
                on: "tests_pass".into(),
            },
            "condition",
        ),
        (
            Node::Parallel {
                id: "refine".into(),
                branches: vec!["a".into(), "b".into()],
            },
            "parallel",
        ),
    ]
}

#[test]
fn every_deferred_node_kind_refuses_and_names_itself() {
    for (node, kind) in deferred_kinds() {
        let mut e = engine(
            ScriptedModel::new(vec![says("done")]),
            RecordingTransport::forbidden(),
        );
        let mut ledger = Ledger::new();

        let err = e
            .run("run-deferred", &workflow_ending_in(node), &mut ledger)
            .expect_err("a node kind this build cannot execute must not silently succeed");

        let SkeinError::Unsupported(detail) = &err else {
            panic!("expected Unsupported for a {kind} node, got {err:?}");
        };
        assert!(
            detail.contains(kind),
            "the refusal must name the kind so an operator knows what is missing: {detail}"
        );
        assert!(
            detail.contains("refine"),
            "and the node, so they know where the workflow stopped: {detail}"
        );
    }
}

#[test]
fn a_deferred_node_logs_nothing_at_all_for_itself() {
    let mut e = engine(
        ScriptedModel::new(vec![says("done")]),
        RecordingTransport::forbidden(),
    );
    let mut ledger = Ledger::new();

    e.run(
        "run-deferred",
        &workflow_ending_in(Node::Loop {
            id: "refine".into(),
            body: "self-refine".into(),
        }),
        &mut ledger,
    )
    .expect_err("the run refuses");

    assert_eq!(
        completed_nodes(&ledger, "run-deferred"),
        vec!["first"],
        "the node that ran is recorded; the node that refused is not"
    );
    assert!(
        !ledger
            .log("run-deferred")
            .iter()
            .any(|s| s.kind == StepKind::Approval),
        "and a refusal is not an approval question either"
    );
    assert!(
        ledger
            .log("run-deferred")
            .iter()
            .all(|s| !s.payload.contains("refine")),
        "nothing on the chain mentions the node at all, so no later reader can \
         mistake it for work that happened"
    );
}

#[test]
fn the_work_done_before_the_refusal_is_kept() {
    // A refusal must not cost the caller the nodes that did succeed — otherwise
    // hitting a deferred kind would mean re-running everything before it.
    let mut e = engine(
        ScriptedModel::new(vec![says("done")]),
        RecordingTransport::forbidden(),
    );
    let mut ledger = Ledger::new();

    e.run(
        "run-deferred",
        &workflow_ending_in(Node::Loop {
            id: "refine".into(),
            body: "self-refine".into(),
        }),
        &mut ledger,
    )
    .expect_err("the run refuses");

    assert_eq!(e.client.calls, 1, "node 1 really ran");
    ledger
        .verify_chain("run-deferred")
        .expect("a refused run still leaves an intact chain");
}

#[test]
fn refusing_is_stable_and_appends_nothing_on_a_retry() {
    let mut ledger = Ledger::new();
    let workflow = workflow_ending_in(Node::Loop {
        id: "refine".into(),
        body: "self-refine".into(),
    });

    engine(
        ScriptedModel::new(vec![says("done")]),
        RecordingTransport::forbidden(),
    )
    .run("run-deferred", &workflow, &mut ledger)
    .expect_err("the run refuses");
    let after_first = snapshot(&ledger, "run-deferred");

    // Retrying against the same build refuses again — with a model that would
    // panic if node 1 were re-executed, so this also shows the completed node is
    // still skipped on the way to the refusal.
    engine(ScriptedModel::forbidden(), RecordingTransport::forbidden())
        .run("run-deferred", &workflow, &mut ledger)
        .expect_err("still refused");

    assert_eq!(
        snapshot(&ledger, "run-deferred"),
        after_first,
        "a build that cannot do the work must not keep writing about it"
    );
}

/// D1's actual payoff, demonstrated end to end: because the refusal logged
/// nothing, the run resumes at the deferred node the moment a build knows how to
/// execute it — with the work before it still skipped.
///
/// The "future slice" is stood in for by the same node id arriving as a kind
/// this build *can* execute, which is what the chain sees either way: the chain
/// records node ids and outcomes, not node kinds.
#[test]
fn a_build_that_implements_the_kind_resumes_at_that_very_node() {
    let mut ledger = Ledger::new();

    engine(
        ScriptedModel::new(vec![says("done")]),
        RecordingTransport::forbidden(),
    )
    .run(
        "run-deferred",
        &workflow_ending_in(Node::Loop {
            id: "refine".into(),
            body: "self-refine".into(),
        }),
        &mut ledger,
    )
    .expect_err("today's build refuses");

    let mut tomorrow = engine(
        ScriptedModel::new(vec![says("refined")]),
        RecordingTransport::forbidden(),
    );
    let run = tomorrow
        .run(
            "run-deferred",
            &workflow_ending_in(Node::Agent {
                id: "refine".into(),
                prompt: Message::user_text("refine it"),
            }),
            &mut ledger,
        )
        .expect("a build that implements the kind completes the workflow");

    assert_eq!(run.exit, WorkflowExit::Completed);
    assert_eq!(run.final_outcome.as_deref(), Some("refined"));
    assert_eq!(
        tomorrow.client.calls, 1,
        "node 1 was skipped; only the previously-refused node ran"
    );
    assert_eq!(
        completed_nodes(&ledger, "run-deferred"),
        vec!["first", "refine"],
        "and each node still has exactly one completion step"
    );
}
