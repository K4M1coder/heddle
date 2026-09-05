# Plan: slice 037 — pluggable `TaskTracker`, silo-backed default, config hierarchy

**Target artifacts:** `specs/038-tasktracker-hierarchy/{spec.md,plan.md,tasks.md}`
**Branch:** `feat/task-tracker`, a clean fast-forward of `dev` at `67ebfd2` (verified: working tree
clean, 0 local commits, rebased onto `github/dev` rather than the stale local `origin/dev`)
**New crate:** none. Two modules in `heddle-core`, one in `heddle-silo`, one generic parameter on
`heddle-workflow`'s engine.

**Scope, stated up front:** this slice implements spec 002's **User Story 2** (tracker chosen through
the hierarchy) and **User Story 3** (workflow progress reflected in the tracker), which slice 002's
own `plan.md` named as out of scope for User Story 1. Of design §4.13's three backends, only
`LocalTracker` is built. Vikunja and Jira are **not**, and the Atlassian MCP connector on the parallel
`feat/atlassian-connector` branch is not a tracker backend. See *Out of scope*, which is exhaustive on
purpose.

---

## Problem

Spec 002 FR-014 requires a pluggable `TaskTracker` and FR-015 requires config resolved through the
Silo ▸ Team ▸ Project ▸ Conversation hierarchy with "highest locks lowest". Neither exists. Before
this slice, `grep -rn "TaskTracker" crates/` returned zero hits and so did any search for a config
scope or a lock. The workflow engine merged by slice 002 executes a graph and logs a `WorkflowNode`
step per node, and nothing anywhere reacts to that.

FR-016 — "workflows MUST be able to orchestrate the SDLC through connectors **and the TaskTracker**" —
is therefore half-met at best: the connector half landed with slice 002, and the tracker half has
nothing to orchestrate against. Phase 1 MVP axis 1e (design §8) names the tracker alongside the
workflow engine as one deliverable.

## What was verified before planning

Everything below was read in this worktree at `67ebfd2`.

1. **`crates/heddle-core/src/ledger.rs:60-70`** — `LedgerStore` is a trait in the core whose only
   implementation, `SqliteLedgerStore`, lives in `heddle-silo`. **`crates/heddle-core/src/secret.rs:44-56`**
   — `SecretProvider` is the same shape, and its `requires_network` exists specifically to govern
   availability under the egress policy. These two are the precedent this slice follows exactly:
   *port in the core, adapter in the silo*. A third port needed no new pattern to be invented.
2. **`crates/heddle-workflow/src/engine.rs:89-95`** — `WorkflowEngine<C: ModelClient, T: ToolTransport>`
   is generic over its ports and boxes no trait object. `grep` confirms the type has **no callers
   outside its own four test files** — no CLI subcommand, no UI wiring — so a third generic parameter
   was a possibility rather than a breaking change to be worked around.
3. **`crates/heddle-core/src/ledger.rs:21`** — `StepKind::StateChange` already exists and this
   workspace has exactly one writer of it, a `heddle-silo` test. A binding step needed no new enum
   variant; slice 002 had to add one for `WorkflowNode` and this slice did not.
4. **`crates/heddle-workflow/src/engine.rs:342-357`** — `last_decisions` demonstrates the crate's
   established handling for a `StepKind` it does not exclusively own: parse leniently, skip what is
   somebody else's. `completed_nodes` above it is strict, for the opposite and equally stated reason.
5. **`crates/heddle-silo/src/lib.rs:1-20`** — the isolation argument is written as "one directory
   holding one SQLite file", and it is what Constitution II rests on. This constrained where tasks
   could be stored (D3).
