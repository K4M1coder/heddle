# Plan: slice 002 (User Story 1) — a native, Ledger-backed `WorkflowEngine`

**Target artifacts:** `specs/002-workflow-engine/{spec.md,plan.md,tasks.md}` (`spec.md` predates this
plan; its `**Status**` moves `Draft` → `Planned` at T0)
**Branch:** `feat/workflow-engine`, cut from `dev` at `d364405` (verified: `git rev-parse --short
HEAD` = `d364405`, working tree clean, `git remote -v` names a local bare repo — no PR host)
**New crate:** `crates/heddle-workflow`, a workspace member the moment its directory exists, by the
root `Cargo.toml`'s `members = ["crates/*"]` — no manifest edit.

**Scope, stated up front:** this slice implements **User Story 1 only** — sequential nodes, one
`Approval` node, resume-after-interrupt — from `spec.md`'s three acceptance scenarios. User Story 2
(`TaskTracker` + the config hierarchy) and User Story 3 (tracker progress sync) are untouched.
FR-017's ground-truth loop-node types (ReAct, Reflexion, Self-Refine, evaluator-optimizer) are **not**
implemented here: `Node::Loop` exists in the type because `spec.md`'s Key Entities name the full node
vocabulary, but its executor arm refuses. See *Out of scope*, which is exhaustive on purpose.

---

## Problem

`spec.md` FR-013/FR-013a require a `WorkflowEngine` that executes a graph of nodes, lands one Ledger
`Step` per node, and is **resumable from the last logged step**. Nothing in the tree does this.
`NativeLoop` drives *turns* against one model until a controller stops it; it has no notion of a
graph, of a node, or of picking up where a dead process left off. `grep -r resume crates/` finds one
unrelated hit (a word in a `heddle-sandbox/src/launch.rs` comment) — there is no resume primitive and
no prior art to mirror, so the design below is this slice's own.

Without it the product cannot run a multi-step SDLC chain (plan → code → test → package) at all,
which is Epic 6's requested capability and `spec.md`'s SC-004.

## What was verified before planning

Everything below was read in this worktree (`feat/workflow-engine`, tip `d364405`).

1. **`crates/heddle-core/src/ledger.rs:92-116`.** `Ledger::append(run_id, kind, payload) ->
   Result<String>` derives `seq` and `parent` by scanning `self.steps` for the same `run_id`. So
   replaying the same `run_id` against a `Ledger` opened from a durable store reconstructs the exact
   same chain shape, and a second `append` for a `run_id` continues the sequence rather than
   restarting it. `Ledger::open` (`ledger.rs:79`) already resumes whatever a `LedgerStore` holds —
   `store.load()?` populates `steps` before any new append. This is the primitive resume rides on;
   **nothing new is needed in `heddle-core` for the store side.**
2. **`StepKind`** (`ledger.rs:13-24`) is a closed enum: `LlmRequest, LlmResponse, ToolCall,
   ToolResult, StateChange, Reflection, IterationBoundary, BudgetSpent, Exit, Approval`. None names
   "one workflow node completed". A new variant is needed (D2). Its two match sites both tolerate a
   new variant without an edit: `heddle-acp/src/lib.rs:138` has a `_ => {}` arm, and
   `heddle-cli/src/ledger.rs:66` matches on the **serde value**, not the enum, deliberately.
