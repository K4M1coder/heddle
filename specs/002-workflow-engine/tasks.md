# Tasks: slice 002 (User Story 1) — a native, Ledger-backed `WorkflowEngine`

**Spec:** `specs/002-workflow-engine/spec.md` · **Plan:** `specs/002-workflow-engine/plan.md` ·
TDD (red→green), branch `feat/workflow-engine` cut from `dev` at `d364405`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)

- **I Headless core** ✅ `heddle-workflow` is a library crate with no CLI of its own, and this slice
  adds no CLI subcommand — deliberately, and recorded in `plan.md`'s Out-of-scope rather than left
  implicit. The engine's whole public surface is `WorkflowEngine::run` and `WorkflowEngine::decide`,
  which is exactly the API a `heddle workflow run` / `heddle workflow decide` pair would drive, in the
  same shape `heddle-cli` already drives `NativeLoop`. Nothing here is reachable only from a UI.
- **II Local-first** ✅ NON-NEGOTIABLE and untouched. The new crate opens no socket, names no
  provider, and reaches the outside world only through the `ModelClient` and `ToolTransport` ports it
  is generic over — both injected by the caller. Its `[dependencies]` are `heddle-core`, `serde`,
  `serde_json`, and nothing else; `cargo tree -p heddle-workflow` adds no package to the workspace.
  Silo boundaries are not crossed because the crate never opens a silo: it borrows a `Ledger` the
  caller already opened.
- **III Test-First** ⚠️ **Partially met, and the shortfall is this run's, not the plan's.** T1
  measures a baseline before any edit, and every test was written before it was run. But only **T3**
  produced a genuine unwritten-code red: T3's green implemented `WorkflowEngine::run` whole rather
  than minimally, so T4, T5 and T6 each passed on first execution against code that already existed.
  That is a real departure from "minimal implementation → green", it is recorded verbatim under
  `## Observed red` rather than papered over, and it is compensated — not excused — by a **measured
  counterfactual for each of the four steps**, in which the mechanism under test is neutered and the
  resulting failure is recorded. No red was re-staged after the fact.
- **IV Inverted coupling** ✅ The engine is `WorkflowEngine<C: ModelClient, T: ToolTransport>`,
  mirroring `NativeLoop<C, P, T>` — a generic over ports, never a `Box<dyn Agent>` or a node-executor
  registry, neither of which exists anywhere in this tree. `heddle-workflow` names no connector crate,
  no protocol and no provider. `heddle-core` gains two **additive** enum variants and nothing else: no
  existing variant's meaning changes, no existing signature changes, and no existing call site
  changes. The core is extended, not rewritten.
- **V Traceability** ✅ An agent node's exact model I/O lands on the chain as `LlmRequest` /
  `LlmResponse`, exactly as `NativeLoop` records a turn's — see the D6 finding below, which is the one
  place this slice added something the plan had not foreseen, and added it because omitting it would
  have been a bypass. Every executed node then lands exactly one `StepKind::WorkflowNode` step on the
  hash-chained Ledger before the engine moves on, and a `Node::Tool` additionally leaves the
  gateway's own unchanged `ToolCall` → `Approval` → `ToolResult` triple. Resume is *derived from* the
  chain by replay — there is no second source of truth for "what has this run done", which is the
  drift this principle exists to prevent. `Ledger::verify_chain` is asserted over a resumed run in
  `tests/resume.rs`, so the chain a second process appends to is proven intact rather than assumed.
- **VI Security** ✅ Deny-by-default is inherited rather than re-implemented: `Node::Tool` goes
  through `ToolGateway::call_captured`, so a tool absent from the policy's allowlist never reaches a
  transport, and a mutating tool without an approval is still refused with a reason. The engine adds
  no bypass and holds no secret; redaction stays where it is, inside the gateway. The `Approval`
  **node** is the confirmation-before-irreversible-action control at the graph level: it cannot be
  passed without a recorded human decision, and a `"rejected"` decision stops the run rather than
  falling through.
