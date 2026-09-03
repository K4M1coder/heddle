# Feature Specification: an ACP facade over the native loop (v0 slice)

**Feature Branch:** `008-acp-facade` · **Created:** 2026-09-03 · **Status:** Draft
**Input:** `specs/007-tool-allowlist/tasks.md` "Next slice" item 1 — *"ACP client facade over the
native loop + gateway"* · Constitution I (**headless core, CLI as the reference client**) ·
ADR-0003 decision 2 and ADR-0004 D3 (**adopt ACP as Skein's client↔core boundary**) ·
design §4.2/§4.3/§4.11/§4.14.

Four merged slices give `skein-core` a working, governed, tested agentic loop — `NativeLoop`
driving a `ModelClient`, mediating every model-requested tool through a deny-by-default
`ToolGateway`, recording everything in a hash-chained `Ledger`. **Nothing outside a Rust test
binary can drive it.** The only "client" today is `crates/skein-core/tests/native_loop.rs`.

Constitution Principle I cannot begin to be satisfied until the core is reachable by a real
client, and ADR-0003 already decided what that boundary is: ACP, not a bespoke Skein wire
protocol. That decision has been recorded-but-unbuilt since the Spike 1 evidence of 2026-07-16 —
the same decayed-evidence condition that motivated slices 004 and 005.

This slice builds the smallest facade that makes **one real ACP session** drive the existing
governed loop end to end, in a new crate `skein-acp` that is the only crate in the product to
name ACP. `skein-core` and `skein-mcp` are unchanged.

## User Scenarios & Testing

### User Story 1 — A real ACP client drives one governed turn (P1)
As a client author, I connect over ACP, open a session, send a prompt, and the existing governed
loop runs — tools mediated, chain recorded — and I get a stop reason back.
**Acceptance:**
1. **Given** a real `agent-client-protocol` client connected to `skein-acp` over a byte-stream
   transport, **When** it performs `initialize` then `session/new` then `session/prompt`, and the
   scripted model asks for an allowlisted read-only tool on turn 1 and returns `final_output` on
   turn 2, **Then** the tool transport is invoked exactly once, `PromptResponse.stop_reason` is
   `StopReason::EndTurn`, the client receives an `AgentMessageChunk` carrying the final message
   text, and `Ledger::verify_chain` passes for run id `{session_id}#1`.

### User Story 2 — Permission is a second gate, never a substitute for the first (P1)
As an operator, the ACP client's answer can only further restrict what `ToolPolicy` already
allowed. A tool the policy refuses is never even shown to the client.
**Acceptance:**
1. **Given** a model that names a tool absent from the allowlist, **When** the loop runs,
   **Then** the client receives **zero** `session/request_permission` requests, the transport is
   never invoked, the run still ends `EndTurn`, and the Ledger holds `ToolCall` + `Approval`
   (`denied`) with no `ToolResult`.
2. **Given** an allowlisted `ToolAccess::Mutating` tool with an empty approved list, **When** the
   loop runs, **Then** the permission-request count and the transport count are both **0**.
3. **Given** an allowlisted read-only tool and a client that selects the reject option, **When**
   the loop runs, **Then** the transport is never invoked, the run still ends `EndTurn`, and the
   next `LlmRequest`'s history carries `[tool_result tool=… status=denied]`.

### User Story 3 — What the client sees is a view of the chain, not a second record (P1)
As an auditor, every ACP update a client received can be pointed at the Ledger step it was
derived from.
**Acceptance:**
1. **Given** a completed run, **When** the client's received `SessionUpdate`s are collected,
   **Then** every `AgentMessageChunk` text equals the `message.text()` of a
   `StepKind::LlmResponse` payload of that run, and the counts match.
2. **Given** two sequential `session/prompt` calls in one session, **When** both complete,
   **Then** the run ids are `{session_id}#1` and `{session_id}#2`, both verify, and each holds
   exactly one `Exit` step.

### User Story 4 — A client can cancel (P1)
As a user, `session/cancel` stops the run and the agent says so.
**Acceptance:**
1. **Given** a multi-iteration script and a `session/cancel` notification delivered mid-run,
   **When** the run ends, **Then** `stop_reason` is `StopReason::Cancelled`, the model was called
   fewer times than the script's length, and `verify_chain` still passes.

## Requirements
- **FR-001**: A new workspace crate `skein-acp` MUST be the only crate that names ACP; the
  dependency direction is `skein-acp` to `skein-core`, never the reverse.
- **FR-002**: **No file under `crates/skein-core/` or `crates/skein-mcp/` may change.** ACP is
  adapted to the core through decorators over the existing `ToolTransport` and `ModelClient`
  ports (Constitution IV).
- **FR-003**: `ToolPolicy::decide` MUST run before any ACP permission request; a tool it denies
  MUST never produce one (Constitution VI).
- **FR-004**: A permission request MUST carry the tool **name** only — never the raw arguments.
- **FR-005**: A client's decline MUST surface as `SkeinError::ToolDenied`, so the existing
  `mediate` path feeds `[tool_result … status=denied]` back and the run survives.
- **FR-006**: ACP session updates MUST be *derived from* `Ledger::log(run_id)`; there MUST be no
  second event record (Constitution V).
- **FR-007**: One ACP `SessionId` owns N Skein runs, one per `session/prompt`, with run ids
  `{session_id}#{n}` starting at 1 — so `NativeLoop`'s "exactly one `Exit` per chain" and
  `verify_chain`'s per-run semantics stay literally true.
- **FR-008**: `session/cancel` MUST end the run at a turn boundary and report
  `StopReason::Cancelled`.
- **FR-009**: The `session/prompt` handler MUST NOT block the connection's dispatch loop.
- **FR-010**: The library MUST NOT depend on a specific async runtime; `tokio` is a
  dev-dependency only.

## Success Criteria
- **SC-001**: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`
  and `cargo test --workspace` all clean; the suite is 40 pre-existing + N new tests.
- **SC-002**: The end-to-end acceptance runs against a **real** ACP client and a **real** ACP
  agent from `agent-client-protocol`, over a real byte-stream transport. No hand-rolled stand-in
  for the protocol.
- **SC-003**: `git diff dev -- crates/skein-core/ crates/skein-mcp/` is empty.
- **SC-004**: `git diff dev -- spikes/ .github/ rust-toolchain.toml` is empty.
- **SC-005**: `git diff dev -- Cargo.toml` shows only added `[workspace.dependencies]` entries.
- As in specs 004–007, the macOS and Linux legs of `core.yml` are unobserved until the repository
  has a remote; only the Windows leg is run locally.

## Assumptions
- **Updates are emitted after the turn completes, not streamed during it.** This is forced, not
  chosen: streaming from inside the run needs either an append-observer seam on `Ledger` (which
  this slice may not touch) or a parallel event channel out of the loop — which would be exactly
  the second record Principle V prohibits. The permission request, which *must* be interactive,
  does happen during the run.
- **Cancellation lands at turn boundaries, not mid-turn.** A model call already in flight
  completes. Mid-turn cancellation needs cancel points inside `NativeLoop`, which this slice may
  not touch; Spike 1 already recorded C3 as PARTIAL for the same reason.
- **Each `session/prompt` is an independent Skein run with no conversation history.**
  `NativeLoop::run` starts from `messages = vec![prompt]`; carrying history would change its
  signature.
- **Only two permission options are offered** — allow-once and reject-once. `allow_always` /
  `reject_always` imply persistent policy mutation, which this slice has no store for; that is
  also where `Exit::HumanReject` finally becomes reachable (Constitution VIII(d)).
- **Non-text prompt content blocks are rejected** with a JSON-RPC error: `Content` has one
  variant, `Text` (design §4.2 defers image/audio/doc/video to v2).
- **`Exit::NoProgress` and `Exit::HumanReject` both map to `StopReason::Refusal`.** An
  engine-forced stall stop is not a success; `EndTurn` would falsely claim one.
- **The ACP tool-call id is the Ledger step id of the `ToolCall` step.** The correlation an ACP
  client uses to join a tool call to its updates is therefore the chain's own identity, not a
  parallel counter.
