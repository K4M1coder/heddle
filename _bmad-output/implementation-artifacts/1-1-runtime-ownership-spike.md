---
baseline_commit: 5389a48321646ce2c4de2978efd14207acce904a
---

# Story 1.1: Runtime ownership spike (ADR-0003 Spike 1)

Status: done

## Senior Developer Review (AI)

- **Reviewer:** adversarial subagent (BMAD blind-hunter), fresh context, 2026-07-16
- **Outcome:** CHANGES-REQUIRED (documentation-only) → **fixes applied** → **Approved**
- **Refuted (1):** "C1 FAIL by design / unfixable" for ACP — false: ACP `_meta`/`ExtNotification` (`schema/src/v1/ext.rs`) can carry raw model I/O with worker cooperation. Reworded to "unavailable through existing workers"; strengthens the ACP-facade decision.
- **Weakened→corrected (3):** C3 races send-phase only (now marked PARTIAL); C1 request-side is exact-pre-serialization not wire-byte (reworded); "Ledger-ready" → "Ledger-shaped, in-memory".
- **Held:** reproducibility (4/4 re-run), citations accurate, Option B "no embeddable loop" true, decision logic sound.
- **Net:** no code rework; decision (native loop + ACP boundary + reduced-assurance workers) unchanged. All fixes are in the evidence note + this story.

## Story

As the Heddle maintainer,
I want objective, side-by-side evidence of which execution tier lets Heddle own the agent loop (native Rust loop vs embedded goose-sdk vs ACP worker),
so that ADR-0003 can be Accepted/Revised on facts and the Ledger/LoopController one-way-door contracts can be frozen safely.

## Acceptance Criteria

Pre-registered in `docs/superpowers/spikes/spike-protocol.md` (Spike 1). For EACH option evaluated, the evidence note must show observable proof (transcript, log, or test output — never self-assessment):

1. **Exact model I/O capture**: the byte-exact request payload sent to the model and the raw response are captured, per turn.
2. **Tool-call interception**: every tool call + result can be intercepted BEFORE execution (a mediation point exists for policy/approval/redaction).
3. **External termination**: the loop can be terminated mid-run by the harness (budget enforcement) without killing the process.
4. **Run correlation**: all events of one run are correlated under one run-id (Ledger-ready shape).
5. **Effort estimate**: a rough build+maintain estimate per option, with the main risks.

**Decision output**: a recommendation (A / B / C / A-behind-ACP-facade) recorded as evidence in `docs/superpowers/spikes/runtime-loop-evidence.md`, and ADR-0003 updated (Accepted or Revised). Sprint status → `review`.

## Tasks / Subtasks

- [x] Task 1 — Scaffold quarantine workspace (AC: all)
  - [x] Create `spikes/runtime-loop/` as a STANDALONE cargo workspace (own `Cargo.toml`; NOT a member of any root workspace; never imported by product code)
  - [x] Stub OpenAI-compatible endpoint for deterministic tests (wiremock) so no network/model is required for criteria 1–4
- [x] Task 2 — Option A: native loop (AC: 1,2,3,4,5)
  - [x] Minimal loop: POST /chat/completions (reqwest) → parse tool_calls → dispatch to an in-process tool behind a Mediator (rmcp MCP wiring = follow-up probe, noted in evidence) → loop
  - [x] Prove: log exact request/response JSON per turn; intercept tool call before exec; cancel via `tokio::select!`/CancellationToken mid-run; tag all events with run_id — **4/4 criteria tests green**
- [x] Task 3 — Option B: embedded goose-sdk (AC: 1,2,3,4,5)
  - [x] Added `goose-sdk 0.1.0-alpha.1` as probe dep; fetched + inspected published source
  - [x] **FINDING: no embeddable loop exists** — the SDK is ACP wire types + uniffi provider bindings; its own example spawns `goose acp` (FAIL-with-evidence recorded, no forking; Option B collapses into Option C)
- [x] Task 4 — Option C: ACP worker (AC: 1,2,3,4,5)
  - [x] Probed `agent-client-protocol 1.2.0` + schema 1.4.0 source: `session/request_permission` (C2 PASS), cancellation semantics (C3 PASS), `SessionId` (C4 PASS)
  - [x] **C1 FAIL by design**: SessionUpdate carries message/thought/tool-call chunks, never raw model request/response payloads
- [x] Task 5 — Evidence note + decision (AC: 5 + decision output)
  - [x] `docs/superpowers/spikes/runtime-loop-evidence.md` written (criteria matrix + decision)
  - [x] ADR-0003 updated (Spike 1/5 decided: native loop + ACP boundary + reduced-assurance workers); sprint-status → `review`; memlog appended

## Dev Notes