- **VII Neutrality** ✅ One new crate, three dependencies, no new external package. YAGNI is applied
  where it costs something and refused where it would cost more later: the `Node` enum names the full
  spec vocabulary now (so no serialized `Workflow` needs migrating) while only three variants
  execute. A trait-object executor registry, a `RunState` store beside the Ledger, a `RunId` newtype,
  a per-node `LoopController`, and a distinct `StepKind::WorkflowApproval` were each considered and
  rejected with a stated reason in `plan.md`.
- **VIII Loop discipline** ✅ NON-NEGOTIABLE, and **deliberately not engaged by this slice** — stated
  as a scoped decision (`plan.md` D4), not an oversight. There is no agentic loop here to govern: a
  `Node::Agent` is exactly one `ModelClient::turn` plus tool mediation for that turn, and a workflow
  is bounded by its `graph: Vec<Node>` being a finite, author-written list. (a)'s
  externally-enforced termination is a property of loops; a single bounded call terminates when the
  call returns or errors. Attaching a budget to a call site that cannot iterate would either never
  fire or fire wrongly. FR-017's ReAct/Reflexion/Self-Refine node **bodies** are where loop
  discipline actually applies, and they are the follow-up slice — `Node::Loop` refuses here rather
  than running unguarded, which is the conservative failure.
- **Cross-platform** ✅ No `#[cfg]`, no OS-specific call, no path handling. The crate is pure logic
  over in-memory types and compiles and tests identically on all three legs.

## Tasks

- [x] **T0** `specs/002-workflow-engine/{plan.md,tasks.md}` written; `spec.md`'s `**Status**` moved
      `Draft` → `Planned`