6. **design §5.5** — the resolution rule is two sentences, and the second ("the **highest** explicit
   lock wins") runs in the opposite direction from the first ("the **most specific** value wins").

---

## Decisions

### D1 — The trait goes in `heddle-core`; the backend goes in `heddle-silo`

**Chosen** because it is the shape `LedgerStore`/`SqliteLedgerStore` and
`SecretProvider`/`OsKeychain` already have, and Constitution IV is what put them there.
`heddle-workflow` depends on `heddle-core` and nothing else; putting the trait anywhere else would
have meant either a new crate for it to depend on or a dependency on `heddle-silo`, and the second is
precisely the inverted-coupling violation the principle names.

**Rejected: a `crates/heddle-tasktracker` crate.** Nothing would live in it but a trait and four
value types, none of which pull a dependency. It would add a manifest, a `cargo tree` node, and a
second place to look for a port, in exchange for nothing.

### D2 — The resolved tracker is a **name**, not an enum

`Hierarchy<T>` is generic and the tracker choice is `Hierarchy<String>`. A `TrackerKind { Local,
Vikunja, Jira }` in `heddle-core` would put three product names in the crate whose defining property
is that it names no provider. Turning a resolved name into an implementation is the host's job.

Generic rather than concrete is not speculative: §5.5 says in as many words that "this single
resolver governs harness, TaskTracker, egress, providers and secrets". Five copies of a lock rule is
where the fifth copy differs from the first.

### D3 — Tasks live in the silo's **existing** SQLite file

A `tasks.sqlite3` beside `ledger.sqlite3` would have been simpler to write and would have quietly
falsified `heddle-silo`'s own stated isolation argument. Keeping the sentence literally true is worth
a shared file and a `busy_timeout` for the two connections. `Silo::store_path()` is the path under a
name that now describes all of its contents; `Silo::ledger_path()` remains, delegating to it, because
every existing caller asks for it by that name.

The `task` table gets **no** append-only trigger, unlike `ledger_step`. A Ledger is a record of what
happened; a task board is a record of where things stand and exists to be moved. The audit trail of
the moving is the chain's.

### D4 — Which task belongs to which node is recorded on the chain

A tracker assigns its own id — Jira answers `PROJ-123` — so a key derived from `run_id + node_id`
would work only for the local backend. The engine appends `StateChange {node_id, task_id}` and reads
it back on resume, which is how it already learns which nodes are complete (Constitution V).

This is also what makes the resume property fall out rather than need building: a completed node is
skipped before any tracker call, and a pending node finds its binding and re-asserts a status instead
of creating a second task.

**Rejected: a `task` field on `Node`.** `Node` is the serialized shape of a `Workflow` and slice 002
froze its vocabulary deliberately. Growing it would migrate every workflow already written.

### D5 — `NoTracker` is uninhabited

The engine is `WorkflowEngine<C, T, K: TaskTracker = NoTracker>` with `tracker: Option<K>`. The
default parameter is what keeps slice 002's four test files and their `WorkflowEngine<C, T>` type
annotations compiling unchanged.

`pub enum NoTracker {}` has no variants, so no value of it can exist and `Option<NoTracker>` is
`None` by construction. The alternative — a unit struct whose `create` returns a fabricated id and
discards the task — compiles just as well and produces exactly the bug worth preventing: a run
reporting progress nobody can see. Each trait method is `match *self {}`, accepted by the compiler
precisely because it is unreachable.

### D6 — A task is opened **before** its node runs, not after

"Moves to the appropriate status" needs a status to move from, and a run that dies inside a node
should leave that node's task visibly in progress rather than absent. The binding step is appended
*after* the tracker answers, so a tracker that refuses leaves no chain entry pointing at a task that
was never opened.

### D7 — Five statuses, not two

A workflow strictly needs `InProgress` and `Done`. `Blocked` is what an `Approval` node waiting on a
person looks like and `Cancelled` is what that person saying no looks like; collapsing either into
`Todo` makes a run that is waiting for its reader indistinguishable from one that has not started.
`Todo` is the state a task created by something other than the engine starts in.

### D8 — The lock is enforced on **write** and on **read**

`Hierarchy::set` refuses a capped scope (spec 002's Edge Case: "explicit refusal"), and
`Hierarchy::resolve` independently prefers the highest lock. Write-only enforcement would leave a
hierarchy deserialized from disk unguarded; read-only enforcement would make the refusal
unobservable.

### D9 — §5.5's security monotonic floor is **not** encoded

"Tighten" needs an ordering on values that only a security-typed value can supply. A resolver
claiming to enforce it for `Hierarchy<String>` would be claiming something it cannot check. Recorded
in the module docs and in `spec.md`'s Out of scope rather than left implicit.

---

## Files

| File | Action |
|---|---|
| `crates/heddle-core/src/task.rs` | CREATE — `TaskId`, `TaskStatus`, `Task`, `NewTask`, `TaskQuery`, `TaskTracker`, `NoTracker` |
| `crates/heddle-core/src/hierarchy.rs` | CREATE — `Scope`, `Lock`, `Setting`, `Mode`, `Hierarchy` |
| `crates/heddle-core/src/lib.rs` | UPDATE — two `pub mod`s and their re-exports |
| `crates/heddle-core/src/error.rs` | UPDATE — `Config`, `ConfigLocked` (both additive) |
| `crates/heddle-silo/src/task_tracker.rs` | CREATE — `LocalTracker` |
| `crates/heddle-silo/src/lib.rs` | UPDATE — `Silo::tracker()`, `Silo::store_path()` |
| `crates/heddle-workflow/src/engine.rs` | UPDATE — third generic param, `open_task`/`move_task`, `task_bindings` |
| `crates/heddle-core/tests/config_hierarchy.rs` | CREATE — 13 tests |
| `crates/heddle-silo/tests/silo_tasks.rs` | CREATE — 12 tests |
| `crates/heddle-workflow/tests/tracker.rs` | CREATE — 11 tests |
| `crates/heddle-workflow/tests/common/mod.rs` | UPDATE — `RecordingTracker`, `tracked_engine`, `task_bindings` |
| `specs/002-workflow-engine/spec.md` | UPDATE — Status |
| `README.md` | UPDATE — Current status |

## Validation

```
cargo test --all
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

## Out of scope (explicit, exhaustive)

Everything in `spec.md`'s *Out of scope* section, which this plan does not restate in order to avoid
two lists that could drift: a Vikunja backend, a Jira backend, a name→implementation registry, a CLI
subcommand or UI surface, persisting a `Hierarchy` to disk, §5.5's security monotonic floor, and
FR-017's `LoopController` node types.
