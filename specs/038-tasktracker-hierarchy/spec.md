# Feature Specification: a pluggable `TaskTracker`, a silo-backed default, and the config hierarchy that resolves it

**Slice**: 037

**Status**: implemented

**Feature Branch**: `feat/task-tracker`

**Created**: 2026-09-05

**Covers**: [`specs/002-workflow-engine`](../002-workflow-engine) **User Story 2** (FR-014, FR-015)
and **User Story 3** (SC-003), which slice 002's own plan named as out of scope for User Story 1.

**Input**: design [§4.13](../../docs/superpowers/specs/2026-07-15-skein-design.md) (pluggable
`TaskTracker`) and [§5.5](../../docs/superpowers/specs/2026-07-15-skein-design.md) (organizational
hierarchy & config resolution); Phase 1 MVP axis **1e** (design §8).

---

## What this slice changes for a user

**Before**: the native workflow engine runs a graph and writes a Ledger step per node. Nothing
creates or updates a task anywhere, and there is no `TaskTracker` abstraction in the tree at all —
`grep -r TaskTracker crates/` finds nothing but spec prose. A workflow meant to orchestrate the SDLC
through a tracker (FR-016) has nothing to orchestrate against.

**After**: a workflow can be handed a tracker at construction. Every node it executes opens a task,
in progress, before the node runs, and moves it when the node ends — `Done` on completion, `Blocked`
while a human is being waited on, `Cancelled` when that human says no. The always-available tracker
is the silo's own SQLite file, so this works with egress off and nothing configured. Which tracker is
active is resolved through the Silo ▸ Team ▸ Project ▸ Conversation hierarchy, where a lock set high
binds every level below it and an unlocked default does not.

Not visible yet: no CLI subcommand and no UI surface. This slice is the library layer, in the shape
`heddle-cli` already drives every other core capability in.

---

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Choose the task tracker through the hierarchy (Priority: P1)

Spec 002's **User Story 2**, unchanged. As a project manager, I set the tracker at the silo or
project level; lower levels inherit it, and a lock set higher up takes precedence.

**Independent Test**: set a tracker locked at the silo → a child project is bound to it; set nothing
above a project → the project chooses freely.

**Acceptance Scenarios**:

1. **Given** a tracker locked at the silo, **When** a lower scope resolves config, **Then** it gets
   the silo's choice, and its own attempt to set another is refused explicitly rather than ignored.
2. **Given** no setting above the project, **When** the project chooses a tracker, **Then** its
   conversations use it.
3. **Given** Local mode, **When** config is resolved, **Then** the hierarchy applies without a Team
   level, and a Team-scoped write is refused outright.
4. **Given** a value set high but **not** locked, **When** a lower scope sets its own, **Then** the
   lower scope wins — setting is not locking.

### User Story 2 — Progress reflected in the tracker (Priority: P2)

Spec 002's **User Story 3**, unchanged. As a user, a workflow's progress creates and updates tasks in
the active tracker.

**Acceptance Scenarios**:

1. **Given** a running workflow, **When** a node completes, **Then** the corresponding task moves to
   `Done` in the resolved tracker.
2. **Given** a workflow stopped at an `Approval` node, **When** it returns, **Then** that node's task
   is `Blocked`; **When** the human refuses, **Then** it is `Cancelled`.
3. **Given** a run resumed in a second process, **When** it walks nodes the chain already records as
   complete, **Then** it opens no second task for them.

### User Story 3 — A tracker that is always there (Priority: P1)

As a local user with egress off, I get a working task board without configuring or reaching anything.

**Acceptance Scenarios**:

1. **Given** any silo, **When** I ask it for a tracker, **Then** I get one whose
   `requires_network()` is `false`.
2. **Given** tasks created in silo A, **When** silo B lists its tasks, **Then** it sees none of them.

### Edge Cases

- **Lock conflict**: a lower scope tries to override a setting locked above → explicit refusal
  (`HeddleError::ConfigLocked`), naming both the refused scope and the scope that holds the lock.
- **A scope that does not exist in this mode**: a Team-scoped write in Local mode → explicit refusal
  (`HeddleError::Config`), not a silently ignored write.
- **Nobody has configured anything**: resolution yields `None`. Not an error — the caller that must
  then fall back to the always-available local tracker is the one that knows what "local" means.
- **A tracker that refuses mid-run**: the node's error ends the run, and no chain entry is left
  pointing at a task that was never opened.
