# Tasks: slice 037 — pluggable `TaskTracker`, silo-backed default, config hierarchy

**Spec:** `specs/038-tasktracker-hierarchy/spec.md` · **Plan:** `specs/038-tasktracker-hierarchy/plan.md` ·
TDD (test-first), branch `feat/task-tracker`, a clean fast-forward of `dev` at `67ebfd2`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)

- **I Headless core** ✅ Everything added is library surface. No CLI subcommand and no UI wiring, and
  that is recorded in `spec.md`'s Out of scope rather than left implicit. `TaskTracker`,
  `Hierarchy`, `Silo::tracker()` and `WorkflowEngine::with_tracker` are exactly the API a
  `heddle task` / `heddle config` pair would drive, in the same shape `heddle-cli` already drives
  `NativeLoop` and `Ledger`. Nothing here is reachable only from a UI.

- **II Local-first, silo isolation** ✅ NON-NEGOTIABLE and strengthened rather than merely preserved.
  `LocalTracker::requires_network()` returns `false` as a **build** property — there is no code path
  in `task_tracker.rs` that could open a socket, and `heddle-silo`'s dependency list is unchanged
  (`cargo tree` adds no package to the workspace for this slice, in any crate). The task table lives
  in the silo's *existing* SQLite file rather than a second one, specifically so this crate's own
  isolation argument — "one directory holding one SQLite file" — stays literally true;
  `a_tasks_ledger_and_its_tasks_share_the_silos_one_file` asserts the directory still holds exactly
  one file, and `two_silos_cannot_see_each_others_tasks` asserts the boundary itself.

- **III Test-First** ⚠️ **Substantially met, with one shortfall recorded rather than papered over.**
  All 36 tests were written before the code they exercise, and the file order is verifiable: each
  test file was committed to disk before the module it imports existed. But the red was
  **structural** (the test files could not compile, because `Hierarchy`, `TaskTracker` and
  `with_tracker` did not exist) rather than **observed** — no failing `cargo test` run was executed
  for any of the three files before its implementation was written. That is a real departure from
  red→green, and no red has been re-staged after the fact to disguise it.

  It is compensated — not excused — by **four measured counterfactuals**, each neutering one
  mechanism and recording exactly which tests failed. See `## Counterfactuals` below. Two of the four
  produced findings this record would not otherwise contain, including one place where a test proves
  less than its name suggests.

- **IV Inverted coupling** ✅ `heddle-workflow` gains a third generic parameter,
  `K: TaskTracker = NoTracker`, and names no backend — the compile-time statement of this is
  `the_engine_holds_whatever_tracker_it_was_given`, whose `fn accepts_any<K: TaskTracker>` would stop
  compiling if the parameter were ever replaced by a concrete type. `heddle-core` gains a port and a
  resolver and no dependency; its `[dependencies]` are the same five crates as before.

  The resolver deliberately resolves to an **opaque name**, not a `TrackerKind { Local, Vikunja,
  Jira }` enum: three product names in the crate whose defining property is that it names no provider
  would have been the violation this principle exists to catch (plan.md D2). `heddle-core` gains two
  **additive** error variants; no existing variant's meaning changes and no existing signature or
  call site changes. Slice 002 needed a new `StepKind`; this slice needed none — `StateChange`
  already existed.

- **V Traceability** ✅ The node→task binding is written to the hash-chained Ledger as a
  `StepKind::StateChange` step and read back from it, so "which task is this node's" has exactly one
  source of truth and it is the chain (plan.md D4). `the_chain_records_which_task_each_node_opened`
  asserts `Ledger::verify_chain` over a run that wrote bindings, so the chain is proven intact rather
  than assumed. The binding is appended **after** the tracker answers, so a tracker that refuses
  leaves no chain entry pointing at a task that was never opened.

  The task board itself is mutable, unlike the chain, and the asymmetry is deliberate and documented:
  a Ledger records what happened, a board records where things stand. The audit trail of the moving
  is the chain's, which is why `task` gets no append-only trigger where `ledger_step` has two.

