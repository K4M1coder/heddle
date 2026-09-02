# Implementation Plan: native turn loop + ModelClient port (v0 strict-local)

**Branch**: `004-native-loop` | **Date**: 2026-09-03 | **Spec**: `specs/004-native-loop/spec.md`

## Summary
Compose the four inert modules of `skein-core` into a running loop. Add the provider port
(`ModelClient` + `TurnRequest`/`TurnResponse`) and the runner (`NativeLoop`, `ProgressProbe`,
`LoopRun`) that drives turns until `LoopController` says stop, writing every turn into the
hash-chained `Ledger`. This promotes the *design* validated by Spike 1 Option A (native
Skein-owned loop) — not its code, which stays quarantined under `spikes/` per ADR-0004 D2.

## Technical Context
**Language/Version**: Rust 1.79 (MSRV, pinned in `rust-toolchain.toml`)
**Primary Dependencies**: *none added* — `serde`, `serde_json`, `thiserror`, `sha2` already present
**Storage**: in-memory `Ledger` (durable SQLite-backed silo deferred to a later slice)
**Testing**: `cargo test`; hand-rolled `ScriptedModel`/`ScriptedProbe` doubles, no mocking crate
**Target Platform**: Windows + macOS + Linux
**Project Type**: library (single-crate workspace member)
**Performance Goals**: N/A (functional correctness first)
**Constraints**: offline (egress OFF), externally-enforced termination, append-only Ledger
**Scale/Scope**: one conversation, one provider port, one progress probe

## Constitution Check
*GATE: must pass before implementation.*
- **I. Headless core**: ✅ library API only; no UI, no `[[bin]]`.
- **II. Local-first / silo isolation**: ✅ no network, no new dependency; per-run Ledger isolation asserted.
- **III. Test-First**: ✅ `tests/native_loop.rs` written and observed red before any of
  `model.rs`/`native_loop.rs` existed.
- **IV. Inverted coupling**: ✅ the core names no provider; `ModelClient` and `ProgressProbe`
  are the seams.
- **V. Traceability**: ✅ every turn appended through the existing `Ledger::append`;
  `verify_chain` asserted on the happy path *and* the provider-error path.
- **VI. Security / secrets by reference**: n/a this slice (no secrets, no external content).
- **VII. Neutrality / YAGNI**: ✅ zero new dependencies; test doubles hand-rolled rather than
  pulling `mockall`; no `tool_calls`, no `raw` field, no async until a caller needs them.
- **VIII. Loop discipline (NON-NEGOTIABLE)**: ✅ — see Complexity Tracking; this slice *closes*
  spec 001's deferral for the core loop. **(d)** HITL escalation remains open.
- **Cross-platform**: ✅ pure Rust, no `#[cfg]`; tri-OS CI for `crates/` added by this slice
  (`.github/workflows/core.yml`) — `spikes.yml` covered only `spikes/**`.

## Project Structure

### Documentation (this feature)
```text
specs/004-native-loop/
├── spec.md      # this feature's requirements
├── plan.md      # this file
└── tasks.md     # executable breakdown
```

### Source Code (repository root)
```text
crates/skein-core/
  src/model.rs         # new — TurnRequest / TurnResponse / ModelClient
  src/native_loop.rs   # new — ProgressProbe / LoopRun / NativeLoop
  src/lib.rs           # +2 modules, +2 re-export lines
  src/error.rs         # +1 variant: SkeinError::Model(String)
  tests/native_loop.rs # new — 9 acceptance tests + ScriptedModel/ScriptedProbe
.github/workflows/core.yml  # new — tri-OS fmt/clippy/test for the workspace
```
**Structure Decision**: everything lands in `skein-core`. Design §4.2 places the agentic loop
in the core; a new crate would need a justification that does not exist. `ledger.rs`,
`loop_ctl.rs`, `content.rs` and `tests/core.rs` are not touched, so spec 003's 6/6 remains an
independent control.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| *(closure, not a violation)* spec 001's plan recorded Principle VIII as ⚠️ Partial — "wall-clock bound, no per-step `LoopController`", deferred to Epic 6 / FR-16 | This slice closes that deferral for the core loop: `LoopController` is now consulted per turn and is the only thing that can end a run. | n/a — nothing was traded away. HITL escalation (VIII(d)) remains open and is tracked in this spec's Assumptions, not here. |
