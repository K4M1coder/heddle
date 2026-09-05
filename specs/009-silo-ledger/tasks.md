# Tasks: a durable silo-backed Ledger (v0 slice)

**Spec:** `specs/009-silo-ledger/spec.md` · TDD (red→green), product code in `crates/heddle-core`
and the new `crates/heddle-silo`, branch `009-silo-ledger` cut from `dev`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ (library only; the silo is reachable through the existing headless API and,
  via `SessionParts`, through the ACP boundary) · II Local-first ✅ **this slice is the one that
  makes Principle II testable**: a local file, no network, no server, no external database;
  isolation is a property of the storage shape and is proved by `s3`
- III Test-First ✅ (T1 pins the `rusqlite` surface against the vendored source before any
  product code; T3's red observed before T4, T5's before T6; `s3` is the dedicated isolation test
  Principle III requires) · IV Inverted coupling ✅ (`LedgerStore` is the seam; `rusqlite` is
  named in exactly one module of one crate and never in `heddle-core`, whose direct dependency
  list stays four)
- V Traceability ✅ (**one** hash function, **one** chaining rule, **one** `verify_chain`, shared
  by both storage shapes; append-only enforced by SQL triggers, not by convention; `s6` proves
  row-level tamper-evidence)
- VI Security n/a *(no secrets in this slice — 010's)*; silo ids are validated against path
  traversal before any directory is created (`s4`)
- VII Neutrality ✅ (one storage shape, one silo layout, two trait methods; no `Backend` trait,
  no `Mode`, no RBAC, no retention policy, no `replay`/`revert`/`branch`)
- VIII Loop discipline ✅ (`LoopController` and `ProgressProbe` untouched; per-step capture
  unchanged except that a failed durable write now ends the run loudly instead of silently
  dropping a step)
- Cross-platform ✅ (`bundled` SQLite needs no system library; no `#[cfg]` in our code.
  `core.yml`'s `paths:` already covers `crates/**` at 1.97 — confirmed by reading, not edited).

## Tasks
- [x] **T0** `specs/009-silo-ledger/{spec.md,plan.md,tasks.md}`; branch `009-silo-ledger` cut
      from `dev`
- [x] **T1** pinned the `rusqlite` surface against the vendored `0.40.2` source *before* writing
      product code, and proved `bundled` builds on this Windows host. Two of the assumed
      spellings were wrong; see below
- [x] **T2** control baseline: `cargo test --workspace` on `dev` before any edit — **52**
- [x] **T3** RED — the three `// ---- ledger store seam ----` tests in
      `crates/heddle-core/tests/core.rs` against the not-yet-existing API; compiler errors
      recorded below
- [x] **T4** GREEN — `LedgerStore`, `Ledger::open`, the `store` field, fallible `append`,
      `HeddleError::Storage`, and the `?` churn across `native_loop.rs`, `tool.rs` and the three
      `heddle-core` test binaries
- [x] **T5** RED — `crates/heddle-silo` with an empty `src/lib.rs` and the whole of
      `tests/silo_ledger.rs` against the not-yet-existing `Silo`; red recorded below
- [x] **T6** GREEN — `SqliteLedgerStore` + `Silo`
- [x] **T7** `heddle-acp` wiring: `SessionParts.ledger`, the two test construction sites, and one
      new test (`a8`) — without it FR-011 would ship untested, because an unwired `Ledger::new()`
      and a wired-but-empty injected ledger are indistinguishable to the existing twelve
- [x] **T8** gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace`; new total recorded below
- [x] **T9** control diff: `git diff dev` empty on `crates/heddle-mcp/`, `spikes/`, `.github/`
      and `rust-toolchain.toml`
- [x] **T10** dependency drift recorded below
- [x] **T11** close out: split the "silo-backed durable Ledger (SQLite) + `SecretProvider`"
      bullet in `specs/003-heddle-core-foundation/tasks.md` into two and ticked the 009 half, set
      this spec's Status, and populated the "Next slice" list

## Control baseline (T2)

`cargo test --workspace` on `009-silo-ledger` @ `1d351df` (identical to `dev`), working tree
clean, 2026-09-03: **52 passing** — `heddle-acp/tests/acp_session.rs` 12, `heddle-core/tests/core.rs`
6, `tests/native_loop.rs` 18, `tests/tool_gateway.rs` 9, `heddle-mcp/tests/rmcp_gateway.rs` 7;
0 failed, 0 ignored. This is the number T8 diffs against.

## Pinned rusqlite surface (T1)

Read from the vendored source of `rusqlite 0.40.2` in the local cargo registry, and exercised by
a throwaway probe outside this repository before any product code was written. Every name below
is used by `ledger_store.rs` exactly as spelled here.

| Item | Pinned spelling |
|---|---|
| `Connection::open` | `pub fn open<P: AsRef<Path>>(path: P) -> Result<Connection>` (`src/lib.rs:437`) |
| `Connection::execute_batch` | `pub fn execute_batch(&self, sql: &str) -> Result<()>` (`src/lib.rs:548`) — accepts a multi-statement script, which is how the schema is applied |
| `Connection::execute` | `pub fn execute<P: Params>(&self, sql: &str, params: P) -> Result<usize>` |
| `Connection::prepare` | `pub fn prepare(&self, sql: &str) -> Result<Statement<'_>>` (`src/lib.rs:781`) |
| `Statement::query_map` | `pub fn query_map<T, P, F>(&mut self, params: P, f: F) -> Result<MappedRows<'_, F>>` where `F: FnMut(&Row<'_>) -> Result<T>` (`src/statement.rs:274`) — **`&mut self`**, so the statement is a `let mut` |
| `Row::get` | `pub fn get<I: RowIndex, T: FromSql>(&self, idx: I) -> Result<T>` (`src/row.rs:285`) |
| `params!` | `params![a, b] == &[&a as &dyn ToSql, …]` (`src/lib.rs:193`) |
| `Connection::pragma_update` | `pragma_update(None, "synchronous", "FULL")` — `schema_name: Option<Name>` |
| `Error` display | `SqliteFailure(_, Some(msg)) => write!(f, "{msg}")` (`src/error.rs:280`) — a trigger's `RAISE(ABORT, 'ledger is append-only')` surfaces as exactly that string |
| features | `bundled = ["libsqlite3-sys?/bundled", "modern_sqlite"]`; `default = ["cache", "ffi-sqlite-wasm-rs"]`, both unwanted, so `default-features = false` |

**Two of the assumed spellings were wrong, and T1 is why they never reached product code:**

1. **`u64` implements neither `ToSql` nor `FromSql`.** `rusqlite` covers `i8..=i64` and
   `u8..=u32`; `u64` needs the `fallible_uint` feature. `Step::seq` is `u64`, so the store casts
   to `i64` on write and converts back on read, failing with `HeddleError::Storage` rather than
   panicking if a stored value does not fit. Observed as
   `error[E0277]: the trait bound u64: ToSql is not satisfied`.
2. **`Row::get::<_, u64>` fails the same way** on `FromSql`.

**Three facts were measured, not assumed** (throwaway probe, 2026-09-03, this Windows host):

- `rusqlite 0.40.2` with `default-features = false, features = ["bundled"]` **compiles here**:
  `libsqlite3-sys 0.38.2` builds the C amalgamation through `cc 1.4.4` with the MSVC toolchain
  already on this machine. No system SQLite, no extra install step.
- A **file-backed** connection reports `journal_mode = delete` and `synchronous = 2` (FULL) after
  `pragma_update`, and the directory holds **exactly one file** — no `-wal`/`-shm` sidecars. This
  is the measurement the one-file-per-silo isolation argument rests on. (An *in-memory*
  connection reports `journal_mode = memory`, which is why the probe used a real file.)
- The append-only triggers behave as designed: `UPDATE` and `DELETE` both fail with the bare
  string `ledger is append-only`, and dropping the trigger with raw SQL then succeeds in forging
  a row — which is precisely the tamper-*evidence*-not-tamper-*proofing* boundary `s6` asserts.

## Observed red (Constitution III)

- **T3** `cargo test -p heddle-core --test core`, 2026-09-03:
  - `error[E0432]: unresolved import heddle_core::LedgerStore` — *"no `LedgerStore` in the root"*
    (`crates/heddle-core/tests/core.rs:4:28`)
  - `error: could not compile heddle-core (test "core") due to 1 previous error`
  - As in slices 007 and 008, rustc abandons the crate once import resolution fails, so this one
    diagnostic is the whole red: the `Ledger::open` and fallible-`append` errors underneath it are
    never reached.
- **T5** `cargo test -p heddle-silo --test silo_ledger`, 2026-09-03, against an empty `src/lib.rs`:
  - `error[E0432]: unresolved import heddle_silo::Silo` — *"no `Silo` in the root"*
    (`crates/heddle-silo/tests/silo_ledger.rs:14:5`)
  - `error: could not compile heddle-silo (test "silo_ledger") due to 1 previous error`
  - Every name the suite needs comes through `Silo`, so again one diagnostic is the whole red.
    `rusqlite` itself resolves in the test binary from the crate's `[dependencies]`, which is how
    `s5`/`s6` reach the file with raw SQL without a second dependency spelling.

## Gate run (T8)

2026-09-03, Windows leg observed locally; macOS and Linux unobserved until the repository has a
remote (SC-001).

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, no objection raised.
- `cargo test --workspace` — **63 passing**, 0 failed, 0 ignored: 52 pre-existing + 3 core seam
  tests + 7 `heddle-silo` tests + `a8`. Per binary: `acp_session` 13, `core` 9, `native_loop` 18,
  `tool_gateway` 9, `rmcp_gateway` 7, `silo_ledger` 7.
- `cargo test -p heddle-core -p heddle-mcp` in isolation — **43 passing** (the pre-slice 40 plus the
  three new seam tests), so nothing in the two oldest crates moved. Slice 008's
  `serde_json/preserve_order` feature-unification hazard has no analogue here: `rusqlite`'s
  `serde_json` dependency is optional and stays off, and `default-features = false` drops its
  `cache` and `ffi-sqlite-wasm-rs` defaults, so the only features it unifies are on crates
  `heddle-core` does not use.

## Control diff (T9)

`git diff dev --stat -- crates/heddle-mcp/ spikes/ .github/ rust-toolchain.toml` is empty
(SC-003, SC-004). `git diff dev -- Cargo.toml` is exactly two added `[workspace.dependencies]`
lines (SC-005). The rest of the slice is additive-plus-`?`-churn under `crates/heddle-core/` and
`crates/heddle-acp/`, new files under `crates/heddle-silo/` and `specs/009-silo-ledger/`, and five
lines in `docs/DEVELOPMENT.md`.

## Drift (T10)

Measured against a detached worktree at the branch point (`1d351df`), so both numbers come from a
real resolution rather than from the previous slice's note.

- **Dependency growth.** `cargo tree -e normal,build,dev` resolves **115** distinct
  package-versions before the slice and **126** after: **11** added, and none removed. Six are
  built into the product — `rusqlite`, `libsqlite3-sys`, `fallible-iterator`,
  `fallible-streaming-iterator`, the new `heddle-silo` itself, and dev-only `tempfile` — and five
  are `libsqlite3-sys`'s build-time chain: `cc`, `find-msvc-tools`, `shlex`, `pkg-config`,
  `vcpkg`. `bitflags` and `smallvec` were already in the graph via the ACP stack, so `rusqlite`
  adds no second copy. **`heddle-core` still has exactly four direct dependencies** (`serde`,
  `serde_json`, `thiserror`, `sha2`) and names no database. (`Cargo.lock` is `.gitignore`d in
  this repository, so the resolved graph is the measurable artefact rather than a lockfile diff;
  the base worktree also resolved `serde`/`proc-macro2`/`quote` one patch ahead of the working
  tree's cached lock, which is resolution noise and not this slice's doing.)
- **New build prerequisite: a C compiler.** `libsqlite3-sys` with `bundled` compiles the SQLite
  amalgamation through `cc`, so this workspace now needs a working C toolchain. That is the
  deliberate trade: no per-OS *SQLite* prerequisite, at the cost of a per-OS *compiler* one. All
  three GitHub runners ship one, and every platform row already in `docs/DEVELOPMENT.md`'s
  "Machine prerequisites" supplies one — but it was implied rather than stated, so a bullet
  naming it explicitly was added there.
- **No toolchain change.** `rusqlite 0.40.2` is edition 2021 and builds under the pinned 1.97;
  `rust-toolchain.toml` and `workspace.package.rust-version` are unchanged.
  `.github/workflows/core.yml` already runs the three gates on `crates/**`, so the new crate is
  picked up with no CI edit — confirmed by reading, not edited.

## Next slice (not this feature)
- [x] `SecretProvider` (OS keychain) + JIT `Redactor` — spec 010, extending `crates/heddle-silo`
- [ ] `heddle-cli` reference client and `heddle ledger log|show|verify` — the first consumer that
      opens a silo by name rather than by path
- [ ] bounded / paged `Ledger` reads: today `Ledger::open` mirrors a silo's whole history in RAM.
      The `LedgerStore` seam already admits a bounded read path when a caller needs one
- [ ] `Ledger` append-observer + **streaming** ACP session updates (still item 1 of 008's list)
- [ ] session persistence / `session/load` / resume on top of the durable ledger
- [ ] the rest of design §4.11's `Ledger`: `replay(from)`, `revert(to)`, `branch(from)`, and the
      `ts`/`principal`/`silo` fields on `Step` — none has a caller, and `principal` has no
      producer until identity exists
- [ ] SQLCipher / at-rest encryption of the silo file and per-silo keys, which becomes coherent
      only once 010 exists
- [ ] RBAC, team silos, `Mode` (Local/Server/Remote), the `Backend` trait and `ModeSupervisor`
      (design §4.8/§5.5/§7.10) — 009 makes *one* silo invariant real; it does not build a silo
      system
- [ ] retention and egress policy over the journal (§7.9)