- [x] **T1** control baseline: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace`, each re-measured rather than quoted
- [x] **T2** types and signatures, no behaviour: the `heddle-workflow` crate skeleton (`Workflow`,
      `Node`, `WorkflowEngine`, `WorkflowRun`, `WorkflowExit`, `NodeRecord`, `WorkflowApproval`) with
      `todo!()` bodies; `StepKind::WorkflowNode` and `HeddleError::Unsupported` added to `heddle-core`
- [x] **T3** RED→GREEN — a 3-node sequential workflow, one `WorkflowNode` step per node, in order
      (`tests/sequential.rs`)
- [x] **T4** RED→GREEN — interrupt after node 2, resume at node 3, nodes 1 and 2 not re-executed
      (`tests/resume.rs`)
- [x] **T5** RED→GREEN — an `Approval` node blocks, re-polling does not grow the Ledger, `decide`
      resumes it, a rejection stops the run (`tests/approval.rs`)
- [x] **T6** RED→GREEN — a deferred node kind refuses **before** logging anything for that node
      (`tests/node_kinds.rs`)
- [x] **T7** gates, delta against T1, dependency drift, control diff

## Control baseline (T1)

On `feat/workflow-engine` @ `d364405`, working tree clean, Windows 11 Pro 10.0.26200, toolchain
1.97, 2026-09-05, before any edit:

- `cargo fmt --all --check` — clean, no output, exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, `Finished dev profile`, exit 0.
- `cargo test --workspace` — **228 passed, 0 failed, 5 ignored**: `acp_session` 16, `cli_acp_agent`
  16, `cli_chat` 12, `cli_ledger` 8, `cli_secret` 2, `connector` 9, `fs_root` 11, `fs_server` 7,
  `git_root` 5, `git_server` 13, `governed_fs_run` 4 (+1 ignored), `governed_git_run` 4 (+1 ignored),
  `governed_proc_run` 0 (+2 ignored), `run_server` 10, `core` 19, `native_loop` 25, `tool_gateway`
  14, `governed_run` 2, `openai_compat` 15 (+1 ignored), `rmcp_gateway` 9, `heddle-sandbox`
  `src/lib.rs` unit target 4, `escape` 4, `launch` 4, `profile` 3, `silo_ledger` 7, `silo_secret` 5.
  Every other `src/lib.rs` and `src/main.rs` unit target reports 0.

This matches slice 020's recorded close-out (228/5) exactly, which is the expected result of no
commit having touched a test since — and it is re-measured rather than quoted, per Principle III.

## Observed red

**T3** — `cargo test -p heddle-workflow --test sequential`, all six new tests, against T2's `todo!()`:

```
thread 'a_three_node_workflow_runs_every_node_in_order_and_reaches_its_final_result' panicked at
crates\heddle-workflow\src\engine.rs:64:9:
not yet implemented: T3: execute the unlogged remainder of the graph
test result: FAILED. 0 passed; 6 failed
```

The expected red, and the only genuine unwritten-code red of this run. See the shortfall recorded
below.

**T4, T5, T6 — no unwritten-code red, and the reason is a shortfall in how this run was sequenced
rather than a property of the steps.**

Principle III asks for the *minimal* implementation that turns a red green. T3's green was not
minimal: it implemented `WorkflowEngine::run` **whole** — the completed-node scan, the approval arm
and the deferred-kind arm together — because `run`'s body is one `match` over `Node` and writing one
arm while leaving the others `todo!()` felt like scaffolding. That judgement was wrong for this
codebase's bar. It cost T4, T5 and T6 their reds: each test passed the first time it was run, not
because the tests were weak but because the code they targeted already existed.

This is recorded rather than repaired by reverting `run` to `todo!()` and "re-observing" a red that
would be a rehearsal of a known implementation. Slice 020's `tasks.md` refuses to dress up a red and
this one inherits that. What is offered instead is a **measured counterfactual per step** — the
mechanism under test is neutered, the suite is re-run, and the failure is recorded. That is the same
substitute slice 020 used for its T4/T5, and it is stronger than a red in one specific way: a red
proves the code did not exist yet, while a counterfactual proves *this* line is what makes the test
pass, and it can be re-measured at any later date.

**T4 counterfactual** — the resume skip removed (the `continue` deleted from `run`'s
already-completed branch, so a logged node falls through to its executor):

```
test without_a_prior_chain_the_same_wiring_executes_every_node ... ok
test resuming_continues_at_node_three_and_completes ... FAILED
test resuming_appends_exactly_one_new_completion_step_and_rewrites_nothing ... FAILED
test the_already_logged_nodes_executors_are_never_entered ... FAILED
test a_run_with_nothing_left_to_do_is_idempotent ... FAILED
test the_chain_a_second_process_appends_to_still_verifies ... FAILED

thread '...' panicked at crates\heddle-workflow\tests\common\mod.rs:138:17:
this node's executor must never be entered, yet it reached a transport
test result: FAILED. 1 passed; 5 failed
```

Five of six fail, and they fail on the **fixture's own panic** rather than on an assertion — the
already-completed tool node's transport was genuinely re-entered. The one that passes is
`without_a_prior_chain_…`, which is the positive control: it asserts that with no prior chain the
same wiring *does* execute all three nodes, so the five absences above cannot be passing merely
because the fixture is inert.

**T5 counterfactual A** — the pending marker made non-idempotent (`Some(PENDING)` re-appends before
returning):

```
test re_polling_an_undecided_gate_repeats_the_answer_without_growing_the_ledger ... FAILED
assertion `left == right` failed: a slow human must not cost one Ledger step per poll
  … seq: 3, kind: Approval, payload: "{\"node_id\":\"sign-off\",\"decision\":\"pending\"}"
  … seq: 4, kind: Approval, payload: "{\"node_id\":\"sign-off\",\"decision\":\"pending\"}"
