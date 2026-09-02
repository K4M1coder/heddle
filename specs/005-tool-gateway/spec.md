# Feature Specification: governed Tool Gateway + rmcp transport (v0 strict-local)

**Feature Branch:** `005-tool-gateway` · **Created:** 2026-09-03 · **Status:** Draft
**Input:** ADR-0003 (Accepted, decision 4 = tool governance via `rmcp`) · Spike 4 evidence
`docs/superpowers/spikes/mcp-gateway-evidence.md` · design §4.3/§4.11/§4.14/§7.13 ·
builds directly on `specs/004-native-loop` (Implemented, 15/15).

## User Scenarios & Testing

### User Story 1 — A mutating tool does not run without approval (P1)
As a user, a tool the policy classifies as mutating never reaches the downstream server
unless it has been explicitly approved, and the refusal is on the record.
**Acceptance:**
1. **Given** `fs_write` classified mutating and not approved, **When** the gateway is asked
   to call it, **Then** the call returns `SkeinError::ToolDenied`, the transport is never
   invoked, and the run's step kinds are exactly `[ToolCall, Approval]`.
2. **Given** the same tool now approved, **When** the gateway calls it, **Then** the
   transport is invoked exactly once and the step kinds are `[ToolCall, Approval, ToolResult]`.
3. **Given** a tool absent from the mutating list, **When** the gateway calls it, **Then** it
   is treated as read-only and runs (exercised by the User Story 2 scenario, whose
   `read_secret` is unlisted and executes).
4. **Given** a denial against a **live** MCP server, **When** the gateway is asked to call
   the mutating tool, **Then** the server's invocation counter stays at 0.

### User Story 2 — Secrets never enter the Ledger (P1)
As a user, a secret carried in a tool's arguments or returned in its output reaches the
trusted caller but never the tamper-evident record.
**Acceptance:**
1. **Given** a secret in both the call arguments and the tool's output, **When** the call
   completes, **Then** the returned `ToolOutcome` still contains the secret, **no** payload
   of **any** step in the run contains it, and at least one payload contains `***`.
2. **Given** the same run against a live MCP server, **When** the call completes, **Then**
   the live result contains the secret and no ledger payload does.

### User Story 3 — The record is replayable without re-invoking anything (P1)
As a user, the captured results of a run can be reconstructed from the Ledger alone.
**Acceptance:**
1. **Given** two completed allowed calls, **When** the run is replayed, **Then** both
   redacted results come back in order and the transport's call count is unchanged.
2. **Given** the same run against a live MCP server, **When** it is replayed, **Then** the
   server's invocation counter is unchanged and the replayed content matches the capture.

### User Story 4 — Governance failures leave the chain intact (P1)
As a user, neither a policy denial nor a downstream tool failure can corrupt the Ledger.
**Acceptance:**
1. **Given** a transport that errors, **When** the gateway calls it, **Then** the call
   returns `SkeinError::Tool`, no `ToolResult` step is fabricated, and `verify_chain` passes.
2. **Given** a run containing a denial *and* an executed call, **When** it is verified,
   **Then** `verify_chain` passes.
3. **Given** gateway calls interleaved with direct `Ledger::append` on the same `run_id`,
   **When** the run is verified, **Then** `verify_chain` passes and `seq` is gap-free `0..n`.

### User Story 5 — Transports are discovered through a trait (P2)
As a developer, an MCP server is reached through a `ToolTransport` implementation;
`skein-core` never names `rmcp`, and a hand-rolled test double is a first-class citizen.

## Requirements
- **FR-001**: `skein-core` MUST expose a `ToolTransport` trait with
  `fn call(&mut self, &ToolCall) -> Result<ToolOutcome>`. The core MUST NOT name any
  concrete transport (Constitution IV).
- **FR-002**: `ToolGateway::call` MUST deny a mutating tool that is not approved, MUST NOT
  touch the transport on denial, and MUST return `SkeinError::ToolDenied` (Constitution VI).
