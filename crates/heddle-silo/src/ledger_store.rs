//! The only module in the product that names SQLite.
//!
//! `heddle-core` reaches durable storage through `LedgerStore` and never learns
//! what is behind it (Constitution IV), exactly as it reaches MCP through
//! `ToolTransport` and ACP through nothing at all.

use heddle_core::{HeddleError, LedgerStore, Result, Step, StepKind};
use rusqlite::{params, Connection};
use std::path::Path;

/// Append order is `ord`, the only `ORDER BY`: `seq` restarts per run, so it
/// cannot order a file that holds several runs.
///
/// `kind` is stored as the same serde string the hash function is fed, so there
/// is no second name mapping that could drift from the hashed bytes.
const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS ledger_step (
  ord     INTEGER PRIMARY KEY AUTOINCREMENT,
  id      TEXT NOT NULL UNIQUE,
  parent  TEXT,
  seq     INTEGER NOT NULL,
  run_id  TEXT NOT NULL,
  kind    TEXT NOT NULL,
  payload TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS ledger_step_run ON ledger_step(run_id, ord);

CREATE TRIGGER IF NOT EXISTS ledger_step_no_update BEFORE UPDATE ON ledger_step
  BEGIN SELECT RAISE(ABORT, 'ledger is append-only'); END;
CREATE TRIGGER IF NOT EXISTS ledger_step_no_delete BEFORE DELETE ON ledger_step
  BEGIN SELECT RAISE(ABORT, 'ledger is append-only'); END;
";

const SELECT_ALL: &str =
    "SELECT id, parent, seq, run_id, kind, payload FROM ledger_step ORDER BY ord";

const INSERT: &str = "\
INSERT INTO ledger_step (id, parent, seq, run_id, kind, payload) VALUES (?1, ?2, ?3, ?4, ?5, ?6)";

pub struct SqliteLedgerStore {
    conn: Connection,
}

impl SqliteLedgerStore {
    /// Opens (creating if needed) the ledger file at `path` and applies the
    /// schema idempotently.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path.as_ref()).map_err(storage)?;
        // An audit log is single-writer and wants durability over throughput,
        // and the default rollback journal keeps a silo to one file — which is
        // what the isolation argument rests on. WAL would add -wal/-shm.
        conn.pragma_update(None, "synchronous", "FULL")
            .map_err(storage)?;
        conn.execute_batch(SCHEMA).map_err(storage)?;
        Ok(SqliteLedgerStore { conn })
    }
}

impl LedgerStore for SqliteLedgerStore {
    fn append(&mut self, step: &Step) -> Result<()> {
        let kind = serde_json::to_string(&step.kind)?;
        self.conn
            .execute(
                INSERT,
                params![
                    step.id,
                    step.parent,
                    // rusqlite binds i8..=i64 and u8..=u32, not u64.
                    seq_to_sql(step.seq)?,
                    step.run_id,
                    kind,
                    step.payload
                ],
            )
            .map_err(storage)?;
        Ok(())
    }

    fn load(&self) -> Result<Vec<Step>> {
        let mut stmt = self.conn.prepare(SELECT_ALL).map_err(storage)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(storage)?;

        let mut steps = Vec::new();
        for row in rows {
            let (id, parent, seq, run_id, kind, payload) = row.map_err(storage)?;
            steps.push(Step {
                id,
                parent,
                seq: seq_from_sql(seq)?,
                run_id,
                kind: serde_json::from_str::<StepKind>(&kind)?,
                payload,
            });
        }
        Ok(steps)
    }
}

fn seq_to_sql(seq: u64) -> Result<i64> {
    i64::try_from(seq).map_err(|_| HeddleError::Storage(format!("step seq {seq} exceeds i64")))
}

fn seq_from_sql(seq: i64) -> Result<u64> {
    u64::try_from(seq).map_err(|_| HeddleError::Storage(format!("stored seq {seq} is negative")))
}

fn storage(error: rusqlite::Error) -> HeddleError {
    HeddleError::Storage(error.to_string())
}
