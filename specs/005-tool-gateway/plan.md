# Implementation Plan: governed Tool Gateway + rmcp transport (v0 strict-local)

**Branch**: `005-tool-gateway` | **Date**: 2026-09-03 | **Spec**: `specs/005-tool-gateway/spec.md`

## Summary
Give the product its first enforcement point for tool calls. `heddle-core` gains one module,
`tool.rs`, holding the whole governed path — the `ToolTransport` port, the policy, the
redactor, the Ledger capture and the transport-free replay — and no new dependency. A new
workspace member, `heddle-mcp`, holds `RmcpToolTransport`, the only place that names `rmcp` or
`tokio`. This promotes the *design* validated by Spike 4 (`spikes/mcp-gateway/`) — not its
code, which stays quarantined under `spikes/` per ADR-0004 D2.

## Technical Context
**Language/Version**: Rust 1.97 (pinned in `rust-toolchain.toml`; raised from 1.79 this slice
because `rmcp` 2.2.0 is `edition = "2024"`, unparseable below 1.85)
**Primary Dependencies**: `rmcp` 2.2 + `tokio` 1 in `heddle-mcp` only; `schemars`/`serde` as
`heddle-mcp` dev-dependencies for the embedded fixture server. `heddle-core` unchanged.
**Storage**: the existing in-memory `Ledger` (durable SQLite-backed silo still deferred)
**Testing**: `cargo test`; a hand-rolled `CountingTransport` double in `heddle-core`, and a
live in-process rmcp server over `tokio::io::duplex` in `heddle-mcp`. No mocking crate.
**Target Platform**: Windows + macOS + Linux
**Project Type**: library (two workspace members)
**Performance Goals**: N/A (functional correctness first)
**Constraints**: offline (egress OFF), deny-by-default for mutating tools, append-only Ledger,
no secret by value in any captured payload
**Scale/Scope**: one governed gateway, one transport port, one rmcp adapter

## Constitution Check
*GATE: must pass before implementation.*
- **I. Headless core**: ✅ library API only; no UI, no `[[bin]]`.
- **II. Local-first / silo isolation**: ✅ no network — the rmcp tests speak to an in-process
  duplex, never a socket or a spawned process. `heddle-core` gains no dependency.
- **III. Test-First**: ✅ `tests/tool_gateway.rs` (T3) and `tests/rmcp_gateway.rs` (T6) were
  each written and observed red before the module they exercise existed.
- **IV. Inverted coupling**: ✅ `ToolTransport` is the seam; `heddle-core` physically cannot
  name `rmcp` because its `Cargo.toml` does not depend on it.
- **V. Traceability**: ✅ capture goes through the existing `Ledger::append` with the existing
  `StepKind`s; `verify_chain` is asserted on the denial path, the error path and the
  interleaved-chain path.
- **VI. Security / secrets by reference**: ✅ deny-by-default for mutating tools with the
  transport untouched on denial; redaction applied to arguments *and* results before every
  append, asserted by scanning every payload of the run rather than only the result step.
- **VII. Neutrality / YAGNI**: ✅ `rmcp` is reused rather than reimplementing the MCP wire
  protocol; no new `StepKind`, no `SecretProvider`, no tool discovery, no async port.
- **VIII. Loop discipline (NON-NEGOTIABLE)**: n/a this slice — the gateway is not wired into
  `NativeLoop`. **(d)** HITL escalation remains open: approval is a configured list.
- **Cross-platform**: ✅ pure Rust, no `#[cfg]`; `core.yml` already filters on `crates/**`, so
  the new crate is covered by the existing tri-OS matrix.

## Project Structure

### Documentation (this feature)
```text
specs/005-tool-gateway/
├── spec.md      # this feature's requirements
├── plan.md      # this file
└── tasks.md     # executable breakdown
```

### Source Code (repository root)
```text
crates/heddle-core/
  src/tool.rs             # new — ToolCall/ToolOutcome/ToolTransport/ToolPolicy/Decision/
                          #       Redactor/CapturedResult/ToolGateway/replay_tool_calls
  src/lib.rs              # +1 module, +1 re-export line
  src/error.rs            # +2 variants: HeddleError::ToolDenied, HeddleError::Tool
  tests/tool_gateway.rs   # new — 6 acceptance tests + CountingTransport
crates/heddle-mcp/         # new workspace member (crates/* already globs it in)
  Cargo.toml
  src/lib.rs              # new — RmcpToolTransport
  tests/rmcp_gateway.rs   # new — 5 acceptance tests against a live embedded rmcp server
Cargo.toml                # rust-version 1.97; +rmcp/tokio/schemars in [workspace.dependencies]
rust-toolchain.toml       # channel 1.97
.github/workflows/core.yml# toolchain 1.97; rust-toolchain.toml added to both paths filters
docs/DEVELOPMENT.md       # Rust 1.79 → 1.97
docs/superpowers/spikes/mcp-gateway-evidence.md  # new — the missing Spike 4 evidence note
```
**Structure Decision**: the governed path lives in `heddle-core` and the protocol adapter in a
second crate. A `#[cfg(feature = "mcp")]` gate inside `heddle-core` was rejected: `core.yml`
runs `cargo test --workspace` with no `--all-features`, so the rmcp path would be silently
untested on all three OSes, and a feature flag would put an async stack one flag away from the
dependency-minimal core. A second crate is always built, always tested, needs no conditional
compilation, and makes the "core names no transport" claim structural rather than conventional.
`ledger.rs`, `loop_ctl.rs`, `content.rs`, `model.rs`, `native_loop.rs` and their tests are not
touched, so specs 003 and 004 remain independent controls.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| **Repo-wide toolchain bump 1.79 → 1.97** (Principle II asks for a stable, deliberate baseline) | `rmcp` 2.2.0 declares `edition = "2024"`; no rustc below 1.85 can parse it, so the gateway cannot exist at the old pin. `_bmad-output/planning-artifacts/validation/review-feasibility-context.md` §3 asked for exactly this evidence: "a current pinned stable toolchain with a defined support/update policy; or a deliberately older MSRV proven against every direct dependency". This slice is that gate, resolved the first way. | Keeping `heddle-core` at a per-package `rust-version = "1.79"` and raising only `heddle-mcp`: Cargo permits it, but `rust-toolchain.toml` pins one toolchain for the whole repo, so the build toolchain still has to move and the per-package claim would be verified by no gate. Unverified MSRV claims are the thing the feasibility review complained about. One honest number, one gate. Pinning the 1.85 floor was rejected because 1.97 is the stable actually exercised against this workspace. |
| **A second crate, `heddle-mcp`** (Principle VII: no structure without a caller) | Principle IV requires the core to discover the MCP transport through a trait rather than depend on it. A separate crate makes `heddle-core`'s zero-dependency claim physical — its `Cargo.toml` cannot name `rmcp` — and confines the edition-2024/async blast radius to one crate that is always built and always tested. | `#[cfg(feature = "mcp")]` inside `heddle-core`: `core.yml` has no `--all-features` leg, so the governed rmcp path would be untested on every OS; and a feature gate yields two compile shapes of the core with an async stack one flag away from it. Naming the crate `heddle-tools` was rejected for `heddle-mcp`, which names the protocol it adapts (design §4.3, "Connectors = MCP servers"). |
