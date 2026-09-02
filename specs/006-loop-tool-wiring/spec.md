# Feature Specification: wire the Tool Gateway into the native loop (v0 strict-local)

**Feature Branch:** `006-loop-tool-wiring` · **Created:** 2026-09-03 · **Status:** Draft
**Input:** ADR-0003 (Accepted, decision A = native Skein-owned loop, decision 4 = tool
governance via `rmcp`) · design §4.2/§4.3/§4.11/§4.14/§7 · builds directly on
`specs/004-native-loop` (Implemented, 15/15) and `specs/005-tool-gateway` (Implemented, 26/26),
whose "Next slice" names this gap: *"wire the gateway into `NativeLoop`"*.

## User Scenarios & Testing

### User Story 1 — A model-requested tool actually runs, through the governor (P1)
As a user, when the model asks for a tool mid-conversation, the loop performs that call
through the `ToolGateway` and never around it, and the run carries on.
**Acceptance:**
1. **Given** a turn whose response carries `tool_calls: [read_file{path: "a"}]` and a
   following turn that claims final output, **When** the loop runs, **Then** the transport is
   invoked exactly once with the *raw* arguments and the run ends `Exit::FinalOutput`.
2. **Given** a turn carrying two tool calls, **When** the loop runs, **Then** both execute
   sequentially in declaration order and their results are fed back in that same order.
3. **Given** the same wiring against a **live** embedded MCP server, **When** the loop runs,
   **Then** the server's own invocation counter reads 1 and the next request carries the
   server's real (redacted) output.

### User Story 2 — A denied tool does not end the run, and is on the record (P1)
As a user, a governance refusal is a normal event: the model is told, the refusal is on the
chain, and the engine keeps its budget-driven control of the run.
**Acceptance:**
1. **Given** `fs_write` classified mutating and unapproved, **When** the model asks for it,
   **Then** the run returns `Ok`, the transport is never invoked, the run's kinds contain
   `ToolCall` then `Approval` with **no** `ToolResult`, and the `Approval` payload records
   `"denied"`.
2. **Given** the same run, **When** the next request is read back out of the Ledger, **Then**
   its history carries a `status=denied` message naming the tool.

### User Story 3 — Tool output enters the conversation as redacted data (P1)
As a user, what the model sees of a tool's output is byte-identical to what the tamper-evident
record holds, so a secret cannot re-enter the chain through the conversation history.
**Acceptance:**
1. **Given** a tool whose output contains a configured secret, **When** the loop feeds that
   result back, **Then** **no** payload of **any** step of the run contains the secret —
   including the *next turn's* `LlmRequest`, which carries the whole history — at least one
   payload contains `***`, and the fed-back message carries `***`.
2. **Given** any tool result, **When** it enters the conversation, **Then** it enters as one
   `Role::User` message (never `System`, never `Assistant`) prefixed by a
   `[tool_result tool=… status=…]` marker, carrying the tool's words rather than the model's.

### User Story 4 — Budgets still belong to the engine (P1)
As a user, tool activity buys the model no extra room: the `LoopController` decides when the
run ends exactly as it did before tools existed.
**Acceptance:**
1. **Given** a 1-iteration budget and a single turn that requests a tool and does not claim
   completion, **When** the loop runs, **Then** the run ends `Exit::MaxIters` after exactly
   one model call, one iteration and one transport call, and the last step is `Exit`.
2. **Given** an already-exhausted budget, **When** the loop is entered, **Then** zero model
   calls **and** zero tool calls occur.

### User Story 5 — The interleaved chain still verifies (P1)
As a user, turn steps and tool steps share one hash chain and one `run_id`.
**Acceptance:**
1. **Given** a two-turn run whose first turn calls one tool, **When** it completes, **Then**
   `verify_chain` passes, `seq` is gap-free `0..n`, and the kinds are exactly
   `[IterationBoundary, LlmRequest, LlmResponse, BudgetSpent, ToolCall, Approval, ToolResult,
   IterationBoundary, LlmRequest, LlmResponse, BudgetSpent, Exit]`.
2. **Given** a transport that errors, **When** the loop runs, **Then** it returns
   `SkeinError::Tool`, no `ToolResult` is fabricated, and `verify_chain` still passes.

### User Story 6 — The loop discovers tools through a port (P2)
As a developer, the loop is generic over `ToolTransport` and holds a concrete `ToolGateway`;
there is no mockable seam that could substitute an ungoverned mediator.

## Requirements
- **FR-001**: `TurnResponse` MUST carry `tool_calls: Vec<ToolCall>`, serde-defaulted so a
  payload captured by a pre-006 run still deserializes out of the Ledger.
- **FR-002**: `NativeLoop::run` MUST route every requested call through `ToolGateway`; there
  MUST be no path from the loop to a `ToolTransport` that skips policy, approval and
  redaction (Constitution VI).
- **FR-003**: Tool steps MUST be appended through the existing `Ledger::append` with the
  existing `StepKind::{ToolCall, Approval, ToolResult}`, on the run's one chain. No second
  ledger, no second `run_id` scheme, no new `StepKind` (Constitution V).