test result: FAILED. 7 passed; 1 failed
```

Exactly one test fails, and the duplicate `pending` step at `seq 4` is the unbounded growth D3's
idempotence note exists to prevent, visible in the failure output.

**T5 counterfactual B** — a rejection made to fall through to the next node (`Some(REJECTED)`
treated as a completed gate instead of returning `Rejected`):

```
test a_rejected_run_stays_rejected_and_stops_growing_the_chain ... FAILED
test a_rejection_ends_the_run_without_executing_the_next_node ... FAILED
thread '...' panicked at crates\heddle-workflow\tests\common\mod.rs:74:32:
test result: FAILED. 6 passed; 2 failed
```

Both rejection tests fail at `mod.rs:74` — the *script-exhausted* panic — which is the strongest
form this claim can take: node 3's executor was not merely reported as having run, it actually ran
and asked a model that had been scripted for no turns at all.

**T6 counterfactual** — the deferred-kind arm made to append a completion step *before* refusing:

```
test a_build_that_implements_the_kind_resumes_at_that_very_node ... FAILED
  left: Some("deferred")
test refusing_is_stable_and_appends_nothing_on_a_retry ... FAILED
test a_deferred_node_logs_nothing_at_all_for_itself ... FAILED
  left: ["first", "refine"]
test result: FAILED. 2 passed; 3 failed
```

This is the most informative of the four, because it reproduces the exact failure D1 predicts in
prose. `a_build_that_implements_the_kind_resumes_at_that_very_node` reports
`final_outcome = Some("deferred")`: the "future build" that finally knows how to execute the node
**skipped it**, because today's build had already written a completion step claiming it was done.
The workflow reports success having never run the node. That is why the refusal happens before the
append, and it is now a measured consequence rather than an argument.

## Finding: `Node::Agent` needed a `Redactor`, which the plan's engine did not have

Raised at T3 and resolved there; recorded because it changes a constructor this plan sketched, and
is written up in full as `plan.md`'s **D6**.

`plan.md` D3 sketches `WorkflowEngine { client, gateway }`. But `Node::Agent` makes a real
`ModelClient::turn` call, and Constitution V requires *"exact model I/O"* on the chain and says
traceability *"cannot be bypassed"* — `NativeLoop` appends `LlmRequest` before every turn and
`LlmResponse` after it (`native_loop.rs:99-112`). An agent node that called a model and recorded only
its own one-line `outcome` would make the workflow path capture strictly **less** than the
native-loop path: no prompt, no advertised tool list, no response object. That is a traceability
bypass by omission.

Capturing it needs a redactor, because the captured values are the raw conversation. So
`WorkflowEngine::new` gained a third argument, required rather than defaulted, for the reason
`native_loop.rs:44-48` states about its own: an optional redactor makes "records the conversation in
cleartext" the silent default. The node's recorded `outcome` is redacted on the same path, so a
completion step cannot carry a secret the `LlmResponse` step just had scrubbed out of it.

The visible consequence is in `sequential.rs`'s chain-shape assertion, which pins ten steps rather
than six for a three-node workflow: `LlmRequest, LlmResponse, WorkflowNode` per agent node, and the
gateway's unchanged `ToolCall, Approval, ToolResult, WorkflowNode` for the tool node.

## Finding: a `Node::Agent` runs the tools it asks for and is never told what they returned

Stated because it is a real limit of D4's one-turn node definition, and it is better read here than
rediscovered by whoever writes the ReAct slice.

D4 defines an agent node as one turn *plus tool mediation for that turn*. The mediation is real: the
call goes through `ToolGateway::call_captured`, the policy decides, and the chain gets its
`ToolCall`/`Approval`/`ToolResult` triple — `sequential.rs`'s
`an_agent_node_that_asks_for_a_tool_has_it_mediated_by_the_gateway` measures exactly that. But there
is no second turn in this slice, so the tool's result has nowhere to go. `NativeLoop::mediate` feeds
each result back as a `[tool_result …]` user message; here the feedback is produced and dropped.

That is coherent for a node whose contract is "one bounded call" — a workflow author who wants a
tool's output to influence a model puts a `Node::Tool` before a `Node::Agent` — but it does mean a
model that *chooses* to call a tool inside an agent node gets a worse deal than one driven by
`NativeLoop`. Whether an agent node should instead refuse tool calls outright, or become a bounded
multi-turn node, is the ReAct/Reflexion slice's question and is deliberately not answered here.

## Correction to `plan.md`'s file list

Two planned edits turned out to be unnecessary, and neither was made:

- **`crates/heddle-core/src/lib.rs`** — untouched. `StepKind` and `HeddleError` are already re-exported
  by name (`lib.rs:16-17`), so both new variants reach `heddle-workflow` with no new `pub use`.
  `git diff --stat crates/heddle-core/src/lib.rs` is empty.
- **Root `Cargo.lock`** — **not tracked by git in this repository** (`.gitignore:13`), so the planned
  "`Cargo.lock` UPDATE" is a non-event. The file does gain a `heddle-workflow` entry locally, listing
  `serde`, `serde_json`, `heddle-core` and nothing else, but it is not part of the diff and cannot be.

## Close-out (T7)

On `feat/workflow-engine`, working tree clean apart from this slice's own changes:

- `cargo fmt --all -- --check` — clean, no output, exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, no diagnostic, exit 0.
- `cargo test --workspace` — **253 passed, 0 failed, 5 ignored**. Against T1's 228/5 that is
  **+25 tests and no change to the ignored count**, which is exactly this slice's four new test
  binaries and nothing else: `sequential` 6, `resume` 6, `approval` 8, `node_kinds` 5.
- **No pre-existing assertion's text changed**, and no pre-existing file's behaviour changed. The two
  `heddle-core` edits are additive enum variants with doc comments; nothing else in that crate moved.
- **Control diff empty** outside `crates/heddle-workflow/`, `crates/heddle-core/src/{ledger.rs,error.rs}`
  and `specs/002-workflow-engine/` — verified with `git status --porcelain`, whose entire output is
  those three areas. Note that `crates/heddle-core/src/lib.rs`, named in the plan's allowlist, is
  **not** in the diff at all.
- **No dependency drift.** `heddle-workflow`'s `[dependencies]` are `heddle-core`, `serde`,
  `serde_json`; all three were already in the tree, so no package is added to the workspace. No
  existing manifest changed, and the root `Cargo.toml` needed no edit — `members = ["crates/*"]`
  picked the crate up as soon as its directory existed, exactly as fact 10 predicted.
- **No `unsafe`, no `#[cfg]`, no OS-specific call** anywhere in the new crate.

