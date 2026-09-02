# Implementation Plan: wire the Tool Gateway into the native loop (v0 strict-local)

**Branch**: `006-loop-tool-wiring` | **Date**: 2026-09-03 | **Spec**: `specs/006-loop-tool-wiring/spec.md`

## Summary
Slices 004 and 005 are each correct and mutually disconnected: `ToolGateway::call` has no
caller in the product, and `NativeLoop::run` never mentions a tool. This slice composes the two
existing ports and nothing else. `TurnResponse` gains `tool_calls: Vec<ToolCall>`, `NativeLoop`
gains a third injected collaborator (`ToolGateway<T>`), and `run` mediates every requested call
through the gateway between the turn's `BudgetSpent` append and its `probe.observe()`.

The one non-obvious decision falls out of composing the two functions as written.
`ToolGateway::call` captures a **redacted** `CapturedResult` for the Ledger and returns the
**raw** `ToolOutcome` to the caller, deliberately — *"The tool needs the real secret; only the
record must not have it."* But `run` appends `serde_json::to_string(&req)` as the `LlmRequest`
payload, and `req.messages` is the whole history. Feeding the raw outcome back into `messages`
would therefore re-import the secret into the same hash chain one step later. So `tool.rs`
gains `call_captured`, which hands back **both** halves; `call` becomes a delegate; and the
loop feeds back the redacted capture. One redaction site, and the model's history is
byte-identical to what the chain records.

## Technical Context
**Language/Version**: Rust 1.97 (pinned in `rust-toolchain.toml`, unchanged this slice)
**Primary Dependencies**: none added. `crates/skein-core/Cargo.toml` declares exactly `serde`,
`serde_json`, `thiserror`, `sha2`, and gains nothing; no `Cargo.toml` in the repo changes.
**Storage**: the existing in-memory `Ledger` (durable SQLite-backed silo still deferred)
**Testing**: `cargo test`; a hand-rolled `RecordingTransport` double local to
`tests/native_loop.rs`, and the existing live in-process rmcp server fixture in `skein-mcp`.
**Target Platform**: Windows + macOS + Linux
**Project Type**: library (two workspace members, unchanged)
**Performance Goals**: N/A (functional correctness first)
**Constraints**: offline (egress OFF), deny-by-default for mutating tools, append-only Ledger,
no secret by value in any captured payload — now including the conversation history
**Scale/Scope**: one loop, one gateway, sequential tool execution in declaration order

## Constitution Check
*GATE: must pass before implementation.*
- **I. Headless core**: ✅ library API only; no UI, no `[[bin]]`. This is the slice that makes
  the headless core actually able to call a tool during a conversation.
- **II. Local-first / silo isolation**: ✅ no network; no new dependency in any crate.
- **III. Test-First**: ✅ all nine tests were written and observed red (T2) before the field,
  the constructor and the mediation existed (T3–T5).
- **IV. Inverted coupling**: ✅ the loop is generic over `T: ToolTransport` and never names a
  transport. A `ToolMediator` trait was rejected: it would make the *governed* step mockable.
- **V. Traceability**: ✅ tool steps go through the existing `Ledger::append` with the existing
  `StepKind`s, on the run's one chain; `verify_chain` is asserted on the interleaved path and
  on the transport-error path.
- **VI. Security / secrets by reference**: ✅ the loop feeds back the gateway's redacted
  capture, so the secret cannot re-enter the chain through the next turn's `LlmRequest`; tool
  output enters as `Role::User` data with a marker, never as `System` instruction. The
  envelope is explicitly *not* claimed as an injection boundary (spec Assumptions, R5).
- **VII. Neutrality / YAGNI**: ✅ no new module, no new crate, no new `StepKind`, no new
  dependency, no `NoTransport` null object, no second `run_with_tools` entry point.
