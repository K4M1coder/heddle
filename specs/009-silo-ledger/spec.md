# Feature Specification: a durable silo-backed Ledger (v0 slice)

**Feature Branch:** `009-silo-ledger` · **Created:** 2026-09-03 · **Status:** Draft
**Input:** `specs/003-skein-core-foundation/tasks.md` "Next slice" — *"silo-backed durable Ledger
(SQLite)"*, carried unticked through slices 004–007 · Constitution II (**airtight silos**,
NON-NEGOTIABLE), III (**a dedicated isolation test guards each silo invariant**), IV (**the core
never names a backend**), V (**append-only, hash-chained, tamper-evident**) · ADR-0004 D3 (silo
Local + Ledger foundation is in v0 build scope) · design §4.8/§4.11/§7.9.

Six merged slices built a governed, ACP-reachable agentic loop whose entire audit trail is a
`Vec<Step>` in process memory. `crates/skein-core/src/ledger.rs` said so in its own module doc:
*"v0 is in-memory; a durable silo-backed store lands with persistence."*

Principle V demands a Ledger that is "inspectable, replayable, reversible… cannot be bypassed".
None of those words survives a process exit today. Principle II demands airtight silos, and the
product has exactly one — the in-process `Vec` — so Principle III's dedicated isolation test has
no boundary to guard.

This slice gives the chain a durable home: **one SQLite file per silo, in its own directory**,
reached through a `LedgerStore` seam that `skein-core` owns and a new crate `skein-silo`
implements. `Ledger` keeps its `Vec<Step>` as a read model, so the hash function, the chaining
rule and `verify_chain` stay *one* implementation shared by both storage shapes.

## User Scenarios & Testing

### User Story 1 — The chain survives the process (P1)
As an auditor, a run I recorded yesterday is still on the chain, still verifiable, today.
**Acceptance:**
1. **Given** a silo opened under a directory root, **When** four steps are appended for `run-1`,
   the `Ledger` is dropped (closing the connection), and the silo's ledger is opened again,
   **Then** `log("run-1")` holds the same four steps with the same ids and payloads, and
   `verify_chain("run-1")` is `Ok`.
2. **Given** that reopened chain, **When** a fifth step is appended, **Then** its `seq` is `4`
   and its `parent` is the pre-close last id — the chain *continues* rather than restarting.

### User Story 2 — A silo is airtight (P1)
As an operator, nothing written in one silo is reachable from another.
**Acceptance:**
1. **Given** two silos `alpha` and `beta` under one root, **When** a step is appended in `alpha`,
   **Then** `beta`'s ledger has `log(run)` empty and `show(alpha_step_id)` is `Err(NotFound)`,
   and the two silos' ledger paths differ and are separate files on disk.
2. **Given** a silo id that would escape the root (`../evil`, `..`, `a/b`) or is empty,
   **When** `Silo::open` is called, **Then** it returns `Err(SkeinError::Storage)` and creates
   nothing outside the root.

### User Story 3 — Append-only is enforced by the engine, not by convention (P1)
As an auditor, I want the storage itself to refuse a rewrite.
**Acceptance:**
1. **Given** a silo's ledger file, **When** a raw `UPDATE` or `DELETE` is issued against
   `ledger_step`, **Then** both fail with `ledger is append-only`.
2. **Given** an attacker with raw file access who drops the update trigger and forges a payload,
   **When** the `Ledger` is reopened, **Then** `verify_chain` returns
   `SkeinError::LedgerIntegrity`. This is tamper-**evidence**, not tamper-**proofing**: a local
   writer with the file can always drop a trigger, which is exactly why the hash chain exists.

### User Story 4 — No existing consumer needed adapting (P1)
As a maintainer, the durable Ledger is the same `Ledger` the loop and the gateway already take.
**Acceptance:**
1. **Given** a `NativeLoop` with scripted doubles and an allowlisted read-only tool, **When** it
   runs against a silo-backed `Ledger`, the ledger is dropped and the silo reopened, **Then** the
   reopened chain holds the same `StepKind` sequence and `verify_chain` passes.
2. **Given** an ACP session, **When** the operator injects a `Ledger` through `SessionParts`,
   **Then** the session's runs land in that chain — the durable ledger is reachable from the
   product's only client boundary.

### User Story 5 — A lost durable write is loud, never silent (P1)
As an auditor, an audit trail that drops a step is worse than one that refuses.
**Acceptance:**
1. **Given** a store whose `append` fails, **When** `Ledger::append` is called, **Then** it
   returns `Err`, the in-memory mirror is unmoved (`log(run)` is empty), and a later append
   against a healthy store still gets `seq == 0`.

