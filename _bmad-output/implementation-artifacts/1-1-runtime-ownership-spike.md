# Story 1.1: Runtime ownership spike (ADR-0003 Spike 1)

Status: ready-for-dev

## Story

As the Skein maintainer,
I want objective, side-by-side evidence of which execution tier lets Skein own the agent loop (native Rust loop vs embedded goose-sdk vs ACP worker),
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

- [ ] Task 1 — Scaffold quarantine workspace (AC: all)
  - [ ] Create `spikes/runtime-loop/` as a STANDALONE cargo workspace (own `Cargo.toml`; NOT a member of any root workspace; never imported by product code)
  - [ ] Stub OpenAI-compatible endpoint for deterministic tests (wiremock or a 20-line axum stub) so no network/model is required for criteria 1–4
- [ ] Task 2 — Option A: native loop (AC: 1,2,3,4,5)
  - [ ] Minimal loop: POST /chat/completions (reqwest or async-openai) → parse tool_calls → dispatch via `rmcp` client to a toy MCP server (or an in-process tool) → loop
  - [ ] Prove: log exact request/response JSON per turn; intercept tool call before exec; cancel via `tokio::select!`/CancellationToken mid-run; tag all events with run_id
- [ ] Task 3 — Option B: embedded goose-sdk (AC: 1,2,3,4,5)
  - [ ] Add `goose-sdk`/`goose-sdk-types` (or current crate names from crates.io / git) as deps; drive one session programmatically
  - [ ] Probe the API surface for: per-turn events? tool-call hook? cancellation? If a criterion has NO API, record FAIL with the evidence (doc/source link) — do not fork to make it pass
- [ ] Task 4 — Option C: ACP worker (AC: 1,2,3,4,5)
  - [ ] Use `agent-client-protocol` Rust SDK; drive `goose acp`/`goosed` (or the ACP example agent if goose unavailable) as an external worker
  - [ ] Probe: does ACP surface model payloads (or only assistant messages)? tool-call permission callbacks? cancellation? correlation ids?
- [ ] Task 5 — Evidence note + decision (AC: 5 + decision output)
  - [ ] Write `docs/superpowers/spikes/runtime-loop-evidence.md`: criteria matrix (A/B/C × 5 criteria, PASS/FAIL/PARTIAL + proof pointer), effort estimates, recommendation
  - [ ] Update ADR-0003 status accordingly; update sprint-status story → `review`; append memlog entry (decision)

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
- [Source: docs/superpowers/specs/2026-07-15-skein-design.md#4.11,#4.14] — Ledger & LoopController contracts the winner must serve
- [Source: _bmad-output/planning-artifacts/validation/validation-report.md#Critical-1] — why this spike gates everything

## Dev Agent Record

### Agent Model Used

(to fill at execution)

### Debug Log References

### Completion Notes List

### File List
