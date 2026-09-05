# Feature Specification: `heddle-cli` — the reference CLI client (v0 slice)

**Feature Branch:** `011-heddle-cli` · **Created:** 2026-09-03 · **Status:** Implemented (v0 slice)
**Input:** `specs/010-secret-provider/tasks.md` "Next slice" — *"`heddle-cli` reference client:
`heddle secret set|delete` (the second caller of `OsKeychain::store`/`delete`) and `heddle ledger
log|show|verify`"* · `specs/003-heddle-core-foundation/tasks.md` "Next slice" — *"`heddle-cli`
reference client"* · Constitution I (**the CLI is the core's complete, authoritative client**),
III (**test-first**), VI (**secrets by reference, never by value**), VII (**no capability without
a real need**) · design §4.1, §4.11, §7.13, §8 (Phase 0 exit: *"journal inspectable via `heddle
ledger`"*).

Ten merged slices built a governed, ACP-reachable, persistently-storing agentic loop with real
secret resolution. Every one of them is a library: `grep -rn '\[\[bin\]\]' --include=Cargo.toml`
over the workspace returns nothing, so **there is no executable anywhere**. Constitution
Principle I says every capability "is exposed through a programmatic API; the CLI is its
complete, authoritative client (the basis for E2E tests)". A library cannot satisfy that.

This slice ships the first executable: `heddle`, with two command groups that are **end-to-end
real** — a real on-disk silo and the real platform credential store, no stand-ins anywhere.

## What v0 lets a user do, and what it does not

**It does:**

```
heddle ledger log    --silo <ID> [--root <PATH>] [--run <RUN_ID>]
heddle ledger show   --silo <ID> [--root <PATH>] <STEP_ID>
heddle ledger verify --silo <ID> [--root <PATH>] [--run <RUN_ID>]
heddle secret set    <REFERENCE>      # value read from stdin, never from a flag
heddle secret delete <REFERENCE>
```

**It does not chat, and it does not serve ACP.** There is no `heddle chat`, no `heddle run`, and no
`heddle acp-agent`. The reason is a fact about the code, not a scoping preference: **no real,
network-backed `ModelClient` exists in this workspace.** `grep -rn "impl ModelClient" crates/`
finds one decorator (`heddle-acp`'s `CancellableModel`, generic over an inner client) and four
test-only `ScriptedModel`s; `grep -rni "reqwest\|hyper\|anthropic\|openai" crates/` finds nothing.

`HeddleAgent::new(factory)` requires `C: ModelClient + Send + 'static`, so serving ACP over stdio
needs *some* model. Two candidate stand-ins were weighed and both rejected:

- **An echo/placeholder model** would put a fake model in *product* code. A Zed or goose user who
  connects gets a chat-shaped experience whose answers are invented; a banner saying so does not
  change what the transcript looks like. Spec 010's SC-002 is this project's own standard: *"No
  in-memory stand-in for the backend under test."* A labelled fake is still a fake.
- **A model that refuses at the boundary** (`initialize` and `session/new` succeed, `session/prompt`
  fails with `HeddleError::Model`) is honest, and it proves the transport without inventing content.
  It is rejected on Principle VII: it is a capability with **no user value and no caller**. The
  whole of it — one `ModelClient` impl, a stdio `ConnectTo<Agent>`, a `tokio` runtime in the CLI
  and a subprocess ACP test — is machinery that exists only to be deleted by the slice that lands
  a real client.

`heddle acp-agent` and `heddle chat` therefore land in the **same slice as the real `ModelClient`**,
where they cost one file each and have a real caller. This is a recorded deferral, not an
omission; see `## Next slice` in `tasks.md`.

`heddle ledger replay|revert|branch` (design §4.11) are likewise absent: `Ledger` has no
`replay`/`revert`/`branch`, and synthesising them in the CLI would be inventing core capability at
the outermost layer, which Principle I forbids.

## User Scenarios & Testing

### User Story 1 — A person can read a silo's journal without knowing a run id (P1)
As an operator, I inspect what an agent did by pointing the CLI at a silo.
**Acceptance:**
1. **Given** a silo whose ledger holds steps from two runs, **When** `heddle ledger log --root
   <root> --silo <id>` is invoked, **Then** every step of both runs is printed, one per line, as
   four tab-separated columns `{run_id}\t{seq}\t{kind}\t{id}`, and the exit code is 0.
2. **Given** the same silo, **When** `--run <RUN_ID>` is added, **Then** only that run's lines are
   printed, with the same four columns, so a script's field offsets never shift.

### User Story 2 — A step's stored payload is reproducible byte for byte (P1)
As an auditor, `show` must be a faithful view of what the chain holds, not a rendering of it.
**Acceptance:**
1. **Given** a step id from the log, **When** `heddle ledger show … <STEP_ID>` is invoked, **Then**
   five `{label}\t{value}` header lines (`id`, `parent` — `-` when there is none — , `run`, `seq`,
   `kind`) are printed, then a line `payload`, then the payload verbatim.
2. **Given** an id that is in no chain, **When** `show` is invoked, **Then** the exit code is 1,
   stderr carries `not found`, and **stdout is empty** — a reader never gets a partial record.

### User Story 3 — Tamper-evidence is visible from the CLI (P1)
As an auditor, the hash chain is only worth something if the tool a person runs surfaces a break.
**Acceptance:**
1. **Given** an intact silo ledger, **When** `heddle ledger verify … ` is invoked with no `--run`,
   **Then** each run in the silo yields one `{run_id}\tok\t{n} steps` line and the exit code is 0.
2. **Given** a ledger row forged directly in SQLite (the trigger dropped and the payload
   overwritten), **When** `verify` is invoked, **Then** the exit code is 1 and stderr carries
   `ledger integrity broken at seq 0`.

### User Story 4 — An inspection command never fabricates an empty answer (P1)
As an operator, a typo in `--silo` must not look like "this silo has no history".
**Acceptance:**
1. **Given** a root under which silo `tpyo` was never created, **When** `heddle ledger log --root
   <root> --silo tpyo` is invoked, **Then** the exit code is 1, stderr carries `not found`, and
   `<root>/tpyo/ledger.sqlite3` does **not** exist.

### User Story 5 — A secret is provisioned without ever appearing in a flag or on screen (P1)
As an operator, I put a credential in the platform store from a script.
**Acceptance:**
1. **Given** a value piped on stdin, **When** `heddle secret set keychain://<service>/<account>` is
   invoked, **Then** the exit code is 0, the value is in the real platform credential store (a
   subsequent `OsKeychain::resolve` returns it), and **neither stdout nor stderr contains the
   value**.
2. **Given** the same reference, **When** `heddle secret delete <REFERENCE>` is invoked, **Then**
   the exit code is 0, its output carries the reference and not the value, and a subsequent
   `resolve` is `Err`.
3. **Given** any invocation, **When** `--value <V>` is passed to `secret set`, **Then** clap exits
   **2** with `unexpected argument '--value' found` — the flag does not exist and cannot be added
   silently.
4. **Given** an empty stdin, **When** `secret set` is invoked, **Then** the exit code is 1 with a
   `secret:` error and nothing is stored.
5. **Given** an interactive terminal on stdin, **When** `secret set` is invoked, **Then** it
   **refuses** with a message naming safe idioms, rather than prompting and echoing the secret
   into terminal scrollback.

## Requirements
- **FR-001**: The workspace MUST produce an executable named `heddle` (`[[bin]] name = "heddle"` in
  a new `crates/heddle-cli`), and that crate MUST have no `lib` target — nothing may depend on the
  outermost layer (Constitution IV).
- **FR-002**: Every CLI capability MUST be reachable through `heddle-core`/`heddle-silo`'s public
  API; the CLI MUST NOT implement any capability the library does not expose (Constitution I).
- **FR-003**: `heddle-core` MUST gain `Ledger::runs()` enumerating a chain's distinct run ids in
  first-append order. It MUST be purely additive: no existing signature changes.
- **FR-004**: `--run` MUST be optional on `log` and `verify`; omitted, they cover every run in the
  silo.
- **FR-005**: The silo root MUST come from `--root`, else `$HEDDLE_ROOT`, else a loud
  `HeddleError::Storage` — v0 has no config file and no platform data directory.
- **FR-006**: A ledger command MUST fail with `HeddleError::NotFound` when the silo has no ledger
  file, and MUST NOT create one.
- **FR-007**: `heddle secret set` MUST read the value from **stdin only**. It MUST NOT accept a
  `--value` flag (Constitution VI: a flag lands in shell history and in process listings).
- **FR-008**: `heddle secret set` MUST refuse a terminal stdin rather than prompt, so the value is
  never rendered on screen.
- **FR-009**: `heddle secret set` MUST refuse an empty value with `HeddleError::Secret`.
  `Redactor::from_values` already drops empty secrets, so an empty credential is silently useless.
- **FR-010**: Neither `secret` subcommand may write the value to stdout or stderr, on any path
  including errors.
- **FR-011**: Exit codes MUST be **0** on success, **1** on any `HeddleError`, **2** on a clap usage
  error.
- **FR-012**: `ledger log`'s `{kind}` column MUST come from the step kind's **serde** name — the
  same string the hash is fed — so there is no second name mapping that can drift from the hashed
  bytes.
- **FR-013**: `crates/heddle-mcp`, `crates/heddle-acp` and `crates/heddle-silo`'s public API MUST be
  unchanged.

## Success Criteria
- **SC-001**: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` and `cargo build --workspace` all clean; the suite is 71 pre-existing +
  10 new = **81** tests (2026-09-03).
- **SC-002**: `cargo build --workspace` produces a runnable `target/debug/heddle{,.exe}` and
  `heddle --help` succeeds — the slice's headline claim, checked by running it.
- **SC-003**: Every CLI test is a **process invocation of the real binary** against a real on-disk
  silo or the real platform credential store. No in-memory stand-in, and no unit test of an inner
  function standing in for a process test.
- **SC-004**: `git diff dev -- crates/heddle-mcp/ crates/heddle-acp/ crates/heddle-silo/ spikes/
  .github/ rust-toolchain.toml` is empty.
- **SC-005**: `git diff dev -- Cargo.toml` is exactly one added `[workspace.dependencies]` line.
- **SC-006**: `git diff dev -- crates/heddle-core/` is one added method plus one added test; all 71
  pre-existing tests stay live controls with their bodies unchanged.
- As in specs 004–010, the macOS and Linux legs of `core.yml` are unobserved until the repository
  has a remote; only the Windows leg is run locally.

## Assumptions
- **`heddle ledger show` prints exactly what the chain holds — including a secret, if one is
  there.** `ToolCall`/`ToolResult` payloads pass through the `Redactor`
  (`ToolGateway::call_captured`), but `NativeLoop::run` appends `LlmRequest`/`LlmResponse`
  payloads **raw**. So `show` can print a credential that appeared in a conversation. This is a
  property of the **Ledger**, not something the CLI introduces: Principle I holds, and the CLI
  exposes exactly what the API exposes. Fixing it means redacting on the model-I/O path, which
  changes the governed loop rather than the CLI — recorded on the next-slice list, not done here.
- **`heddle secret` takes no `--silo` flag.** `OsKeychain::new()` takes no silo and derives its
  service name entirely from the `keychain://<service>/<account>` reference. Design §7.2's "one
  keychain per silo" is explicitly deferred on spec 010's own backlog. A `--silo` flag that is
  accepted and ignored would be exactly the fake this slice refuses to ship.
- **An unknown `--silo` leaves an empty directory behind.** `Silo::open` `create_dir_all`s, and its
  id validation is the security-relevant part that must not be re-implemented in the CLI. The
  ledger commands therefore open the silo and *then* require the ledger file to exist. The empty
  directory is an accepted wart; avoiding it would need a `Silo::open_existing()` addition to
  `heddle-silo` — a second API change for a cosmetic gain.
- **Tab-separated output is the contract.** It is asserted by tests on purpose: a "100%
  scriptable" CLI's stdout *is* its user contract, so changing it must break something. `--json`
  is on the next-slice list for when a second consumer needs it.
- **The terminal-stdin refusal has no automated test.** The harness has no PTY. It is exercised by
  hand and the observed message recorded in `tasks.md`.