- **A run resumed with no tracker**: the chain is unchanged from what an untracked run would write —
  no binding step is appended when there is nothing to bind to.

---

## Requirements *(mandatory)*

Numbering continues spec 002's, because these are spec 002's requirements being met rather than new
ones.

- **FR-014** (spec 002): The system MUST provide a pluggable `TaskTracker`. **Met for the trait and
  the local silo-backed implementation.** Vikunja and Jira backends are explicitly *not* in this
  slice — see *Out of scope*.
- **FR-015** (spec 002): Config MUST be resolved according to the Silo ▸ Team ▸ Project ▸
  Conversation hierarchy, a setting fixed at one level **locking** the lower levels. **Met.**
- **FR-016** (spec 002): Workflows MUST be able to orchestrate the SDLC through connectors **and the
  TaskTracker**. **The TaskTracker half is met**; the connector half was already met by slice 002.

New to this slice:

- **FR-037a**: `heddle-workflow` MUST reach a tracker only through the `TaskTracker` trait, never a
  concrete backend, and MUST name no tracker product (Constitution IV).
- **FR-037b**: The local tracker MUST be usable with no network egress and no configuration
  (Constitution II).
- **FR-037c**: Which task belongs to which node MUST be recoverable from the Ledger, so a run resumed
  by a different process reuses the task it already opened rather than opening a second one
  (Constitution V).
- **FR-037d**: A refused config write MUST leave the resolved value untouched.

### Key Entities

Spec 002's, realised:

- **Task**: `{id, title, status, links}`. `id` is assigned by the backend — Jira answers `PROJ-123`,
  the local tracker answers a row id — so the caller supplies a `NewTask` and receives a `Task`.
- **ConfigScope**: spec 002's "resolution level + `locked` flag" is `Setting { scope, value, lock }`;
  the level alone is `Scope`, and `Mode` is which levels exist.

---

## Success Criteria *(mandatory)*

- **SC-002** (spec 002): hierarchical resolution honours "the highest level locks", tested across all
  four levels and in Local mode's three. **Met** — 13 tests in
  `crates/heddle-core/tests/config_hierarchy.rs`.
- **SC-003** (spec 002): a workflow's progress is visible in the resolved tracker. **Met** — 11 tests
  in `crates/heddle-workflow/tests/tracker.rs`.
- **SC-037a**: the local tracker's task board survives the process that wrote it and is invisible to
  another silo. **Met** — 12 tests in `crates/heddle-silo/tests/silo_tasks.rs`, all against a real
  SQLite file under a real temporary directory.
- **SC-037b**: `cargo tree -p heddle-workflow` gains no package. **Met** — the trait lives in
  `heddle-core`, which this crate already depended on.

---

## Out of scope *(explicit — do not read these as missing)*

- **A Vikunja-backed `TaskTracker`.** Design §4.13 names it as the embedded OSS option; nothing here
  embeds a server.
- **A Jira-backed `TaskTracker`.** It would go through an MCP connector. The Atlassian connector
  landing on `feat/atlassian-connector` is a separate, parallel concern and is *not* a tracker
  backend.
- **A registry mapping a resolved tracker name to an implementation.** `Hierarchy` resolves to an
  opaque name and the host binds it; a registry in `heddle-core` would make the core name the
  backends it exists not to know (Constitution IV).
- **A CLI subcommand or UI surface** for tasks. The library layer is what this slice delivers, in the
  same shape every other core capability was delivered in before its CLI arrived.
- **Persisting a `Hierarchy` to disk.** It serializes — `Serialize`/`Deserialize` are derived — but
  nothing in this slice reads a config file. Where the hierarchy's values *come from* is a later
  slice's question.
- **Enforcing §5.5's security monotonic floor.** "Tighten" requires an ordering on values that only a
  security-typed value can supply, so a resolver claiming to enforce it for an arbitrary `T` would be
  claiming something it cannot check. Recorded in `hierarchy.rs`'s module docs rather than left
  implicit.
- **FR-017's `LoopController` node types.** Out of scope for slice 002's User Story 1 and still out
  of scope here.

---

## Assumptions

- The hierarchy lives within a silo and never crosses the silo boundary (design §5.3, §7.10).
- In Local mode the Team level does not exist, so a Team-scoped write is an error rather than a
  no-op.
- A tracker's task board is mutable state, unlike the Ledger: the audit trail of *how* it got there is
  the chain's job, not the board's.