- **VIII. Loop discipline (NON-NEGOTIABLE)**: ✅ **(a)** `record_iteration` and `should_exit`
  are still called exactly once per boundary, in the same fixed order, and `loop_ctl.rs` is
  byte-identical; a tool-bearing turn buys no extra iteration and an exhausted budget makes
  zero model *and* zero tool calls. **(b)** tools run *before* `probe.observe()` precisely so
  the ground-truth probe can see the effect of the turn's own tool — design §4.14 names tool
  results as a reflection anchor, and running them after the probe would make VIII(b)
  decorative and would trip `Exit::NoProgress` on tool-driven runs. **(c)** per-step Ledger
  capture now covers the tool attempt, the decision and the result; terminal verification is
  still the `Exit` step. **(d)** HITL escalation remains open: approval is a configured list.
- **Cross-platform**: ✅ pure Rust, no `#[cfg]`; `core.yml`'s `paths:` already covers
  `crates/**` and its toolchain pin is already 1.97, so no CI edit is needed.

## Project Structure

### Documentation (this feature)
```text
specs/006-loop-tool-wiring/
├── spec.md      # this feature's requirements
├── plan.md      # this file
└── tasks.md     # executable breakdown
```

### Source Code (repository root)
```text
crates/skein-core/
  src/model.rs             # +use crate::tool::ToolCall; +#[serde(default)] tool_calls field;
                           #  corrected doc comment (tool calls arrived; advertisement has not)
  src/tool.rs              # +ToolGateway::call_captured; `call` reduced to a delegate
  src/native_loop.rs       # +T: ToolTransport type param, +pub gateway field, +mediate helper
  tests/native_loop.rs     # +RecordingTransport, +no_tools(), +reply_with_tools(), +8 tests;
                           #  the 10 existing NativeLoop::new sites gain a third argument
crates/skein-mcp/
  tests/rmcp_gateway.rs    # +1 test driving NativeLoop against the live embedded server
```
**Structure Decision**: no new module and no new file outside `specs/006-loop-tool-wiring/`.
`lib.rs` needs no change — every type this slice uses is already re-exported. `loop_ctl.rs`,
`ledger.rs`, `content.rs`, `error.rs`, `tests/core.rs` and `tests/tool_gateway.rs` are
untouched, so specs 003, 004 (its four Exit-variant tests) and 005 remain independent controls.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| **`NativeLoop` gains a third type parameter and `new` changes arity**, touching ten construction sites in spec 004's test file (Principle VII: churn in a passing control) | Principle VI requires that there be no path from the loop to a transport that skips the governor. Owning a concrete `ToolGateway<T>` — not a trait object, not an `Option` — makes that structural rather than conventional, and `T: ToolTransport` is the *existing* seam from slice 005, so Principle IV is already satisfied without a new abstraction. | A `NoTransport` null object plus a second `impl` block so `NativeLoop::new(client, probe)` keeps compiling: machinery whose only caller is backwards compatibility, in a pre-1.0 repo with no remote and no external consumer. Worse, a "loop with no tools" cannot express deny-all under the current `ToolPolicy` (spec Assumptions, R3), so the null object would ship a *permissive* default. `run_with_tools(...)` as a second method: two loop algorithms, or one plus a delegating wrapper that still has to name a transport type for the tool-free case — same null-object problem, plus a second public entry point to keep in sync. The churn is bounded and reviewed: T9 requires the four Exit-variant tests to differ by the constructor argument and nothing else. |
| **`ToolGateway` gains a second public entry point, `call_captured`** (Principle VII: two methods where one existed) | The loop needs the redacted capture and the gateway is the only place that may compute it. Returning both halves from one implementation keeps redaction to a single site, and keeps `call`'s signature and semantics byte-identical so all eleven spec-005 tests stay controls. | The loop re-reading the last `ToolResult` step out of the Ledger: a search over the log for data the callee already held in hand. Exposing `ToolGateway::redact` and redacting a second time in the loop: two redaction sites that can drift, which is exactly the failure mode this slice exists to close. Feeding the raw outcome back: reintroduces the secret into the chain. |