3. **`crates/heddle-core/src/tool.rs`** already has an `Approval` *concept*, at a different layer:
   `ToolGateway::call_captured` (`tool.rs:312`) appends a `StepKind::Approval` step recording whether
   a *mutating tool call* was allowed by `ToolPolicy` — an automatic, policy-driven decision, not a
   human one — as `ApprovalRecord { tool, decision, reason }` (`tool.rs:246-251`). The workflow
   spec's `Approval` **node** is a different thing: a graph node that blocks until a **human** records
   a decision out of band. Reusing the `StepKind` is defensible (both mean "a decision was required
   and recorded"), but the payload shapes must stay distinct types so a reader of `heddle ledger show`
   is not handed one shape and told it is the other. D2 keeps them as two payload types under one
   kind.
4. **`crates/heddle-core/src/native_loop.rs:36,43`.** `NativeLoop<C: ModelClient, P: ProgressProbe,
   T: ToolTransport>::run` is generic over the model client and the tool transport, never naming a
   protocol (Constitution IV). There is **no `Agent` trait** in `heddle-core`: "an agent" *is* the
   pairing of a `ModelClient` with a `ToolGateway`, exactly as `NativeLoop` embodies it. A
   `Node::Agent` therefore needs no new core trait — it needs the engine to be generic the same way.
5. **`crates/heddle-core/src/model.rs:47-48`** — `ModelClient::turn(&mut self, req: &TurnRequest) ->
   Result<TurnResponse>`, synchronous, exactly one round trip.
6. **`crates/heddle-core/src/tool.rs:312`** — `call_captured` is the complete governed path
   (`ToolCall` step → policy decision → `Approval` step → `ToolResult` step, or `Err(ToolDenied)`
   without reaching the transport). `Node::Tool`'s executor is a thin wrapper over it, not a
   re-implementation.
7. **`crates/heddle-core/src/error.rs:6-38`.** No `Unsupported`/`NotImplemented` variant exists. One
   is needed for the deferred node kinds (D1).
8. **`crates/heddle-core/src/lib.rs:14-24`** re-exports everything a downstream crate needs. So
   `heddle-workflow`'s `[dependencies]` names only `heddle-core`, `serde`, `serde_json` — never a
   concrete connector crate. That is what keeps Constitution IV's inverted coupling true for the new
   crate by construction rather than by review.
9. **`grep -r resume crates/`** — one unrelated hit. No existing resume primitive or precedent.
10. **Root `Cargo.toml:3`** — `[workspace] members = ["crates/*"]`; `[workspace.dependencies]`
    already pins `serde` and `serde_json`, referenced as `{ workspace = true }`, the pattern every
    existing crate uses.
11. **Test-double conventions**, `crates/heddle-core/tests/native_loop.rs:17,89`: a `ScriptedModel`
    (`ModelClient` replaying a fixed `Vec<TurnResponse>`, one entry per call, **panicking loudly if
    the script is exhausted** — "the engine asked for a turn it wasn't scripted for" is a test bug,
    not a silent default) and a `RecordingTransport` (`ToolTransport` with a `TransportMode` enum:
    `Reply`/`Fail`/`Forbidden`). `heddle-workflow`'s fixtures mirror this shape rather than reaching
    for a mocking library.
12. **Plan/tasks house style**, from `specs/018-acp-permission-gate/plan.md` and
    `specs/020-run-dir-allowlist/{plan.md,tasks.md}`.

---

## Approach

One sentence: **a `Workflow` is a named, ordered `Vec<Node>`; `WorkflowEngine::run` replays a run's
Ledger to learn which nodes are already logged, executes exactly the unlogged remainder in order, and
returns as soon as it either finishes or reaches an `Approval` node with no decision on the chain —
never blocking a thread, so "interrupted" and "paused on a human" are the same code path.**

### D1 — `Node` names the full spec vocabulary now; only three variants execute in this slice

```rust
pub enum Node {
    Agent     { id: String, prompt: Message },
    Tool      { id: String, call: ToolCall },
    Approval  { id: String, message: String },
    Subagent  { id: String, workflow: String },
    Condition { id: String, on: String },
    Parallel  { id: String, branches: Vec<String> },
    Loop      { id: String, body: String },
}
```

`spec.md`'s Key Entities (`Node: agent/tool/subagent/approval/cond/parallel/loop`) is the type's
contract, not a suggestion. Building only `{Agent, Tool, Approval}` today and adding the rest later
would force every `Workflow` serialized by this slice to be migrated. The four deferred variants'
arm returns `Err(HeddleError::Unsupported(_))` **before appending any Ledger step for that node** — so
a workflow that reaches one fails loudly and leaves no partial or misleading step behind, and
retrying after a future slice implements the kind resumes cleanly at that same node, because nothing
was logged for the engine to skip past.

**Rejected — ship only `{Agent, Tool, Approval}` in the enum.** The migration cost above, and the
spec is explicit about the vocabulary. Constitution IV's "adding a capability = adding an
implementation behind an interface" reads more naturally as "the interface is stated once;
implementations catch up per variant" than as a type that grows breaking variants slice by slice.

**Rejected — `Box<dyn NodeExecutor>` per node, from a registry.** No such registry exists anywhere in
the tree (fact 4), and `NativeLoop`'s own pattern is a generic struct over `ModelClient`/
`ToolTransport`, never a trait object for "the agent" itself. `WorkflowEngine<C, T>` over the same
two ports covers every variant this slice executes; a registry would be new coupling machinery for
nothing.

### D2 — Two additive primitives in `heddle-core`; nothing existing changes shape

`StepKind` gains `WorkflowNode`. `HeddleError` gains `Unsupported(String)`. Both are additive: no
existing variant's meaning changes and no existing call site's behaviour changes.

The payloads live in `heddle-workflow`, not `heddle-core` — exactly as `heddle-core` already does for
`Approval`/`ToolResult`, whose payload types (`ApprovalRecord`, `CapturedResult`) live in `tool.rs`
beside the code that produces them:

```rust
struct NodeRecord       { node_id: String, outcome: String }
struct WorkflowApproval { node_id: String, decision: String }  // "pending" | "approved" | "rejected"
```

`StepKind::Approval` is **reused** for the workflow-level gate, with its own payload type distinct
from `tool.rs`'s `ApprovalRecord` — fact 3 records why that reuse is defensible.

**Rejected — a `StepKind::WorkflowApproval` distinct from `StepKind::Approval`.** Two variants
meaning "a decision was recorded here" would ask every future reader of the Ledger CLI to know which
to look for. One kind, two payload shapes distinguished by which crate produced them — already true
of the *existing* `Approval` kind, which today only `tool.rs` produces — is the smaller addition.

**Rejected — encode "which node" by `seq`/position alone.** A resumed run must be able to say *which*
node is pending without recomputing graph position from `seq`, especially once `Parallel` (a
follow-up) makes "position" ambiguous. Every `Node` variant carries an `id` for this reason.

### D3 — Resume is "replay the Ledger, skip what is already there", not a separate resume API

```rust
pub enum WorkflowExit {
    Completed,
    AwaitingApproval { node_id: String },
    Rejected         { node_id: String },
}
pub struct WorkflowRun { pub exit: WorkflowExit, pub final_outcome: Option<String> }

pub struct WorkflowEngine<C: ModelClient, T: ToolTransport> {
    pub client: C,
    pub gateway: ToolGateway<T>,
    redactor: Redactor,   // see D6
}
```

`run` scans `ledger.log(run_id)` once for `StepKind::WorkflowNode` entries (completed nodes, by
`NodeRecord::node_id`) and for the **last** `StepKind::Approval` entry per `node_id` that parses as a
`WorkflowApproval` (the workflow gate's state). It then walks `workflow.graph` in order:

- a node whose `id` already has a completed `NodeRecord` is **skipped** — not re-executed, not
  re-logged. That is the whole of SC-001;
- `Node::Agent`/`Node::Tool` execute via `self.client`/`self.gateway` (D4), then append their
  `NodeRecord` and continue;
- `Node::Approval` with no `WorkflowApproval` on the chain appends exactly one
  `WorkflowApproval{decision: "pending"}` and returns `Ok(AwaitingApproval{node_id})` — **without**
  touching a thread, a timer, or any blocking primitive;
- `Node::Approval` whose last decision is `"approved"` appends its `NodeRecord` completion step and
  continues to the next node **in the same call**: an approval, once decided, does not need a second
  `run` to take effect;
- `Node::Approval` whose last decision is `"rejected"` returns `Ok(Rejected{node_id})`. A rejection
  is a normal, named exit — not an error and not an unsupported operation;
- the four deferred kinds hit D1's `Unsupported` before anything is appended for that node.

`decide(run_id, node_id, approved, ledger)` appends exactly one `WorkflowApproval` with
`"approved"`/`"rejected"`. It does **not** itself resume execution — the caller calls `run` again,
which is one Ledger scan away from continuing. This mirrors `Ledger::append`'s own
append-then-let-the-reader-act shape rather than inventing a callback.

**Idempotence of the "pending" marker**, spelled out because it is the subtle part: if `run` is
re-invoked before a human decides, it re-scans, finds the last `Approval` for that `node_id` still
says `"pending"`, and returns `AwaitingApproval{node_id}` again **without appending a second
`"pending"` step**. Repeated interrupt/resume cycles on an undecided node therefore cannot grow the
Ledger without bound, which T5 exercises directly.

**Rejected — `run` blocks (`thread::park`, a channel `recv`) until a decision arrives.** A blocked
thread is not a resumable process, it is a paused one. A real interruption — the process killed —
must be recoverable by a **new** process opening the same Ledger, which only the poll-on-`run` shape
supports. It also matches the CLI's synchronous, one-shot-per-invocation style throughout.

**Rejected — a `RunState` struct persisted beside the Ledger.** A second source of truth for "what
has this run done" is exactly the drift Constitution V's Ledger exists to prevent; `spec.md`'s own
`WorkflowRun` entity is defined as *derived from* the Ledger, not stored beside it.

### D4 — `Node::Agent` is one bounded turn, `Node::Tool` one governed call; neither owns a `LoopController`

`Node::Agent`'s executor in full: build `TurnRequest{run_id, messages: vec![prompt.clone()], tools:
self.gateway.advertise()?}`, call `self.client.turn(&req)`, mediate `resp.tool_calls` through
`self.gateway.call_captured` exactly as `NativeLoop::mediate` already does (fact 6), and log the
model's final message text as the node's `outcome`. `Node::Tool`'s executor calls `call_captured`
once and logs the `CapturedResult.content`.

Neither takes a `LoopBudget`/`LoopController`. FR-017's engine-enforced loop discipline governs
*iteration inside* a node once ReAct/Reflexion/Self-Refine bodies exist (the deferred work), not "how
many nodes a workflow has" — a workflow is already bounded by its `graph: Vec<Node>` being a finite,
author-written list. **There is no open-ended iteration in this slice for a budget to guard**, so
adding one now would be budget theatre around a loop that cannot occur.

**Rejected — wrap every `Node::Agent` in a `NativeLoop` with a generous default budget "for safety".**
A default budget on a call site that cannot iterate either never fires (dead code a reader must still
reason about) or fires wrongly on a legitimately slow single turn. Constitution VIII(a)'s
externally-enforced termination is a property of *loops*; a single bounded call needs no termination
policy beyond "the call returns or errors", which `ModelClient::turn`'s `Result` already gives.

### D5 — `Workflow` matches `spec.md`'s Key Entities exactly

```rust
pub struct Workflow { pub name: String, pub params: serde_json::Value, pub graph: Vec<Node> }
```

`RunId` ("executed instance, addressed by `RunId`, derived from the Ledger") is the `run_id: &str`
`Ledger::append`/`log` already key on — no new type, since `heddle-core` already treats run identity
as a plain string and a newtype here would be a second representation of the Ledger's own key.

### D6 — the engine carries a `Redactor`, because a node that calls a model must record what it said

**Added during implementation (T3), not planned. Recorded here rather than in
`tasks.md` alone, because it changes a constructor signature this plan sketched.**

`Node::Agent` makes a real `ModelClient::turn` call. Constitution V says every step — *"exact model
I/O"* — is captured, and *"traceability cannot be bypassed"*; `NativeLoop` accordingly appends an
`LlmRequest` before each turn and an `LlmResponse` after it. A `Node::Agent` that called a model
without recording the exchange would make the workflow path capture strictly **less** than the
native-loop path, which is a bypass by omission rather than by design.

Recording it needs a redactor, since the request and response are the raw conversation. So
`WorkflowEngine::new` takes one, and it is **required rather than optional** for the reason
`NativeLoop::new` states about its own: an optional redactor would make "this workflow records its
conversation in cleartext" the silent default, which is the bug it exists to prevent (Constitution
VI). It is the one private field, again mirroring `NativeLoop` — a caller configures it and never
reads it back.

The node's recorded `outcome` is redacted on the same path, so the completion step cannot carry a
secret the `LlmResponse` step just had scrubbed out of it.

**Rejected — record nothing and let the node's `outcome` stand as the trace.** The outcome is the
model's final text; it is not the prompt, the advertised tool list, or the token count, and a run
that has to be audited or replayed needs all four. It would also mean the only way to get a fully
traced model call was to avoid the workflow engine.

**Rejected — give the redactor a `Default` so the constructor stays two-argument.** Precisely the
silent-cleartext default Constitution VI forbids, and `NativeLoop::new` already refused the same
convenience for the same reason.

---

## Steps (strict TDD; red observed and recorded in `tasks.md` before each green)

- **T0** — this `plan.md`, `tasks.md`, and `spec.md`'s `Status` → `Planned`.
- **T1** — control baseline: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --
  -D warnings`, `cargo test --workspace`, each **measured** on the implementation machine.
- **T2** — crate skeleton, no behaviour: `crates/heddle-workflow/{Cargo.toml,src/lib.rs,src/node.rs,
  src/engine.rs}` with `todo!()` bodies; `StepKind::WorkflowNode` and `HeddleError::Unsupported` added
  (D2). Gate: `cargo build --workspace` green, no test yet.
- **T3** — RED→GREEN: a 3-node sequential workflow reaches its final result, one `WorkflowNode` step
  per node, in order (`tests/sequential.rs`).
- **T4** — RED→GREEN: interrupting after node 2 and resuming does not re-execute node 1 or 2
  (`tests/resume.rs`), against a **second, independent** engine/model/transport so a call reaching an
  already-completed node's executor is structurally impossible, not merely unasserted.
- **T5** — RED→GREEN: an `Approval` node blocks; re-polling it does not grow the Ledger; `decide`
  resumes it; a rejection ends the run without the next node (`tests/approval.rs`).
- **T6** — RED→GREEN: a deferred node kind fails with `Unsupported` **before** logging anything for
  that node (`tests/node_kinds.rs`).
- **T7** — gates and close-out: re-run all three, record the delta against T1, confirm the control
  diff is empty outside `crates/heddle-workflow/`, `crates/heddle-core/src/{ledger.rs,error.rs,lib.rs}`,
  `specs/002-workflow-engine/` and the root `Cargo.lock`.

---

## Validation

### Project gates (must all pass)
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace` — T1's baseline plus exactly T3–T6's new tests, no pre-existing assertion
  text changed.

### New tests

| Test | File | What it proves |
|---|---|---|
| a 3-node sequential workflow reaches its final result, one step per node, in order | `tests/sequential.rs` | FR-013/FR-013a sequencing and one-`Step`-per-node (Acceptance Scenario 1) |
| interrupting after node 2 and resuming continues at node 3, not re-executing 1/2 | `tests/resume.rs` | SC-001, Acceptance Scenario 2 |
| an `Approval` node blocks until a decision is recorded, then resumes | `tests/approval.rs` | Acceptance Scenario 3 |
| a `"rejected"` decision ends the run without running the next node | `tests/approval.rs` | The negative half of Scenario 3, implied by "waits for human validation before continuing" |
| re-polling an undecided `Approval` node does not grow the Ledger | `tests/approval.rs` | D3's idempotence note; guards against unbounded growth on a slow human |
| a deferred node kind fails before logging anything for that node | `tests/node_kinds.rs` | D1's fail-loud-and-clean, and that a future slice can resume safely at that node |

---

## Risks and mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| The `StepKind`/`HeddleError` additions read as "rewriting the core" (Constitution IV) | Low | Medium | Both are additive enum variants, the same shape as slices 007→020's own additions; no existing variant's meaning changes, and both match sites already tolerate a new variant (fact 2) |
| A future `TaskTracker` (US2) wants to observe node completion and `NodeRecord` carries too little | Medium | Low | `NodeRecord`/`WorkflowApproval` are private to `heddle-workflow`; their fields can grow behind `#[serde(default)]` — the pattern `TurnResponse::tool_calls` already uses — without breaking T3–T6 |
| `Node::Agent`'s one-turn definition proves too thin for a real SDLC chain (SC-004) | Medium | Medium | Stated as a deliberate, scoped simplification (D4), not discovered later; the follow-up adding ReAct/Reflexion bodies is exactly where a node needs more than one turn |

---

## Out of scope (explicit, so nobody helpfully does it)

- **User Story 2** — `TaskTracker` (local/Vikunja/Jira), `ConfigScope`, the Silo▸Team▸Project▸
  Conversation hierarchy and its locking rule (FR-014/FR-015). `WorkflowEngine` in this slice reads
  and writes nothing tracker-related.
- **User Story 3** — workflow progress reflected into a tracker (FR-016's tracker half, SC-003).
- **FR-017's ground-truth loop-node types** as actual node bodies — `Node::Loop` exists in the type
  and refuses when reached (D1).
- **`Node::Subagent`, `Node::Condition`, `Node::Parallel`** as executable behaviour — same treatment.
- **Goose recipes / BMAD / Spec-Kit flows executable as workflows** (FR-013b) — needs a parser from
  those formats into `Workflow`; this slice builds the engine such a parser would target.
- **A CLI subcommand** (`heddle workflow run`/`decide`) — this slice is the library crate only,
  exercised by its own tests. Wiring a CLI surface is analogous to how `heddle-cli` wires `NativeLoop`
  today, and is left for the same follow-up that would add the tracker CLI.
- **Persisting `Workflow` definitions** (a registry/store) — the tests construct a `Workflow` in
  process. Where definitions live is a question this slice does not answer, only that a `Workflow`
  value, however obtained, is what `run` consumes.
- **Any Temporal/Windmill backend** — `spec.md`'s Assumptions name these as optional future backends
  "behind `WorkflowEngine`"; this slice does not define that trait boundary, only the native engine.
  Extracting a trait later is mechanical: the public surface is `run` and `decide`.