- **VI Security & secrets** ✅ Nothing here holds, resolves or logs a secret. `TaskTracker` has no
  credential surface — a networked backend would resolve one through `SecretProvider`, which is
  where that already belongs. The hierarchy's `Lock` is an authorization mechanism and is enforced on
  **both** the write path and the read path (plan.md D8); counterfactual B shows write-only
  enforcement is what three tests bite on, and A shows read-only enforcement is bitten by one.

  §5.5's security monotonic floor is **not** implemented, and saying so is the point: "tighten" needs
  an ordering on values that only a security-typed value can supply, so a resolver claiming to enforce
  it for `Hierarchy<String>` would be claiming something it cannot check. Recorded in the module
  docs, in `spec.md`'s Out of scope and in plan.md D9 — three places, none of them a silent gap.

- **VII Neutrality & reuse (YAGNI)** ✅ No new crate: two modules in `heddle-core`, one in
  `heddle-silo`, one generic parameter on an existing struct (plan.md D1). `Hierarchy<T>` is generic,
  which is the one place this slice builds more than the immediate need — justified because §5.5
  states in as many words that the same resolver governs five different settings, so the alternative
  is five copies of a lock rule rather than one.

- **VIII Loop discipline** ✅ NON-NEGOTIABLE and untouched. Nothing here iterates, and FR-017's
  `LoopController` node types remain out of scope exactly as they were for slice 002's User Story 1.

---

## Tasks

