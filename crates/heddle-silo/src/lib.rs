//! The local silo backend (design §4.8 `EmbeddedBackend`, §7.9).
//!
//! A silo is **one directory holding one SQLite file**, not a row-level tenancy
//! predicate. That is the whole isolation argument (Constitution II, airtight,
//! NON-NEGOTIABLE): with a shared database, isolation would depend on every
//! present and future query remembering a `WHERE silo = ?`, and one forgotten
//! predicate leaks the journal. With a file each, a cross-silo read is not
//! merely forbidden — it has no expressible form, because there is no handle to
//! the other silo's data.
//!
//! This is also the only crate in the product that names `rusqlite` or a
//! credential store, so `heddle-core` discovers durable storage through
//! `LedgerStore` and secrets through `SecretProvider`, never through a database
//! type or an OS API.

mod ledger_store;
mod secret;

pub use ledger_store::SqliteLedgerStore;
pub use secret::OsKeychain;

use heddle_core::{HeddleError, Ledger, Result};
use std::path::{Path, PathBuf};

const LEDGER_FILE: &str = "ledger.sqlite3";

/// One silo's local storage, rooted at `<root>/<id>`.
pub struct Silo {
    dir: PathBuf,
}

impl Silo {
    /// Opens (creating if needed) the silo `id` under `root`.
    ///
    /// The id is validated *before* any path is joined, so a silo can never
    /// address anything outside `root`.
    pub fn open(root: impl AsRef<Path>, id: &str) -> Result<Silo> {
        let dir = root.as_ref().join(validated(id)?);
        std::fs::create_dir_all(&dir)
            .map_err(|e| HeddleError::Storage(format!("silo {id}: {e}")))?;
        Ok(Silo { dir })
    }

    /// A `Ledger` backed by this silo's file, resuming everything already in it.
    /// The caller gets the same `Ledger` type `NativeLoop` and `ToolGateway`
    /// already take, so nothing downstream knows a database is involved.
    pub fn ledger(&self) -> Result<Ledger> {
        Ledger::open(Box::new(SqliteLedgerStore::open(self.ledger_path())?))
    }

    pub fn ledger_path(&self) -> PathBuf {
        self.dir.join(LEDGER_FILE)
    }
}

/// A silo id is a single path component of `[A-Za-z0-9._-]`, and never `.` or
/// `..`. Rejecting the separators outright is what makes the containment a
/// property of the id rather than of the join that follows it.
fn validated(id: &str) -> Result<&str> {
    let refuse = |why: &str| Err(HeddleError::Storage(format!("silo id {id:?} {why}")));
    if id.is_empty() {
        return refuse("is empty");
    }
    if id == "." || id == ".." {
        return refuse("is a directory traversal");
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return refuse("must be ASCII alphanumeric, '.', '_' or '-'");
    }
    Ok(id)
}
