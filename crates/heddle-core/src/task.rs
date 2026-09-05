//! Task tracking (design §4.13, spec 002 FR-014): the core creates and moves
//! tasks through a trait and never learns which tracker is behind it.
//!
//! This module is a **port**, exactly as [`LedgerStore`] and [`SecretProvider`]
//! are: `heddle-core` gains no dependency from it, and the three backends design
//! §4.13 names — silo-local, Vikunja, Jira over MCP — are all somebody else's
//! crate. That is why nothing here spells any of those three names, not even in
//! an enum: which tracker is active is resolved as an opaque *name* through
//! [`Hierarchy`](crate::Hierarchy), and turning that name into an implementation
//! is the host's job (Constitution IV).
//!
//! **One deviation from the design sketch, stated rather than hidden.** §4.13
//! writes `fn create(&self, t: Task) -> TaskId`, which asks the caller to build a
//! `Task` that already has the `TaskId` the call is about to return. A Jira
//! backend assigns `PROJ-123` itself and a local one assigns a row id, so the
//! caller cannot supply it. [`NewTask`] is that same argument with the field the
//! caller cannot know removed; [`Task`] is what comes back.
//!
//! [`LedgerStore`]: crate::LedgerStore
//! [`SecretProvider`]: crate::SecretProvider

use crate::error::Result;
use serde::{Deserialize, Serialize};

/// A tracker-assigned identifier. Opaque on purpose: `42` from the local
/// tracker and `PROJ-123` from Jira are both just strings to everything above
/// the backend that minted them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(String);

impl TaskId {
    pub fn new(id: impl Into<String>) -> Self {
        TaskId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a task stands.
///
/// Five states rather than the two a workflow strictly needs, because the two a
/// workflow needs are not the ones a *human* reading the board needs:
/// `Blocked` is what an `Approval` node waiting on a person looks like, and
/// `Cancelled` is what a person saying no looks like. Collapsing either into
/// `Todo` would make a run that is waiting for its reader indistinguishable from
/// one that has not started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Todo,
    InProgress,
    /// Waiting on something outside the run — a human decision, most often.
    Blocked,
    Done,
    /// Closed without being finished. Distinct from `Done` so "the release was
    /// refused" does not read as "the release shipped".
    Cancelled,
}

impl TaskStatus {
    /// The wire name, which is also what a backend stores. Taken from serde so
    /// there is exactly one spelling of each state in the product.
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Todo => "todo",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Blocked => "blocked",
            TaskStatus::Done => "done",
            TaskStatus::Cancelled => "cancelled",
        }
    }

    /// The inverse of [`TaskStatus::as_str`], for a backend reading its own
    /// storage back.
    pub fn parse(s: &str) -> Option<TaskStatus> {
        match s {
            "todo" => Some(TaskStatus::Todo),
            "in_progress" => Some(TaskStatus::InProgress),
            "blocked" => Some(TaskStatus::Blocked),
            "done" => Some(TaskStatus::Done),
            "cancelled" => Some(TaskStatus::Cancelled),
            _ => None,
        }
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Spec 002's Key Entity, verbatim: a tracking unit `{id, title, status, links}`.
///
/// `links` is free-form and backend-agnostic — the workflow engine puts the run
/// id and the node id in it, which is what lets a caller ask a tracker for "this
/// run's tasks" without the tracker knowing what a run is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub status: TaskStatus,
    pub links: Vec<String>,
}

/// A [`Task`] minus the one field only the backend can fill in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewTask {
    pub title: String,
    pub status: TaskStatus,
    pub links: Vec<String>,
}

impl NewTask {
    /// A task nobody has started yet.
    pub fn new(title: impl Into<String>) -> Self {
        NewTask {
            title: title.into(),
            status: TaskStatus::Todo,
            links: Vec::new(),
        }
    }

    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_link(mut self, link: impl Into<String>) -> Self {
        self.links.push(link.into());
        self
    }

    /// What a backend returns once it has assigned an id. Written here rather
    /// than in each backend so the two types cannot drift apart field by field.
    pub fn into_task(self, id: TaskId) -> Task {
        Task {
            id,
            title: self.title,
            status: self.status,
            links: self.links,
        }
    }
}

/// A filter over a tracker's tasks. Design §4.13's `Query`.
///
/// Every field is an *optional* narrowing, so [`TaskQuery::all`] is the identity
/// and a backend that receives an empty query returns everything. Keeping it a
/// value rather than a string spares each backend a query-language parser it
/// would have to keep compatible with the others.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskQuery {
    pub status: Option<TaskStatus>,
    pub link: Option<String>,
}

impl TaskQuery {
    pub fn all() -> Self {
        TaskQuery::default()
    }

    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn linked_to(mut self, link: impl Into<String>) -> Self {
        self.link = Some(link.into());
        self
    }

    /// Whether `task` satisfies this query.
    ///
    /// Lives here so an in-memory backend and a SQL one agree on what a filter
    /// *means* even though only one of them can push it into a `WHERE` clause —
    /// the SQL backend uses this to check what it selected, not instead of
    /// selecting.
    pub fn matches(&self, task: &Task) -> bool {
        if let Some(status) = &self.status {
            if &task.status != status {
                return false;
            }
        }
        if let Some(link) = &self.link {
            if !task.links.iter().any(|l| l == link) {
                return false;
            }
        }
        true
    }
}

/// How the core creates and moves tasks (design §4.13).
///
/// `create` and `update` take `&mut self` for [`LedgerStore::append`]'s reason:
/// a backend may hold a connection or a session, and a trait that promised
/// `&self` would push every one of them into interior mutability for no gain.
/// `list` does not, because reading is genuinely read-only.
///
/// [`LedgerStore::append`]: crate::LedgerStore::append
pub trait TaskTracker {
    /// Open a task; the returned id is the backend's, not the caller's.
    fn create(&mut self, task: NewTask) -> Result<TaskId>;

    /// Move an existing task. Setting the status a task already holds is a
    /// no-op rather than a conflict — the workflow engine re-asserts a status
    /// every time a run is resumed onto a node that is still pending, and an
    /// error there would turn an ordinary poll into a failure.
    fn update(&mut self, id: &TaskId, status: TaskStatus) -> Result<()>;

    fn list(&self, query: &TaskQuery) -> Result<Vec<Task>>;

    /// Governs availability under the egress policy (design §7.3), exactly as
    /// [`SecretProvider::requires_network`] does: in Local mode with egress OFF,
    /// only offline trackers are usable — which is what makes the silo-backed
    /// one the always-available default.
    ///
    /// [`SecretProvider::requires_network`]: crate::SecretProvider::requires_network
    fn requires_network(&self) -> bool;
}

/// The type that names "this engine tracks nothing".
///
/// Uninhabited on purpose. A no-op tracker would be a real value whose `create`
/// silently discarded a task, and the bug that produces — a run that reports
/// progress nobody can see — is exactly the one worth making unrepresentable.
/// Because no value of this type can exist, an `Option<NoTracker>` is `None` by
/// construction, and every method below is provably unreachable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoTracker {}

impl TaskTracker for NoTracker {
    fn create(&mut self, _task: NewTask) -> Result<TaskId> {
        match *self {}
    }

    fn update(&mut self, _id: &TaskId, _status: TaskStatus) -> Result<()> {
        match *self {}
    }

    fn list(&self, _query: &TaskQuery) -> Result<Vec<Task>> {
        match *self {}
    }

    fn requires_network(&self) -> bool {
        match *self {}
    }
}