| # | Task | Files | Result |
|---|---|---|---|
| T1 | Tests for hierarchy resolution (spec 002 US2, all four acceptance scenarios) | `crates/heddle-core/tests/config_hierarchy.rs` | 13 tests, written before `hierarchy.rs` existed |
| T2 | `Scope`, `Lock`, `Setting`, `Mode`, `Hierarchy` + two additive error variants | `crates/heddle-core/src/{hierarchy.rs,error.rs,lib.rs}` | 13/13 green |
| T3 | Tests for the silo-backed tracker | `crates/heddle-silo/tests/silo_tasks.rs` | 12 tests, written before `task_tracker.rs` existed |
| T4 | `TaskTracker` port + `LocalTracker` adapter + `Silo::tracker()` / `Silo::store_path()` | `crates/heddle-core/src/task.rs`, `crates/heddle-silo/src/{task_tracker.rs,lib.rs}` | 12/12 green after one test-side borrow fix (below) |
| T5 | Tests for workflow→tracker wiring (spec 002 US3) | `crates/heddle-workflow/tests/{tracker.rs,common/mod.rs}` | 11 tests, written before the engine change |
| T6 | Third generic parameter, `open_task`/`move_task`, `task_bindings` scan | `crates/heddle-workflow/src/engine.rs` | 11/11 green, and slice 002's 25 existing tests unchanged and still green |
| T7 | Four counterfactuals | — | all four bite; see below |
| T8 | Spec 002 status, spec 037 folder, README | `specs/002-workflow-engine/spec.md`, `specs/038-tasktracker-hierarchy/*`, `README.md` | done |
| T9 | Close-out: `cargo test --all`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings` | — | all three green |

**Test count**: 36 added (13 + 12 + 11). Workspace total after this slice: every pre-existing test
still passes, none modified.

---

## Counterfactuals (T7)

Each neuters one mechanism in the implementation, records which tests fail, and is then reverted.
Run against the real suite, not described from memory.

### A — `Hierarchy::resolve` ignores locks entirely

`winner()` reduced to `self.settings.iter().max_by_key(|s| s.scope)`, dropping the highest-lock
branch.

**1 of 13 failed**: `the_highest_explicit_lock_wins_over_a_lower_one`.

**Finding, and it is worth stating plainly**: the other lock tests survive because
`Hierarchy::set` refuses the conflicting write, so `resolve` is never asked the question. The read
path is independently exercised by exactly one test — the one that locks a *lower* scope first and
then a higher one, which is the only ordering `set` permits. That is thin, and it is thin for a
structural reason rather than an oversight: with `set` enforcing the cap, the only way to build a
hierarchy holding two locks is bottom-up. A hierarchy deserialized from disk could hold any
combination, which is precisely why the read path is enforced at all (plan.md D8) — and that path
will need more tests the moment a config file exists to deserialize.

### B — `Hierarchy::set` does not enforce the lock

`if let Some(above) = self.lock_above(scope)` replaced with `if let Some(above) = None::<Scope>`.

**3 of 13 failed**: `a_lower_scope_that_tries_to_override_a_lock_above_is_refused_explicitly`,
`a_lock_below_an_unlocked_default_still_binds_what_is_under_it`,
`local_mode_resolves_the_hierarchy_with_no_team_level`.

Spec 002's "explicit refusal" edge case is genuinely load-bearing on the write path.

### C — the engine ignores the chain's binding and always creates a task

`if let Some(task_id) = bound.get(node.id())` replaced with `if let Some(task_id) = None::<&String>`.

**3 of 11 failed**: `polling_a_pending_gate_re_asserts_the_status_without_re_opening_the_task`,
`a_rejected_gate_cancels_its_task`, `an_approved_gate_finishes_its_task_and_the_run_carries_on`.

**Finding**: `a_resumed_run_does_not_open_a_second_task_for_a_node_it_already_finished` **did not
fail**, and its name promises more than it proves. A completed node is skipped by the `completed`
guard before `open_task` is reached at all, so that test is carried by slice 002's resume mechanism
rather than by this slice's binding. The binding's real job is the *pending* node — the approval gate
being polled — which is what the three failures above are. The property still holds; the attribution
in the test name does not, and this is the honest place to say so.

### D — a completed node does not move its task

`self.move_task(&task, TaskStatus::Done)?` removed from the completion path.

**3 of 11 failed**: `every_node_that_completes_leaves_its_task_done`,
`a_node_waiting_on_a_human_has_its_task_blocked`, `an_approved_gate_finishes_its_task_and_the_run_carries_on`.

Spec 002's User Story 3 acceptance scenario — the whole point of the slice — is bitten by three
tests, including the two about the approval gate.

---

## Deviations

| Item | Planned | Done | Why |
|---|---|---|---|
| `TaskTracker::create` signature | design §4.13's `create(&self, t: Task) -> TaskId` | `create(&mut self, task: NewTask) -> Result<TaskId>` | §4.13's sketch asks the caller to build a `Task` carrying the `TaskId` the call is about to return. The backend assigns it — Jira answers `PROJ-123` — so the caller cannot. `NewTask` is the same argument minus the field the caller cannot know. `&mut self` and `Result` follow `LedgerStore::append`'s precedent; `list` keeps `&self` because reading is genuinely read-only. Recorded in `task.rs`'s module docs. |
| `TaskStatus::parse` | `from_str` | `parse` | An inherent `from_str` trips `clippy::should_implement_trait`, and the gate is `-D warnings`. A full `FromStr` impl would buy a `str::parse` call site nothing asks for. |
| `Silo::ledger_path()` | not addressed | kept, delegating to the new `store_path()` | The file now holds the chain *and* the board, so `ledger_path` no longer describes its contents. Renaming outright would churn every existing caller for no behavioural gain; the new name is the honest one and the old one is a one-line delegation. |
| `crates/heddle-silo/tests/silo_tasks.rs` | — | one test-side fix during T4 | `list_returns_tasks_in_creation_order` borrowed `&str` from a temporary `Vec<Task>` (E0716). Collected `String` instead. A test-harness fix, not a change to what is asserted. |

---

## Close-out (T9)

Run in this worktree, in this order, all green:

- `cargo test --all` — exit 0. 36 new tests pass; every pre-existing test in the workspace still
  passes, and none was modified. `crates/heddle-workflow`'s 25 existing tests are of particular
  interest, since the engine they exercise gained a generic parameter: they compile and pass
  unchanged, which is what the `NoTracker` default type parameter is for (plan.md D5).
- `cargo fmt --all -- --check` — exit 0 (after `cargo fmt --all` applied four diffs).
- `cargo clippy --all-targets -- -D warnings` — exit 0, no warnings.