- **FR-004**: Tool results fed back into the conversation MUST be the gateway's **redacted**
  capture, MUST enter as `Role::User`, and MUST be labelled as tool data (Constitution VI).
- **FR-005**: A `SkeinError::ToolDenied` MUST NOT end the run; any other tool error MUST
  propagate as `Err` and MUST leave `verify_chain` passing.
- **FR-006**: `record_iteration` and `should_exit` MUST be called exactly once per iteration
  boundary regardless of tool calls, and `loop_ctl.rs` MUST NOT change (Constitution VIII(a)).
- **FR-007**: No new Cargo dependency and no new crate.

## Success Criteria
- **SC-001**: `fmt --check`, `clippy --workspace --all-targets -D warnings` and
  `cargo test --workspace` all clean; the suite is 26 pre-existing + 9 new = **35** tests.
- **SC-002**: The loop's tool call is proven against a **live embedded rmcp server**, not only
  against the in-core double.
- **SC-003**: `git diff` on every `Cargo.toml` in the repository is empty.
- **SC-004**: `git diff` on `crates/skein-core/tests/tool_gateway.rs` and
  `crates/skein-core/src/loop_ctl.rs` is empty; the four Exit-variant tests of spec 004 differ
  only by the added `NativeLoop::new` argument, so they remain controls.
- **SC-005**: `git diff` under `spikes/` is empty (ADR-0004 D2).
- As in specs 004 and 005, the macOS and Linux legs of `core.yml` are unobserved until the
  repository has a remote; only the Windows leg is run locally.

## Assumptions
- **Tools are executed sequentially in declaration order.** Streaming tool calls, parallel
  execution, inter-call dependency policy and per-call retry are not this slice's concern
  (Constitution VII).
- **Tools run before the progress probe and before the exit decision.** Design §4.14 and
  Constitution VIII(b) name tool results as a ground-truth reflection anchor, so a probe that
  ran first could never see the effect of the turn's own tool. The accepted consequence: the
  last turn before a budget exit still performs its tool's side effect, and a response that
  claims `final_output` *and* requests a tool executes the tool, records it, and then ends.
  Bounding that needs pre-execution cost estimation, which is deferred. What is *not*
  conceded: an exhausted budget still makes zero model calls and zero tool calls.
- **`ToolPolicy` has no allowlist, and the tool name is now model-chosen.** `decide` returns
  `Allow { reason: "not mutating" }` for any tool absent from the `mutating` list. In slice 005
  the caller was trusted product code; from this slice the *model* picks the name, so
  "anything not classified mutating is allowed" is a materially weaker posture than it was.
  Blast radius is bounded by the configured transport's own tool list — an unknown name fails
  downstream as `SkeinError::Tool` — and by the operator's choice of which server to connect.
  This is a named, open governance gap, carried into "Next slice"; it is not fixed here because
  adding a required allowlist changes `ToolPolicy::new`'s signature and would rewrite the
  eleven spec-005 tests this slice exists to keep as controls.
- **The tool *name* is not redacted on its way into the Ledger.** `tool.rs` redacts `args` and
  `content` but copies `call.tool` verbatim into both the `ToolCall` attempt and the
  `CapturedResult`. Pre-existing (spec 005), and only now reachable with model-supplied text.
  Recorded, not widened.
- **The `[tool_result …]` envelope is a marker, not a security boundary.** Design §7 item 5's
  prompt-injection concern is *not* discharged by this slice: tool output enters as `Role::User`
  data with a label, which is strictly better than `System` or `Assistant` and strictly less
  than a typed variant. The real fix is a `Content::ToolResult { .. }` variant; `content.rs`'s
  `Content` is `#[serde(tag = "type")]` with a single `Text` variant and `Message::text()`
  matches it exhaustively, so adding a variant later is additive but touches spec 003's
  controls. Deferred.
- **A `tools` field on `TurnRequest`** — advertising the available tools to the model — is not
  added. It needs tool discovery (`tools/list`), which spec 005 defers, and there is no real
  provider to advertise to. `model.rs`'s doc comment is corrected to say so rather than left
  stale; the scripted models in the tests name tools they already know.
- **Tool cost is unmetered.** `record_iteration` receives only `resp.tokens_used`, which is
  model metering; a tool that burns wall-clock or money is bounded only by `max_iters`. Design
  §4.14's `LoopBudget` sketches `max_cost` and `wall_clock`, which `loop_ctl.rs` does not
  implement — a divergence that predates this slice (spec 004 recorded the analogous
  `Exit::Error` gap).
- **HITL approval and `Exit::HumanReject` stay unreachable**; approval remains a configured
  list of tool names, exactly as spec 005 assumed. Constitution VIII(d) stays open.
- **Design §4.11's `Step` sketch carries `ts`, `principal` and `silo` fields** the v0 `Step`
  does not have. That gap predates this slice (spec 003) and is not widened here.
- The ACP client facade, the durable SQLite Ledger, `SecretProvider` and `skein-cli` remain the
  following slices.
