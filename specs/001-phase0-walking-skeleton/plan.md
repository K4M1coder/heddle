# Implementation Plan: Phase 0 — Vertical Skeleton

> **Status: BLOCKED / REQUIRES REGENERATION.** ADR-0003 supersedes the former headless-Goose subprocess assumption. Run the runtime, workflow, context and Tool Gateway spikes, then regenerate this plan and `tasks.md` through the BMAD × Spec-Kit gates before implementation.

**Branch**: `001-phase0-walking-skeleton` | **Date**: 2026-07-15 | **Spec**: `specs/001-phase0-walking-skeleton/spec.md`

## Summary
Prove Heddle's vertical slice (CLI → Heddle control plane → selected per-turn runtime/worker → model gateway → local model) with Local silo persistence, an event-sourced Ledger, context manifest and SecretProvider foundation. The concrete runtime is selected by ADR-0003 evidence; a batch CLI subprocess is not a valid core-loop implementation.

## Technical Context
**Language/Version**: Rust 1.79 (MSRV)
**Primary Dependencies**: tokio, rusqlite (bundled), reqwest, serde/serde_json, clap, thiserror/anyhow, tracing, sha2, keyring, zeroize; Goose (external binary), LiteLLM (external proxy)
**Storage**: SQLite (local file), namespaced per silo
**Testing**: cargo test; wiremock (HTTP), assert_cmd/predicates (CLI E2E), tempfile; `goose` binary stub; provider mock
**Target Platform**: Windows + macOS + Linux (first-class cross-platform)
**Project Type**: desktop-app / CLI (multi-crate Cargo workspace)
**Performance Goals**: N/A in Phase 0 (functional correctness first)
**Constraints**: offline-capable (Local mode, egress OFF); secrets by reference; per-silo isolation
**Scale/Scope**: skeleton — 1 silo (local), 1 provider (local), 1 connector (fs via Goose)

## Constitution Check
*GATE: must pass before implementation.*
- **I. Headless core / CLI as source of truth / UI as an overlay**: ✅ CLI = only surface in Phase 0; no UI.
- **II. Local-first / silo isolation**: ✅ Local mode only; isolation test (Story 1.3).
- **III. Test-First**: ✅ each task follows red→green→refactor.
- **IV. Inverted coupling**: ✅ Goose behind `AgentRuntime`, model behind `ModelGateway`, secrets behind `SecretProvider`.
- **V. Traceability (event sourcing)**: ✅ Ledger from Phase 0 (Story 1.8).
- **VI. Security / secrets by reference**: ✅ JIT SecretProvider + redact (Story 1.9).
- **VII. Neutrality / YAGNI**: ✅ Goose/LiteLLM reused; advanced secret back-ends deferred.
- **VIII. Loop discipline (NON-NEGOTIABLE)**: ⚠️ **Partial in Phase 0** — see Complexity Tracking. Phase 0 has a wall-clock/kill bound but no per-step `LoopController`; full loop control lands with FR-16 (Epic 6). Depends on ADR 0001/0002 D1 (loop ownership).
- **Cross-platform**: ✅ tri-OS CI matrix.
→ **One justified deferral (Principle VIII), recorded below.**

## Project Structure

### Documentation (this feature)
```text
specs/001-phase0-walking-skeleton/
├── spec.md      # present
├── plan.md      # this file
└── tasks.md     # executable breakdown
```
The **exhaustive bite-sized** TDD plan (complete code per step) lives in `docs/superpowers/plans/2026-07-15-heddle-phase0-walking-skeleton.md` and is authoritative for execution; `tasks.md` is its Spec-Kit index.

> **Deviation note (recorded)**: the Spec-Kit `research.md` / `data-model.md` / `quickstart.md` / `contracts/` artifacts are **intentionally consolidated** into `docs/superpowers/*` (design + exhaustive plan) and, for the Goose spike, into ADR 0001. `contracts/` is waived (this is a CLI/library; the CLI surface in FR-001/Task T007 is the contract). This is a conscious deviation from the template, not an omission.

### Source Code (repository root)
```text
crates/
  heddle-core/
    src/{lib,content,event,error,silo,gateway,runtime,session,ledger,secrets}.rs
  heddle-cli/
    src/main.rs
    tests/cli.rs
config/litellm.config.yaml
.github/workflows/ci.yml
```
**Structure Decision**: a two-crate Cargo workspace (core + CLI). Additional sidecar/UI/connectors arrive in later phases.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Principle VIII (loop discipline) only partially satisfied in Phase 0 (wall-clock bound, no per-step `LoopController`) | Phase 0's job is the vertical skeleton; per-step loop control depends on the loop-ownership decision (ADR 0002 D1: Heddle owns the loop via goosed/embedded + MCP proxy), which the T000 spike resolves. Building `LoopController` before that decision would be rework. | Implementing full `LoopController` now would either force the loop-ownership decision prematurely or be built against a stub that cannot provide per-step hooks — net rework. Deferred to Epic 6 (FR-16), tracked in DESIGN-COMPLETENESS-POLICY bucket C. |
