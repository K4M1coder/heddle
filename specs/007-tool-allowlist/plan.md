# Implementation Plan: deny-by-default for tool identity (v0 strict-local)

**Branch**: `007-tool-allowlist` | **Date**: 2026-09-03 | **Spec**: `specs/007-tool-allowlist/spec.md`

## Summary
`ToolPolicy` holds one list — `mutating` — and `decide` allows anything absent from it. This
slice replaces that field with `allowed: Vec<(String, ToolAccess)>`, where `ToolAccess` is
`ReadOnly | Mutating`. A name absent from `allowed` is denied outright; a name present is then
subject to exactly the approval rule it is subject to today. The `mutating` field is **deleted**,
not kept beside the new one.

Folding mutability *into* the allowlist rather than adding a second list is the whole design
decision. Two parallel string lists would be two configuration axes for one question, and would
let an operator classify `fs_write` mutating while forgetting to allowlist it — or the reverse.
With one list carrying a class per entry, each name is classified exactly once and
"mutating but not allowlisted" is unrepresentable. It also keeps `new`'s two parameters at
*different* types, where `new(allowed, mutating, approved)` would have offered three adjacent
`Vec<String>` positional parameters that no type check distinguishes — a silent-swap footgun in
the one type whose job is refusing things.

Nothing else in `tool.rs` changes. `call_captured` already turns a `Decision::Deny` into
`HeddleError::ToolDenied` after appending `[ToolCall, Approval]`, and `NativeLoop::mediate`
already feeds a `ToolDenied` back to the model as
`[tool_result tool=… status=denied]` and carries on. The unlisted-tool path *is* the existing
denial path, byte for byte, with no second mechanism.

## Technical Context
**Language/Version**: Rust 1.97 (pinned in `rust-toolchain.toml`, unchanged this slice)
**Primary Dependencies**: none added; no `Cargo.toml` in the repository changes.
**Storage**: the existing in-memory `Ledger` (durable SQLite-backed silo still deferred)
**Testing**: `cargo test`; the existing `CountingTransport` / `RecordingTransport` doubles and
the existing live in-process rmcp server fixture in `heddle-mcp`.
**Target Platform**: Windows + macOS + Linux
**Project Type**: library (two workspace members, unchanged)
**Performance Goals**: N/A. `decide` stays a linear scan over a handful of names.
**Constraints**: offline (egress OFF), deny-by-default for tool identity *and* for mutation,
append-only Ledger, no secret by value in any captured payload
**Scale/Scope**: one type's field, one enum, one `decide` body, four construction sites

## Constitution Check
*GATE: must pass before implementation.*
- **I. Headless core**: ✅ library API only; no UI, no `[[bin]]`.
- **II. Local-first / silo isolation**: ✅ no network, no new dependency in any crate.
- **III. Test-First**: ✅ all five tests were written and the compile failure observed and
  recorded (T2) before `ToolAccess` and the new `decide` existed (T3).
- **IV. Inverted coupling**: ✅ nothing about the transport seam changes; `ToolPolicy` names no
  protocol and no server.
- **V. Traceability**: ✅ an unlisted tool is refused through the *existing* `[ToolCall,
  Approval]` shape and the existing `HeddleError::ToolDenied`; no new `StepKind`, no second
  denial path, and `verify_chain` is asserted on the new refusal.
- **VI. Security / secrets by reference**: ✅ this slice exists to restore the principle's
  opening clause. Deny-by-default now governs tool *identity* and not only mutation, and it
  fails closed: `ToolPolicy::new(Vec::new(), Vec::new())` is a policy that allows nothing.
- **VII. Neutrality / YAGNI**: ✅ no new module, crate, `StepKind`, dependency or builder type.
  One enum and a changed field. A superseded field is deleted rather than kept alongside.
- **VIII. Loop discipline (NON-NEGOTIABLE)**: ✅ **(a)** `loop_ctl.rs` and `native_loop.rs` are
  byte-identical; a denied tool burns the same iteration budget the mutating-denial path already
  burns. **(b)** the probe still runs after the turn's tools, unchanged. **(c)** per-step capture
  of the attempt and the decision is unchanged — the refusal reason is the only new text.
  **(d)** HITL escalation stays open: approval remains a configured list, deliberately *not*
  folded into `ToolAccess`, so that seam survives for interactive approval.
- **Cross-platform**: ✅ pure Rust, no `#[cfg]`; `core.yml`'s `paths:` already covers `crates/**`
  and its toolchain pin is already 1.97, so no CI edit is needed.

## Project Structure

### Documentation (this feature)
```text
specs/007-tool-allowlist/
├── spec.md      # this feature's requirements
├── plan.md      # this file
└── tasks.md     # executable breakdown
```

### Source Code (repository root)
```text
crates/heddle-core/
  src/tool.rs                  # +ToolAccess; ToolPolicy.mutating -> allowed; new `decide`
  src/lib.rs                   # +ToolAccess in the `pub use tool::{…}` list
  tests/tool_gateway.rs        # `fn gateway` migrated; +3 tests
  tests/native_loop.rs         # `fn gateway` migrated; `fn no_tools` doc corrected; +1 test
crates/heddle-mcp/
  tests/rmcp_gateway.rs        # `fn live_server` delegates to `live_server_allowing`; +1 test
```
**Structure Decision**: no new file outside `specs/007-tool-allowlist/`. `native_loop.rs`,
`loop_ctl.rs`, `ledger.rs`, `error.rs`, `model.rs`, `content.rs`, `heddle-mcp/src/lib.rs` and
`tests/core.rs` are untouched, so specs 003, 004 and 006 remain independent controls — as do the
bodies of all six spec-005 gateway tests.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| **`ToolPolicy::new` changes shape and drops the `mutating` parameter, breaking every construction site** (Principle VII: churn in passing controls) | Principle VI's deny-by-default cannot be expressed unless the allowlist is a *mandatory* constructor argument: any constructor that omits it is a permissive default, which is the defect. The churn is bounded and enumerated — three helper functions and one literal, no pre-existing test body. Spec 006's estimate that an allowlist "would rewrite the eleven spec-005 tests" was verified against the code this slice and does not hold: every construction is confined to a helper. | `new(allowed: Vec<String>, mutating: Vec<String>, approved: Vec<String>)`: smaller diff, but three adjacent `Vec<String>` positional parameters that no type check distinguishes, so a swapped pair silently reclassifies every tool — in the one type whose job is refusing things. It also keeps two sources of truth for one tool's classification, letting `mutating` name tools the allowlist does not contain. A builder (`ToolPolicy::builder().allow("x").allow_mutating("y").build()`): fixes the footgun but adds a second type, four methods and a build step whose only callers are four test helpers (Principle VII); revisit when an operator-facing config file lands. Enforcing the allowlist in `ToolGateway::call_captured` instead: splits the governance decision across two places, so `Decision` stops being the whole answer and a future caller of `decide` gets the wrong verdict. Folding `approved` into the enum as `Mutating { approved: bool }`: collapses a runtime axis into static configuration and closes the seam Constitution VIII(d) needs for interactive approval. |
