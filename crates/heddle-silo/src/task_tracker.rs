//! The silo-backed `TaskTracker` (design §4.13 `impl LocalTracker`, spec 002
//! FR-014) — the tracker that is always available.
//!
//! "Always available" is the whole point of this backend and it is a *build*
//! property, not a promise: [`LocalTracker::requires_network`] returns `false`
//! because there is no code path here that could open a socket. Under Local mode
//! with egress off (§7.3), a Vikunja server or a Jira cloud tenant is refused
//! before it is reached; this one keeps working, so a workflow that reports
//! progress always has somewhere to report it (Constitution II).
//!
//! The tasks live in **the silo's one SQLite file**, beside `ledger_step`. A
//! second file would have quietly falsified this crate's own isolation argument
//! — "one directory holding one SQLite file" — and the argument is what
//! Constitution II rests on, so it is worth a shared file to keep it literally
//! true rather than approximately so.
//!
//! Unlike `ledger_step`, `task` has no append-only trigger, and the difference
//! is deliberate: a Ledger is a record of what happened and must never change,
//! while a task board is a record of where things *stand* and exists to be
//! moved. The audit trail of the moving is the Ledger's, not this table's.

use heddle_core::{HeddleError, NewTask, Result, Task, TaskId, TaskQuery, TaskStatus, TaskTracker};
use rusqlite::{params, Connection};
use std::path::Path;

/// `links` is a JSON array in one column rather than a join table. The tracker
/// only ever reads them whole and filters with a membership test, so a second
/// table would buy a query shape nothing asks for — and `serde_json` is already
/// this crate's dependency for the Ledger's payloads.
const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS task (
  id     INTEGER PRIMARY KEY AUTOINCREMENT,
  title  TEXT NOT NULL,
  status TEXT NOT NULL,
  links  TEXT NOT NULL
);
";

const INSERT: &str = "INSERT INTO task (title, status, links) VALUES (?1, ?2, ?3)";

const UPDATE_STATUS: &str = "UPDATE task SET status = ?2 WHERE id = ?1";

/// `id` is the `AUTOINCREMENT` key, so ordering by it is creation order — the
/// same reasoning `ledger_step` uses for `ord`, and for the same reason it is
/// the only `ORDER BY` here.
const SELECT_ALL: &str = "SELECT id, title, status, links FROM task ORDER BY id";

/// A task board in one silo's SQLite file.
pub struct LocalTracker {
    conn: Connection,
}

impl LocalTracker {
    /// Opens (creating if needed) the task table in the database at `path` and
    /// applies the schema idempotently.
    ///
    /// `path` is the silo's one file, which a [`SqliteLedgerStore`] normally
    /// also holds open. Two connections to one file is why `busy_timeout` is
    /// set: the default is to fail instantly on a locked database, which would
    /// turn an ordinary interleaving of a Ledger append and a task update into
    /// a spurious error.
    ///
    /// [`SqliteLedgerStore`]: crate::SqliteLedgerStore
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path.as_ref()).map_err(storage)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(storage)?;
        // `SqliteLedgerStore`'s reasoning, unchanged: durability over
        // throughput, and one file per silo rather than the -wal/-shm pair WAL
        // would add.
        conn.pragma_update(None, "synchronous", "FULL")
            .map_err(storage)?;
        conn.execute_batch(SCHEMA).map_err(storage)?;
        Ok(LocalTracker { conn })
    }
}

impl TaskTracker for LocalTracker {
    fn create(&mut self, task: NewTask) -> Result<TaskId> {
        let links = serde_json::to_string(&task.links)?;
        self.conn
            .execute(INSERT, params![task.title, task.status.as_str(), links])
            .map_err(storage)?;
        Ok(TaskId::new(self.conn.last_insert_rowid().to_string()))
    }

    fn update(&mut self, id: &TaskId, status: TaskStatus) -> Result<()> {
        // A row count of zero is the only way this backend can tell "no such
        // task" from "already had that status" — an `UPDATE` that matches a row
        // and changes nothing still reports one row affected. Without the
        // distinction, a typo'd id would be indistinguishable from a no-op,
        // which is what `TaskTracker::update`'s idempotence promise would then
        // be hiding.
        let changed = self
            .conn
            .execute(UPDATE_STATUS, params![id.as_str(), status.as_str()])
            .map_err(storage)?;
        if changed == 0 {
            return Err(HeddleError::NotFound(format!("task {id}")));
        }
        Ok(())
    }

    fn list(&self, query: &TaskQuery) -> Result<Vec<Task>> {
        // Selected whole and filtered in Rust through `TaskQuery::matches`,
        // rather than compiled into a `WHERE`. A silo's board is one local
        // user's, so the row count is small; and one definition of what a filter
        // means — shared with every other backend — is worth more here than a
        // second one written in SQL that could drift from it.
        let mut stmt = self.conn.prepare(SELECT_ALL).map_err(storage)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(storage)?;

        let mut tasks = Vec::new();
        for row in rows {
            let (id, title, status, links) = row.map_err(storage)?;
            let task = Task {
                id: TaskId::new(id.to_string()),
                title,
                status: TaskStatus::parse(&status).ok_or_else(|| {
                    HeddleError::Storage(format!("task {id} holds unknown status {status:?}"))
                })?,
                links: serde_json::from_str(&links)?,
            };
            if query.matches(&task) {
                tasks.push(task);
            }
        }
        Ok(tasks)
    }

    /// Never. That is this backend's reason to exist.
    fn requires_network(&self) -> bool {
        false
    }
}

fn storage(error: rusqlite::Error) -> HeddleError {
    HeddleError::Storage(error.to_string())
}
