//! Acceptance tests for the silo-backed `LocalTracker` (spec 002 FR-014,
//! design §4.13 `impl LocalTracker`).
//!
//! Like `silo_ledger.rs`, every assertion runs against a real SQLite file under
//! a real temporary directory. The claim under test is "the local tracker is
//! always available, offline, and survives the process that wrote it", and an
//! in-memory stand-in would prove none of the three.

use heddle_core::{HeddleError, NewTask, TaskQuery, TaskStatus, TaskTracker};
use heddle_silo::Silo;
use tempfile::TempDir;

fn silo() -> (TempDir, Silo) {
    let dir = TempDir::new().expect("a temporary directory");
    let silo = Silo::open(dir.path(), "acme").expect("a silo opens");
    (dir, silo)
}

#[test]
fn the_local_tracker_needs_no_network() {
    // Constitution II: the default posture is full local, and the tracker that
    // backs it must be usable with egress off (design §4.13, "toujours
    // disponible"). This is the property that makes it the safe fallback.
    let (_dir, silo) = silo();
    let tracker = silo.tracker().expect("a silo always has a local tracker");

    assert!(!tracker.requires_network());
}

#[test]
fn a_created_task_comes_back_with_the_id_the_tracker_assigned() {
    let (_dir, silo) = silo();
    let mut tracker = silo.tracker().unwrap();

    let id = tracker
        .create(NewTask::new("write the plan").with_link("run-1"))
        .expect("a task is created");

    let tasks = tracker.list(&TaskQuery::all()).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, id);
    assert_eq!(tasks[0].title, "write the plan");
    assert_eq!(
        tasks[0].status,
        TaskStatus::Todo,
        "a task with no stated status starts as Todo"
    );
    assert_eq!(tasks[0].links, vec!["run-1".to_string()]);
}

#[test]
fn ids_are_distinct_across_tasks() {
    let (_dir, silo) = silo();
    let mut tracker = silo.tracker().unwrap();

    let a = tracker.create(NewTask::new("first")).unwrap();
    let b = tracker.create(NewTask::new("second")).unwrap();

    assert_ne!(a, b, "two tasks are two tasks");
}

#[test]
fn update_moves_a_task_to_the_status_it_was_given() {
    let (_dir, silo) = silo();
    let mut tracker = silo.tracker().unwrap();
    let id = tracker.create(NewTask::new("run the tests")).unwrap();

    tracker.update(&id, TaskStatus::InProgress).unwrap();
    tracker.update(&id, TaskStatus::Done).unwrap();

    let tasks = tracker.list(&TaskQuery::all()).unwrap();
    assert_eq!(tasks[0].status, TaskStatus::Done);
}

#[test]
fn updating_to_a_status_a_task_already_holds_is_not_an_error() {
    // The workflow engine re-asserts a status every time a run is resumed onto
    // a node that is still pending. Idempotence here is what keeps that from
    // being a second mechanism.
    let (_dir, silo) = silo();
    let mut tracker = silo.tracker().unwrap();
    let id = tracker.create(NewTask::new("await approval")).unwrap();

    tracker.update(&id, TaskStatus::Blocked).unwrap();
    tracker
        .update(&id, TaskStatus::Blocked)
        .expect("re-asserting a status is a no-op, not a conflict");

    assert_eq!(
        tracker.list(&TaskQuery::all()).unwrap()[0].status,
        TaskStatus::Blocked
    );
}

#[test]
fn updating_a_task_that_does_not_exist_is_refused() {
    let (_dir, silo) = silo();
    let mut tracker = silo.tracker().unwrap();

    let refusal = tracker
        .update(&heddle_core::TaskId::new("no-such-task"), TaskStatus::Done)
        .expect_err("there is nothing to move");
    assert!(
        matches!(refusal, HeddleError::NotFound(_)),
        "expected NotFound, got {refusal:?}"
    );
}

#[test]
fn list_filters_by_status() {
    let (_dir, silo) = silo();
    let mut tracker = silo.tracker().unwrap();
    let done = tracker.create(NewTask::new("packaged")).unwrap();
    tracker.create(NewTask::new("not started")).unwrap();
    tracker.update(&done, TaskStatus::Done).unwrap();

    let finished = tracker
        .list(&TaskQuery::all().with_status(TaskStatus::Done))
        .unwrap();

    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].title, "packaged");
}

#[test]
fn list_filters_by_link() {
    // `links` is how a task says which run it belongs to (spec 002 Key
    // Entities), so filtering on it is how a workflow finds its own tasks
    // without the tracker having to know what a workflow is.
    let (_dir, silo) = silo();
    let mut tracker = silo.tracker().unwrap();
    tracker
        .create(NewTask::new("mine").with_link("run-a"))
        .unwrap();
    tracker
        .create(NewTask::new("someone else's").with_link("run-b"))
        .unwrap();

    let mine = tracker.list(&TaskQuery::all().linked_to("run-a")).unwrap();

    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].title, "mine");
}

#[test]
fn list_returns_tasks_in_creation_order() {
    let (_dir, silo) = silo();
    let mut tracker = silo.tracker().unwrap();
    tracker.create(NewTask::new("first")).unwrap();
    tracker.create(NewTask::new("second")).unwrap();
    tracker.create(NewTask::new("third")).unwrap();

    let titles: Vec<String> = tracker
        .list(&TaskQuery::all())
        .unwrap()
        .into_iter()
        .map(|t| t.title)
        .collect();

    assert_eq!(titles, vec!["first", "second", "third"]);
}

#[test]
fn tasks_outlive_the_connection_that_wrote_them() {
    let dir = TempDir::new().unwrap();
    let id = {
        let silo = Silo::open(dir.path(), "acme").unwrap();
        let mut tracker = silo.tracker().unwrap();
        let id = tracker.create(NewTask::new("survive a restart")).unwrap();
        tracker.update(&id, TaskStatus::InProgress).unwrap();
        id
    };

    let silo = Silo::open(dir.path(), "acme").unwrap();
    let tracker = silo.tracker().unwrap();
    let tasks = tracker.list(&TaskQuery::all()).unwrap();

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, id);
    assert_eq!(tasks[0].status, TaskStatus::InProgress);
}

#[test]
fn two_silos_cannot_see_each_others_tasks() {
    // Constitution II, NON-NEGOTIABLE, restated for this table: isolation is a
    // property of there being no handle to the other silo's file, not of a
    // predicate someone must remember to write.
    let dir = TempDir::new().unwrap();
    let acme = Silo::open(dir.path(), "acme").unwrap();
    let other = Silo::open(dir.path(), "other").unwrap();
    acme.tracker()
        .unwrap()
        .create(NewTask::new("acme's private plan"))
        .unwrap();

    assert!(other
        .tracker()
        .unwrap()
        .list(&TaskQuery::all())
        .unwrap()
        .is_empty());
}

#[test]
fn a_tasks_ledger_and_its_tasks_share_the_silos_one_file() {
    // The isolation argument in `heddle-silo`'s own module docs is stated as
    // "one directory holding one SQLite file". Tasks live in that same file so
    // the sentence stays literally true rather than becoming approximately so.
    let (_dir, silo) = silo();
    silo.tracker()
        .unwrap()
        .create(NewTask::new("a task"))
        .unwrap();

    assert_eq!(silo.store_path(), silo.ledger_path());
    assert!(silo.store_path().exists());
    assert_eq!(
        std::fs::read_dir(silo.store_path().parent().unwrap())
            .unwrap()
            .count(),
        1,
        "a silo directory still holds exactly one file"
    );
}