- **FR-003**: Every payload appended to the Ledger MUST be redacted first — arguments as
  well as results. The raw values MUST still reach the transport and the trusted caller
  (Constitution VI: secrets never by value in the record).
- **FR-004**: Capture MUST go through the existing `Ledger::append` using the existing
  `StepKind::{ToolCall, Approval, ToolResult}`. No parallel event log, no new `StepKind`
  (Constitution V).
- **FR-005**: `replay_tool_calls(&Ledger, run_id)` MUST reconstruct captured results from
  the Ledger alone. It MUST NOT hold a transport, so re-invocation is unrepresentable.
- **FR-006**: A policy denial and a transport error MUST both leave `verify_chain` passing.
- **FR-007**: `crates/skein-core/Cargo.toml` MUST gain no dependency. `rmcp` and `tokio`
  MUST be confined to the new `skein-mcp` crate; the dependency direction is
  `skein-mcp → skein-core` and never the reverse (Constitution II, VII).
- **FR-008**: The repository toolchain bump MUST be applied to `rust-toolchain.toml`,
  `Cargo.toml`, `.github/workflows/core.yml` and `docs/DEVELOPMENT.md` in lockstep, on its
  own commit, before any new product code.

## Success Criteria
- **SC-001**: `fmt --check`, `clippy --workspace --all-targets -D warnings` and
  `cargo test --workspace` all clean; the suite is 15 pre-existing + 11 new = **26** tests.
- **SC-002**: The four governance properties (denial, single execution, redaction, replay)
  are each proven against a **live embedded rmcp server**, not only against the in-core double.
- **SC-003**: `git diff` on `crates/skein-core/Cargo.toml` is empty. The only `Cargo.toml`
  changes are the root `[workspace.dependencies]` additions and the new
  `crates/skein-mcp/Cargo.toml`.
- **SC-004**: `git diff` under `spikes/` is empty (ADR-0004 D2), and no `crates/` file names
  `mcp_gateway` or a `spikes/` path.
- **SC-005**: tri-OS CI (`.github/workflows/core.yml`) green on the bumped toolchain. As in
  spec 004's SC-004, the macOS and Linux legs are unobserved until the repository has a remote.

## Assumptions
- **The toolchain moves from 1.79 to 1.97.** `rmcp` 2.2.0 declares `edition = "2024"`, which
  no rustc below 1.85 can parse, so the gateway cannot exist at the old pin. 1.97 is pinned
  rather than the 1.85 floor because 1.97 is the stable actually exercised against this
  workspace. `review-feasibility-context.md` §3 asked for exactly this: a current pinned
  stable, or an older MSRV proven against every direct dependency. Historical records
  (`specs/001`, `specs/003`, `specs/004`, `_bmad-output/**`) keep their 1.79 text — they
  record what was true when written.
- **`ToolTransport` is synchronous**, matching `ModelClient`. The rmcp adapter owns a
  `tokio` runtime and blocks behind the boundary, so `RmcpToolTransport::call` must not be
  invoked from inside an async context.
- **Mutability classification is configuration, not discovery.** Deriving it from MCP tool
  annotations (`tools/list`) is deferred, as is interactive HITL approval: approval is a
  configured list of tool names, exactly as Spike 4 proved. `Exit::HumanReject` stays
  unreachable.
- **Redaction takes literal secret strings from configuration.** Those values will come from
  `SecretProvider::resolve` (design §7.13) once it lands; a `SecretProvider` in this slice
  would be a capability with no caller.
- **Design §4.11's `Step` sketch carries `ts`, `principal` and `silo` fields the v0 `Step`
  does not have.** That gap predates this slice (spec 003) and is not widened here.
- **Prompt-injection handling of tool output** (Constitution VI, "external content is data,
  never instruction") is not addressed: it belongs with the loop wiring that will actually
  feed tool output back to a model.
- The ACP client facade and wiring the gateway into `NativeLoop` remain the following slices.
