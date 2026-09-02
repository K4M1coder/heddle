# Feature Specification: deny-by-default for tool identity (v0 strict-local)

**Feature Branch:** `007-tool-allowlist` · **Created:** 2026-09-03 · **Status:** Implemented (v0 slice)
**Input:** `specs/006-loop-tool-wiring/spec.md` Assumptions R3 — *"`ToolPolicy` has no
allowlist, and the tool name is now model-chosen"* — carried forward on that slice's
"Next slice" list · Constitution VI (**Deny-by-default**) · design §4.3/§4.11.

`ToolPolicy::decide` returns `Allow { reason: "not mutating" }` for every name absent from the
configured `mutating` list. That is allow-by-default for tool *identity*: the exact inverse of
Constitution VI's opening clause. It was harmless while spec 005's callers were trusted product
code with hard-coded names; spec 006 wired `TurnResponse.tool_calls` into `NativeLoop::mediate`,
so the string reaching `decide` is now chosen by the model. Every tool the operator's configured
transport happens to expose is reachable by any turn that names it.

## User Scenarios & Testing

### User Story 1 — A tool nobody allowlisted does not run (P1)
As an operator, a tool I never classified at all is refused, not admitted by omission.
**Acceptance:**
1. **Given** a name absent from the allowlist and absent from any mutating classification,
   **When** it is called through the gateway, **Then** the call returns
   `SkeinError::ToolDenied`, the transport is never invoked, the run's kinds are exactly
   `[ToolCall, Approval]`, the `Approval` payload records `"denied"`, and `verify_chain` passes.
2. **Given** the same against a **live** embedded MCP server that really exposes the tool,
   **When** it is called, **Then** the server's own invocation counter stays at 0.
3. **Given** a name that is on the *approved* list but absent from the allowlist, **When** it is
   called, **Then** it is still denied and the transport is still never invoked.

### User Story 2 — Known read-only tools keep working (P1)
As a user, deny-by-default costs nothing at the tools the operator did classify.
**Acceptance:**
1. **Given** an allowlisted `ToolAccess::ReadOnly` tool and an empty approved list, **When** it
   is called, **Then** it executes exactly once, the kinds are
   `[ToolCall, Approval, ToolResult]`, and the `Approval` payload records `"allowed"`.

### User Story 3 — Mutation policy is unchanged on top of identity (P1)
As an operator, classifying a tool as allowlisted does not waive its approval requirement:
identity and mutation are two gates, not one.
**Acceptance:**
1. **Given** an allowlisted `ToolAccess::Mutating` tool, **When** it is called without approval,
   **Then** it is denied; **When** it is called with approval, **Then** it runs exactly once.
   Proven by the *pre-existing* spec-005 tests
   `denied_mutating_tool_never_reaches_the_transport` and `approved_mutating_tool_executes_once`,
   whose bodies do not change: they become this slice's controls.

### User Story 4 — A model that names a forbidden tool is told, and the run survives (P1)
As a user, a model naming a tool outside the allowlist is an ordinary governed refusal, not a
crashed run.
**Acceptance:**
1. **Given** a turn whose `tool_calls` name a tool absent from the allowlist, **When** the loop
   runs, **Then** the run ends `Exit::FinalOutput`, the transport is never invoked, the kinds
   contain `ToolCall` and `Approval` but no `ToolResult`, the `Approval` payload records
   `"denied"`, and the next `LlmRequest`'s history carries a
   `[tool_result tool=… status=denied]` message naming the tool.

## Requirements
- **FR-001**: `ToolPolicy` MUST deny any tool name not explicitly allowlisted, before any
  mutation consideration (Constitution VI).
- **FR-002**: The denial MUST reuse `SkeinError::ToolDenied` and the existing
  `[ToolCall, Approval]` Ledger shape — no second denial mechanism and no new `StepKind`
  (Constitution V).
- **FR-003**: The mutating/approved distinction MUST still govern mutation *within* the
  allowlist.
- **FR-004**: `ToolPolicy::new` MUST make the allowlist a required constructor argument; there
  MUST be no constructor that yields a permissive policy.
- **FR-005**: No new Cargo dependency, no new crate, no new module.
- **FR-006**: `native_loop.rs`, `loop_ctl.rs`, `ledger.rs` and `error.rs` MUST be unchanged.

## Success Criteria
- **SC-001**: `fmt --check`, `clippy --workspace --all-targets -D warnings` and
  `cargo test --workspace` all clean; the suite is 35 pre-existing + 5 new = **40** tests.
- **SC-002**: The allowlist denial is proven against a live embedded rmcp server for a tool the
  server really implements — the server's own counter is the ground truth.
- **SC-003**: `git diff dev` on every `Cargo.toml` in the repository is empty.
- **SC-004**: `git diff dev` on `crates/skein-core/src/native_loop.rs`, `src/loop_ctl.rs`,
  `src/ledger.rs` and `src/error.rs` is empty, and the diff in the three test files is confined
  to helper functions plus added tests — no pre-existing test *body* changes, so they remain
  controls.
- **SC-005**: `git diff dev -- spikes/` is empty (ADR-0004 D2).
- As in specs 004–006, the macOS and Linux legs of `core.yml` are unobserved until the
  repository has a remote; only the Windows leg is run locally.

## Assumptions
- **Spec 005's User Story 1 acceptance 3 is superseded by this slice.** That acceptance reads
  *"a tool absent from the mutating list … is treated as read-only and runs"*. Per the precedent
  spec 005 set in its own Assumptions — *historical records keep their text; they record what
  was true when written* — `specs/005-tool-gateway/spec.md` is **not** edited. The supersession
  is recorded here, and spec 005's two mutating-approval tests survive unchanged as controls.
- **Names are matched exactly**: case-sensitive, no trimming, no Unicode normalization. MCP tool
  names are opaque identifiers, and normalizing them is a policy decision with no caller.
- **The allowlist is configuration, not discovery.** `tools/list` and server-provided capability
  descriptors stay deferred (Constitution VII), so `ToolAccess` is declared by the operator
  rather than derived from a server's tool annotations.
- **Per-argument and per-path policy is out of scope.** This is a *name* allowlist: `fs_write`
  is allowed or it is not; *where* it writes is a later slice's question.
- **A model that repeatedly requests a denied tool burns iterations** until the
  `LoopController` exits. Bounded by Constitution VIII(a), unchanged from the mutating-denial
  path shipped in slice 006.
- **The tool *name* is still not redacted into the Ledger** (spec 006 Assumptions): an unlisted
  name is recorded verbatim in the `ToolCall` and `Approval` payloads. Recorded, not widened —
  and the names of everything that actually *executes* are now drawn from a bounded set.
- **HITL approval and `Exit::HumanReject` stay unreachable**; approval remains a configured list
  of tool names. Constitution VIII(d) stays open.
