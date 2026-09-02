# Feature Specification: native turn loop + ModelClient port (v0 strict-local)

**Feature Branch:** `004-native-loop` · **Created:** 2026-09-03 · **Status:** Implemented (v0 slice)
**Input:** ADR-0003 (Accepted, decision A = native Skein-owned loop) · Spike 1 evidence
`docs/superpowers/spikes/runtime-loop-evidence.md` · design §4.2/§4.11/§4.14 ·
builds directly on `specs/003-skein-core-foundation` (Implemented, 6/6).

## User Scenarios & Testing

### User Story 1 — The engine stops the loop, the model never does (P1)
As a user, a real multi-turn run terminates on an external budget, and the model's
"I'm done" claim is adjudicated by the engine rather than obeyed.
**Acceptance:**
1. **Given** a 3-iteration budget and a model that never claims completion, **When** the loop
   runs, **Then** exactly 3 model calls occur and the run ends `Exit::MaxIters`.
2. **Given** a token budget of 50 and a first turn costing 60, **When** the loop runs,
   **Then** exactly 1 model call occurs and the run ends `Exit::MaxTokens`.
3. **Given** a no-progress limit of 2 and a ground-truth probe reporting no progress,
   **When** the loop runs, **Then** the run ends `Exit::NoProgress`; a single true
   progress signal resets the counter and the run continues.
4. **Given** a model that claims a final output on turn 2, **When** the loop runs,
   **Then** the run ends `Exit::FinalOutput` carrying that turn's message.
5. **Given** an already-exhausted budget, **When** the loop is entered, **Then** zero
   model calls occur.

### User Story 2 — Every turn lands in the tamper-evident Ledger (P1)
As a user, each turn's exact request and response are appended to the hash-chained
Ledger, and the chain verifies.
**Acceptance:**
1. **Given** a two-turn run, **When** it completes, **Then** `verify_chain` succeeds and
   the step kinds are `[IterationBoundary, LlmRequest, LlmResponse, BudgetSpent] × 2 ++ [Exit]`
   with gap-free monotonic `seq`.
2. **Given** a recorded `LlmRequest` step, **When** its payload is read back, **Then** it
   deserializes into the exact `TurnRequest` that was sent.
3. **Given** two runs on one Ledger, **When** both complete, **Then** each run's log is isolated.
4. **Given** a provider failure mid-run, **When** the loop returns `Err`, **Then** the
   chain still verifies and its last step is the failed turn's `LlmRequest`.

### User Story 3 — Providers are discovered through a trait (P2)
As a developer, a model provider is a `ModelClient` implementation; the core never
depends on a concrete provider, and a test double is a first-class citizen.

## Requirements
- **FR-001**: `skein-core` MUST expose a `ModelClient` trait with
  `fn turn(&mut self, &TurnRequest) -> Result<TurnResponse>`, plus serde-round-trippable
  `TurnRequest`/`TurnResponse` (Constitution IV).
- **FR-002**: `skein-core` MUST expose a `ProgressProbe` trait supplying the ground-truth
  progress signal. It MUST NOT receive model output (Constitution VIII(b)).
- **FR-003**: `NativeLoop::run` MUST drive turns until `LoopController::should_exit`
  returns `Some`, and MUST NOT call the model when the budget is already exhausted.
- **FR-004**: Each turn MUST append `IterationBoundary`, `LlmRequest` (before the call),
  `LlmResponse`, `BudgetSpent` to the existing `Ledger`; every terminated run MUST append
  exactly one `Exit` step. The loop MUST use `Ledger::append` and never reimplement it
  (Constitution V).
- **FR-005**: `tokens_used` MUST come from the provider's reply and `made_progress` from
  the probe; neither may be inferred from the model's text.
- **FR-006**: A provider error MUST propagate as `Err` and MUST leave `verify_chain` passing.
- **FR-007**: No network, no new Cargo dependency, no new crate (Constitution II, VII).

## Success Criteria
- **SC-001**: `cargo test -p skein-core` green — spec 003's 6 tests unmodified plus this
  slice's 9; `clippy -D warnings` clean; `fmt --check` clean. (15/15, 2026-09-03.)
- **SC-002**: Four distinct tests reach `Exit::MaxIters`, `Exit::MaxTokens`,
  `Exit::NoProgress` and `Exit::FinalOutput` **through `NativeLoop::run`**, each asserting
  the model call count — not through isolated `LoopController` unit tests.
- **SC-003**: `git diff` on `Cargo.toml` and `crates/skein-core/Cargo.toml` is empty.
- **SC-004**: tri-OS CI (`.github/workflows/core.yml`) green on Windows, macOS, Linux
  (ADR-0004 D1(d)). The workflow is in place and its three commands pass locally on
  Windows; the macOS and Linux legs are unobserved until the repository has a remote.

## Assumptions
- **`ModelClient` is synchronous in v0.** The LiteLLM-backed client (BMAD Story 1.4) owns a
  `tokio` runtime internally and blocks behind this boundary. Streaming and mid-turn
  cancellation (Spike 1 criterion C3 — PARTIAL) arrive with it.
- **Ledger payloads are the serialized `TurnRequest`/`TurnResponse`**, which in v0 *is* the
  exact I/O. Raw-provider-body capture (Spike 1 C1, byte-exact) becomes an additive
  `raw: Option<String>` field when the HTTP client lands.
- **`Exit::HumanReject` is unreachable in this slice** — no approval/HITL gate exists yet
  (design §4.14 HITL escalation; `StepKind::Approval` is currently unused). Not a defect.
- **Design §4.14 sketches an `Exit::Error` variant that `loop_ctl.rs` does not implement.**
  Divergence recorded; extending `Exit` is deferred rather than amending an Implemented spec.
- The Tool Gateway (rmcp, Spike 4) and the ACP facade remain the following slices.