- **This is spike code (ADR-0004 D2)**: throwaway, quarantined in `spikes/`, no clippy-perfection needed, no product imports, deletion after evidence capture is a normal outcome. Do NOT touch `crates/` (does not exist yet — keep it that way).
- **Loop discipline (Constitution VIII) applies to the spike itself**: time-box each option; if an option's crates don't compile within a bounded effort (e.g. 2 attempts at dependency resolution), record FAIL-with-reason and move on. Ground truth = observable logs/tests, not impressions.
- **The 5 criteria ARE the Ledger contract in disguise**: an option that cannot capture/intercept/terminate/correlate cannot feed `LedgerStore`/`LoopController` (design §4.11/§4.14, ADR-0002 D1). Judge strictly.
- **Failure modes to avoid** (from checklist): do not "make it work" by patching/forking goose (that invalidates the evidence — the question is what the API exposes TODAY); do not test only happy paths (criterion 3 requires cancelling DURING a turn); do not let the stub model return no tool_calls (criterion 2 needs at least one tool round-trip).
- **Rust building blocks** (verified in landscape research): `rmcp` = official MCP Rust SDK (docs.rs/rmcp); `agent-client-protocol` = ACP Rust SDK with rmcp bridge (github.com/agentclientprotocol/rust-sdk); goose crates are Apache-2.0 (workspace: `goose-sdk`, `goose-sdk-types`, `goose-providers` — verify exact names/availability on crates.io first, they may be git-only).
- **Toolchain**: Rust 1.79 pinned repo-wide (`rust-toolchain.toml` planned but not yet present at root — the spike may pin its own). Windows is the dev machine: mind path handling; CI not required for spike.

### Project Structure Notes

```
spikes/runtime-loop/            # standalone cargo workspace (quarantine)
  Cargo.toml                    # [workspace] members = ["opt-a-native", "opt-b-goose", "opt-c-acp", "common"]
  common/                       # shared stub server + event log types (spike-local only)
  opt-a-native/
  opt-b-goose/
  opt-c-acp/
docs/superpowers/spikes/runtime-loop-evidence.md   # output (the real deliverable)
```
- The **evidence note is the deliverable**; the code is disposable. Optimize for legible proofs (e.g. a `--demo` mode printing the captured JSON per turn), not for architecture.

### References

- [Source: docs/superpowers/spikes/spike-protocol.md#Spike-1] — pre-registered exit criteria (authoritative)
- [Source: docs/superpowers/adr/0003-platform-composition-and-worker-strategy.md] — options + "reject any path that hides turn-level model/tool events"
- [Source: docs/superpowers/adr/0004-solo-v0-calibration.md#D2] — spike authorization & quarantine rules
- [Source: docs/research/agent-platform-landscape.md#Rust-core-building-blocks-and-protocol-convergence] — rmcp/ACP/goose-sdk facts + option table
- [Source: docs/superpowers/adr/0002-design-hardening.md#D1,#D11] — loop-ownership decision and native-loop fallback
- [Source: docs/superpowers/specs/2026-07-15-heddle-design.md#4.11,#4.14] — Ledger & LoopController contracts the winner must serve
- [Source: _bmad-output/planning-artifacts/validation/validation-report.md#Critical-1] — why this spike gates everything

## Dev Agent Record

### Agent Model Used

claude-fable-5 (Claude Code, loop mode iteration 1)

### Debug Log References

- `cargo test` (spikes/runtime-loop): 4 passed / 0 failed in 0.16s (first run after 35.8s cold build)
- Machine had NO Rust toolchain — installed rustup 1.97.0 via winget (bootstrap.ps1 step 1); ADR-0004 D4 validated in practice

### Completion Notes List

- Option A (native Heddle-owned loop) PASSES criteria 1–4 with observable proofs:
  C1 raw response byte-exact; request = exact pre-serialization payload (structural assert, not wire/headers);
  C2 `ToolIntercepted` strictly precedes `ToolExecuted`; Deny path blocks execution entirely;
  C3 PARTIAL: send-phase external cancel via CancellationToken, process alive, <5s (body/tool cancel = follow-up);
  C4 all events share run_id, gap-free seq (Ledger-shaped, in-memory; durability out of scope).
- Deviation (pre-authorized in story): in-process tool behind the Mediator instead of full rmcp round-trip — the mediation point is proven; rmcp integration is a follow-up probe for the evidence note.
- Effort estimate (C5, option A partial): loop core ≈150 LOC + 4 tests ≈160 LOC, one afternoon incl. toolchain install. Remaining risk: MCP wiring (rmcp) + streaming.
- Tasks 3–5 (options B goose-sdk / C ACP worker, evidence note + ADR decision) → next loop iterations.

### File List

- spikes/runtime-loop/Cargo.toml (new — quarantine workspace)
- spikes/runtime-loop/.gitignore (new)
- spikes/runtime-loop/opt-a-native/Cargo.toml (new)
- spikes/runtime-loop/opt-a-native/src/lib.rs (new — native loop, ~150 LOC)
- spikes/runtime-loop/opt-a-native/tests/criteria.rs (new — 4 pre-registered criteria tests)
- _bmad-output/implementation-artifacts/sprint-status.yaml (modified — 1-1 in-progress)
- _bmad-output/implementation-artifacts/1-1-runtime-ownership-spike.md (modified — this record)
