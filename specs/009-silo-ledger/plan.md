# Implementation Plan: a durable silo-backed Ledger (v0 slice)

**Branch**: `009-silo-ledger` | **Date**: 2026-09-03 | **Spec**: `specs/009-silo-ledger/spec.md`

## Summary
`Ledger` keeps its `Vec<Step>` as an in-process **read model** and gains an optional
**write-through store** behind a trait `heddle-core` owns:

```rust
pub trait LedgerStore {
    fn append(&mut self, step: &Step) -> Result<()>;
    fn load(&self) -> Result<Vec<Step>>;
}

pub struct Ledger {
    steps: Vec<Step>,
    store: Option<Box<dyn LedgerStore>>,   // None == today's in-memory Ledger
}
```

`Ledger::new()` is unchanged (`store: None`). `Ledger::open(store)` loads the store's rows into
the mirror, so the existing parent/seq derivation continues a reopened chain with no change to
the hashing code at all.

**Exactly one public signature changes: `append` becomes fallible.** `log`, `show` and
`verify_chain` are byte-identical, so read paths stay infallible and borrow-returning and no
consumer's read code is rewritten. Principle V is satisfied *structurally*: `fn hash`, the
parent/seq derivation, `log`, `show` and `verify_chain` are the same lines of code for both
storage shapes; there is no second implementation of the chaining rule that could drift.

`append`'s **ordering invariant** is the load-bearing detail: compute the `Step`, call
`store.append(&step)?`, and **only then** push to `self.steps`. If the store fails, the mirror is
untouched, so the next `append` recomputes the same `seq`/`parent` and the chain never silently
skips a durable step.

A silo is **one SQLite file in its own directory**: `<root>/<silo_id>/ledger.sqlite3`, one
connection per `Silo`, no shared database. Isolation is then a property of the storage *shape*,
not of every present and future query remembering a `WHERE silo = ?` predicate: a cross-silo read
has no expressible form, because there is no handle to the other silo's data.

## Technical Context
**Language/Version**: Rust 1.97 (pinned in `rust-toolchain.toml`, unchanged this slice)
**Primary Dependencies**: `rusqlite = { version = "0.40", default-features = false, features =
["bundled"] }`, confined to `crates/heddle-silo/src/ledger_store.rs`. Dev-only: `tempfile`.
`heddle-core` keeps exactly four dependencies.
**Storage**: SQLite, one file per silo, rollback journal, `PRAGMA synchronous = FULL`
**Testing**: `cargo test`; three seam tests in `heddle-core` against a `VecStore`/failing-store
double, seven file-backed tests in `heddle-silo` over a `tempfile::TempDir` root
**Target Platform**: Windows + macOS + Linux
**Project Type**: library (four workspace members)
**Performance Goals**: N/A
**Constraints**: `heddle-core` may not name a database; `crates/heddle-mcp/` unchanged; no network
**Scale/Scope**: one new crate, one table, one trait with two methods, one new error variant

## Constitution Check
*GATE: must pass before implementation.*
- **I. Headless core**: ✅ library only; the silo is reachable through the existing headless API
  and, via `SessionParts`, through the ACP boundary. No `[[bin]]`, no UI.
- **II. Local-first / silo isolation**: ✅ **this slice is the one that makes Principle II
  testable.** A local file, no network, no server, no external database. Isolation is a property
  of the storage shape and is proved by the dedicated test `s3`.
- **III. Test-First**: ✅ T1 pins the `rusqlite` surface against the vendored source before any
  product code; T3's red is observed and recorded before T4, T5's before T6. `s3` is the
  dedicated isolation test Principle III requires.
- **IV. Inverted coupling**: ✅ `LedgerStore` is the seam. `rusqlite` is named in exactly one
  module of one crate and never in `heddle-core`, whose direct dependency list stays four.
- **V. Traceability**: ✅ **one** hash function, **one** chaining rule, **one** `verify_chain`,
  shared by both storage shapes. Append-only is enforced by SQL triggers rather than by
  convention; `s6` proves tamper-evidence at the row level.
- **VI. Security / deny-by-default**: n/a — no secrets in this slice. Silo ids are validated
  against path traversal before any directory is created.
- **VII. Neutrality / YAGNI**: ✅ one storage shape, one silo layout, two trait methods. No
  `Backend` trait, no `Mode`, no RBAC, no retention policy, no `replay`/`revert`/`branch`.
