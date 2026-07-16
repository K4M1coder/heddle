# Feature Specification: skein-core foundation (v0 strict-local)

**Feature Branch:** `003-skein-core-foundation` · **Created:** 2026-07-16 · **Status:** Implemented (v0 slice)
**Input:** ADR-0003 (Accepted), ADR-0004 D3 (v0 = strict-local core), design §4.2/§4.11/§4.14. Promotes the proven spike patterns (Spike 1 loop, ledger shape) into product code under `crates/`.

## User Scenarios & Testing

### User Story 1 — Inspectable, tamper-evident execution record (P1)
As a user, every step is captured in an append-only, hash-chained Ledger I can inspect, and any tampering is detectable.
**Acceptance:**
1. **Given** a run, **When** requests/responses/tool events are appended, **Then** each `Step` chains onto the previous by content hash and `show(id)` returns the exact payload.
2. **Given** an intact chain, **When** an earlier payload is altered, **Then** `verify_chain` returns an integrity error.
3. **Given** two runs, **When** appended to the same Ledger, **Then** each run's `log` is isolated.

### User Story 2 — Engine-enforced loop control (P1)
As a user, the loop stops on an external budget, never on the model's say-so, and reflect/retry keys off external ground truth.
**Acceptance:**
1. **Given** a max-iteration budget, **When** the model would continue, **Then** the controller returns `Exit::MaxIters`.
2. **Given** a no-progress limit, **When** N iterations report no external progress, **Then** `Exit::NoProgress`; a progress signal resets the counter.
3. **Given** a token budget, **When** exceeded, **Then** `Exit::MaxTokens`; a genuine final output returns `Exit::FinalOutput`.

### User Story 3 — Typed content pipeline (P2)
As a developer, messages carry typed `Content` parts that round-trip losslessly (v0: text; multimodal added later without pipeline change).

## Requirements
- **FR-001**: `skein-core` MUST expose `Content`/`Message`/`Role` (serde round-trip).
- **FR-002**: `Ledger` MUST be append-only, hash-chained, per-run isolated, with `append/log/show/verify_chain`.
- **FR-003**: `LedgerIntegrity` MUST be raised when the chain is inconsistent.
- **FR-004**: `LoopController` MUST enforce iteration/token/no-progress budgets externally; `record_iteration(tokens, made_progress)` takes ground truth from outside the model.
- **FR-005**: No network, no product runtime beyond these library types (strict-local v0).

## Success Criteria
- **SC-001**: `cargo test -p skein-core` green; `clippy -D warnings` clean; `fmt --check` clean. (6/6 tests, 2026-07-16.)
- **SC-002**: tamper test proves integrity detection (not just "no known leak").

## Assumptions
- In-memory Ledger for v0; durable silo-backed store is a later slice.
- The native loop wiring (ModelClient + MCP via rmcp + ACP facade) is the next slice, building on these contracts.
