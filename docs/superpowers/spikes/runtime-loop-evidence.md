# Spike 1 Evidence — Runtime ownership (`spikes/runtime-loop/`)

**Date:** 2026-07-16 · **Story:** 1-1-runtime-ownership-spike · **Status:** COMPLETE
**Method:** pre-registered criteria (spike-protocol.md §Spike 1); ground truth = passing tests (option A) + published crate source inspection (options B/C). No option was patched/forked to "make it pass".

## Criteria matrix

| Criterion | A — native loop | B — embedded goose-sdk | C — ACP worker |
|---|---|---|---|
| C1 exact model I/O per turn | **PASS** — raw response captured **byte-exact** (`raws == [TOOL_TURN_BODY, FINAL_TURN_BODY]`); request captured as the **exact pre-serialization payload** (asserted structurally, not against wire bytes/headers — see Caveats) | **N/A** — no embeddable loop exists (see below) | **FAIL for existing workers** — no *standard* ACP field carries raw model I/O (`SessionUpdate` = message/thought/tool-call chunks, schema v1 `client.rs:100-113`); goose acp emits only usage/cost. **Not "impossible": protocol-legal via `_meta`/`ExtNotification` extensions (`ext.rs`) IF the worker cooperates** |
| C2 tool interception before exec | **PASS (call-side)** — `criterion_2…`: `ToolIntercepted` < `ToolExecuted`; Deny blocks execution; mediator consulted (external mutex). **Result-side mediation deferred to Spike 4** | N/A | **PASS** — `session/request_permission` (`client.rs:653,2284`) + `ToolCall/ToolCallUpdate` updates |
| C3 external termination mid-turn | **PARTIAL** — `criterion_3…`: cancel at 150ms vs a 10s-hanging model, process alive. Races the **send phase only**; body-read + tool-exec cancel points are follow-up (trivial to extend) | N/A | **PASS** — JSON-RPC cancellation + "Stop all language model requests as soon as possible" (`agent.rs:5223`) |
| C4 run correlation | **PASS** — `criterion_4…`: single run-id, gap-free monotonic seq. Ledger-**shaped** (in-memory Vec; streaming/durability out of scope) | N/A | **PASS** — `SessionId` threads all updates |
| C5 effort | ~150 LOC loop + ~160 LOC tests; one afternoon incl. toolchain bootstrap. Remaining: rmcp wiring, SSE streaming, cancel-point coverage. NB a published `goose-providers` crate offers an in-process provider layer (not a loop) as an alt to raw reqwest | — | SDK mature (v1.2.0, schema v1.4.0); client integration = days; full-fidelity C1 needs a non-standard extension |

Test run: `cargo test` → **4 passed / 0 failed** (commit `712921b`). Reviewed adversarially (BMAD blind-hunter) 2026-07-16 — verdict CHANGES-REQUIRED (documentation-only); fixes applied here.

## Adversarial review corrections (applied)

- **C1/Option-C is NOT "FAIL by design".** ACP has a first-class extension mechanism (`_meta` on ~every type; `ExtRequest`/`ExtNotification` for `_`-prefixed methods — `agent-client-protocol-schema-1.4.0/src/v1/ext.rs`; goose already uses it for `_goose/unstable/session/update`). Correct claim: **raw model I/O is unavailable through *existing* workers but protocol-legal via extensions requiring worker cooperation.** This *strengthens* the decision: Heddle's own ACP facade can publish full-fidelity evidence through `_meta`.
- **C3 is PARTIAL** (send-phase cancellation proven; body/tool cancel points are production follow-up).
- **C1 request-side & C4 wording softened** ("exact pre-serialization payload", "Ledger-shaped in-memory") to match what the tests actually prove. Headers (auth) are not captured; one `received_requests()` byte assertion would close the request-side gap.

## Option B finding (decisive)

The published `goose-sdk 0.1.0-alpha.1` is **not an embeddable agent runtime**: it re-exports ACP wire types "so you can build an Agent Client Protocol (ACP) client that talks to `goose acp` over stdio" (lib.rs docstring) plus uniffi provider bindings. Its **own example** (`examples/acp_client.rs`) spawns `goose acp` as a child process. Embedding Goose's loop would require git-depending on unpublished workspace internals — rejected (no stability contract, high maintenance). **Option B collapses into Option C.**

## Decision

1. **Heddle's governed execution tier = Option A: native Heddle-owned loop.** It is the only option satisfying C1 (byte-exact Ledger capture, design §4.11) — which is non-negotiable for the transparency/reversibility promise.
2. **Adopt ACP as Heddle's client↔core boundary.** The ACP schema (message/thought/tool-call/plan updates, permission requests, cancellation, session ids) is almost exactly Heddle's event contract; exposing the native loop *through* ACP buys free interop (Zed, goose clients) — consistent with the landscape recommendation.
3. **External ACP agents (goose acp, OpenCode, …) = reduced-assurance workers** (QUALITY-GATES G5 class): C2–C4 governable; C1 **unavailable absent worker cooperation** (a worker that emits raw model I/O via `_meta`/ext could raise its assurance) → by default their runs carry a typed unavailable-evidence marker, never full Ledger fidelity.

## Consequences for ADR-0003

Spike 1 of 5 complete. The runtime-ownership question is answered with evidence; Spikes 2–5 (workflow reuse, context quality, tool governance via rmcp, install UX) remain before full ADR acceptance.