### Environment note, recorded because it interrupted the run and not the code

The first `cargo test --workspace` at T7 failed to build with `Espace insuffisant sur le disque
(os error 112)` across eight crates — the `D:` volume was at 100% with 3 MB free, this worktree's
`target/` accounting for 7.7 GB of it. Deleting `target/debug/incremental` (4.3 GB, wholly
regenerable) freed enough, and the suite was then re-run with `CARGO_INCREMENTAL=0`. **No source,
manifest or lockfile was touched to resolve it**, and the recorded 253/0/5 is from the clean re-run,
not from a partial one.

## Next slice

- **FR-017's loop-node bodies** (ReAct, Reflexion, Self-Refine, evaluator-optimizer) as executable
  `Node::Loop`, under a `LoopController`. This is the one node kind that genuinely needs Constitution
  VIII's machinery, and the finding above about an agent node's dropped tool feedback is its problem
  to solve properly.
- **`Node::Subagent`, `Node::Condition`, `Node::Parallel`.** `Parallel` is the one that will force a
  decision this slice avoided: node completion is currently keyed by id and walked in `Vec` order, and
  "position" stops being meaningful once branches interleave.
- **A CLI surface** — `heddle workflow run` / `heddle workflow decide` over the two public methods, so
  the engine has the authoritative client Constitution I asks for. Until then this crate is exercised
  only by its own tests.
- **Where workflow definitions live.** The tests build a `Workflow` in process; nothing yet persists
  one, and FR-013b's Goose/BMAD/Spec-Kit parsers need somewhere to put what they parse.