## Requirements
- **FR-001**: `skein-core` MUST define the durable seam (`LedgerStore`) and MUST NOT name
  SQLite, `rusqlite`, or any database (Constitution IV). Its direct dependency list stays four.
- **FR-002**: A new workspace crate `skein-silo` MUST be the only crate that names `rusqlite`,
  and within it `rusqlite` MUST be named in exactly one module.
- **FR-003**: `Ledger::append` MUST persist to the store **before** mirroring in memory, so a
  store failure leaves `seq`/`parent` derivation untouched.
- **FR-004**: `Ledger::append` is the **only** public signature that changes (`-> String` becomes
  `-> Result<String>`). `log`, `show` and `verify_chain` MUST stay byte-identical, so reads stay
  infallible and borrow-returning.
- **FR-005**: There MUST be exactly one hash function, one parent/seq derivation and one
  `verify_chain`, shared by the in-memory and the durable shape (Constitution V).
- **FR-006**: A silo MUST be one SQLite file in its own directory,
  `<root>/<silo_id>/ledger.sqlite3`. No shared file, no `silo` column, no `ATTACH`.
- **FR-007**: A silo id MUST be validated on open: non-empty and `[A-Za-z0-9._-]+` with `.` and
  `..` rejected, so it can never escape `<root>`. An invalid id fails with `SkeinError::Storage`.
- **FR-008**: The schema MUST forbid `UPDATE` and `DELETE` on `ledger_step` with SQL triggers.
- **FR-009**: `StepKind` MUST be stored with the same serde representation the hash function
  feeds, so no second name mapping can drift from the hashed bytes.
- **FR-010**: The store MUST require no system SQLite: `rusqlite` is taken with
  `default-features = false, features = ["bundled"]` so all three OSes build identically.
- **FR-011**: `skein-acp`'s `SessionParts` MUST accept an injected `Ledger`, so a durable ledger
  is reachable from the product's client boundary.

## Success Criteria
- **SC-001**: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`
  and `cargo test --workspace` all clean; the suite is 52 pre-existing + 10 new = **62** tests
  (2026-09-03).
- **SC-002**: The persistence acceptance closes a real connection and reopens a real file. No
  in-memory stand-in for the durable path.
- **SC-003**: `git diff dev -- crates/skein-mcp/` is empty.
- **SC-004**: `git diff dev -- spikes/ .github/ rust-toolchain.toml` is empty.
- **SC-005**: `git diff dev -- Cargo.toml` shows only added `[workspace.dependencies]` entries.
- **SC-006**: Every pre-existing test still passes, its body unchanged except for the mechanical
  `?`/`unwrap` on `append` — the strongest available evidence that the chaining semantics did not
  move.
- As in specs 004–008, the macOS and Linux legs of `core.yml` are unobserved until the repository
  has a remote; only the Windows leg is run locally.

## Assumptions
- **A silo's whole history is mirrored in RAM.** `Ledger::open` loads every row. Bounded/paged
  reads are deferred; the `LedgerStore` seam already admits them when a caller needs one.
- **Rollback journal, not WAL.** `PRAGMA synchronous = FULL` and SQLite's default journal mode. A
  single-writer audit log wants durability over throughput, and WAL's `-wal`/`-shm` sidecars
  muddy the one-file-per-silo story the isolation argument rests on. Verified: a file-backed
  connection reports `journal_mode = delete` and produces exactly one file.
- **`seq` crosses the SQL boundary as `i64`.** `rusqlite` implements `ToSql`/`FromSql` for
  `i8..i64` and `u8..u32`, not `u64`. `Step::seq` stays `u64` in the type system; the store
  converts, and a stored `seq` that does not fit `u64` is a `SkeinError::Storage`, not a panic.
- **`tamper_payload_for_test` stays mirror-only.** It cannot tamper a database row, and
  pretending otherwise would be dishonest. Row-level tamper-evidence has its own test, which
  forges the row through raw SQL.
- **`Step` gains no fields.** design §4.11's `ts`/`principal`/`silo` would change the hashed
  content, so "the same hash function" would stop being literally true. A silo is a property of
  *which store you opened*, not of a row. `principal` has no producer until identity exists.
- **The durable ledger is opt-in.** `Ledger::new()` is unchanged and still in-memory, so every
  pre-existing test remains a live control on this slice.