- **VIII. Loop discipline (NON-NEGOTIABLE)**: ✅ `LoopController` and `ProgressProbe` untouched;
  per-step capture unchanged except that a failed durable write now ends the run loudly.
- **Cross-platform**: ✅ `bundled` SQLite needs no system library; no `#[cfg]` in our code.
  `core.yml`'s `paths:` already covers `crates/**`, so a new crate needs no CI edit — confirmed
  by reading, not edited.

## Project Structure

### Documentation (this feature)
```text
specs/009-silo-ledger/
├── spec.md      # this feature's requirements
├── plan.md      # this file
└── tasks.md     # executable breakdown
```

### Source Code (repository root)
```text
Cargo.toml                        # +rusqlite, +tempfile in [workspace.dependencies]
crates/heddle-core/
  src/error.rs                    # +HeddleError::Storage
  src/ledger.rs                   # +LedgerStore, +Ledger::open, +store field, fallible append
  src/lib.rs                      # re-export LedgerStore
  src/native_loop.rs              # `?` churn; terminate -> Result<LoopRun>
  src/tool.rs                     # `?` churn
  tests/core.rs                   # +3 seam tests; `?` churn
  tests/native_loop.rs            # `?` churn
  tests/tool_gateway.rs           # `?` churn
crates/heddle-silo/
  Cargo.toml                      # new member (picked up by `members = ["crates/*"]`)
  src/lib.rs                      # Silo::open / ledger / ledger_path; id validation
  src/ledger_store.rs             # SqliteLedgerStore — the only module that names rusqlite
  tests/silo_ledger.rs            # s1..s7
crates/heddle-acp/
  src/lib.rs                      # SessionParts.ledger
  tests/acp_session.rs            # two construction sites; `?` churn
```
**Structure Decision**: `crates/heddle-mcp/` is byte-identical to `dev`, so spec 005's suite
remains a live control. `Ledger::new()` is unchanged, so every pre-existing test is a live
control on the in-memory shape.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| **`Ledger::append` changes from `-> String` to `-> Result<String>`**, touching every call site in five passing test/product files (Principle VII: churn in green controls) | A durable store can fail, and an audit trail that drops a step silently is worse than one that refuses loudly (Principle V). It is the *only* public signature that changes; reads stay borrow-returning and infallible, so `project_updates`, `replay_tool_calls` and every read assertion are untouched. | Keeping `append` infallible and latching a poison flag surfaced at `verify_chain`: defers the failure to whenever someone asks, which for an audit log may be never. Making the whole read path fallible so the store is the single source of truth: several times the churn, and it forfeits the ability to run every existing test unchanged against the SQLite ledger. |
| **A fourth workspace crate, and the first C-toolchain build dependency the repo has taken on** (Principle VII) | Principle IV forbids the core naming a database. `heddle-mcp` (rmcp) and `heddle-acp` (ACP) set the precedent twice; `bundled` SQLite is the only shape that works identically on three OSes with no system prerequisite. | `rusqlite` inside `heddle-core` behind a feature: `core.yml` has no `--all-features` leg, so the durable path would be untested on every OS — the identical argument spec 005 made and won. Linking system SQLite: a per-OS install prerequisite, which the tri-OS constraint exists to avoid. |
| **The `Ledger` holds both a mirror and a store — two representations of one chain** | It buys an unchanged read API and, more importantly, one implementation of the hash and the chaining rule. The divergence risk is bounded by the ordering invariant (persist, then mirror) with a dedicated test, and by `s1`/`s7`, which reopen and compare. | Store-only: fallible reads everywhere (above). Mirror-only with a periodic flush: a window in which the durable record is behind the chain, which is the failure Principle V is about. |
| **A silo is a directory and a file, not a `silo` column** (more moving parts on disk) | Constitution II says "airtight" and "NON-NEGOTIABLE". A property enforced by every query remembering a predicate is not airtight; one forgotten `WHERE` leaks the whole journal. With separate files a cross-silo read has no expressible form. | One database with a `silo` column, or one file with `ATTACH`/schema-per-silo: both share a file and a lock, and both keep isolation a matter of query discipline. SQLCipher per-silo keys: real defence-in-depth, but needs key management that does not exist until slice 010, and it is not what makes the isolation test pass. |
