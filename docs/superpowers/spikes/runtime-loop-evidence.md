# Spike 1 Evidence — Runtime ownership (`spikes/runtime-loop/`)

**Date:** 2026-07-16 · **Story:** 1-1-runtime-ownership-spike · **Status:** COMPLETE
**Method:** pre-registered criteria (spike-protocol.md §Spike 1); ground truth = passing tests (option A) + published crate source inspection (options B/C). No option was patched/forked to "make it pass".

## Criteria matrix

| Criterion | A — native loop | B — embedded goose-sdk | C — ACP worker |
|---|---|---|---|
| C1 exact model I/O per turn | **PASS** — `criterion_1_captures_exact_model_io` asserts byte-exact raw bodies + exact request payloads, incl. tool-result feedback into turn 2 | **N/A** — no embeddable loop exists (see below) | **FAIL by design** — ACP carries `UserMessageChunk/AgentMessageChunk/AgentThoughtChunk`, never the raw model request/response (schema v1 `client.rs:100-113`) |
| C2 tool interception before exec | **PASS** — `criterion_2…`: `ToolIntercepted` < `ToolExecuted`; Deny blocks execution | N/A | **PASS** — `session/request_permission` (`client.rs:653,2284`) + `ToolCall/ToolCallUpdate` updates |
| C3 external termination mid-turn | **PASS** — `criterion_3…`: cancel at 150ms against a 10s-hanging model; process alive | N/A | **PASS** — JSON-RPC cancellation + "Stop all language model requests as soon as possible" (`agent.rs:5223`) |
| C4 run correlation | **PASS** — `criterion_4…`: single run-id, gap-free monotonic seq | N/A | **PASS** — `SessionId` threads all updates |
| C5 effort | ~150 LOC loop + ~160 LOC tests; one afternoon incl. toolchain bootstrap. Remaining: rmcp wiring, streaming | — | SDK mature (v1.2.0, schema v1.4.0); client integration = days, but C1 unfixable at this layer |

Test run: `cargo test` → **4 passed / 0 failed** (commit `712921b`).

## Option B finding (decisive)

The published `goose-sdk 0.1.0-alpha.1` is **not an embeddable agent runtime**: it re-exports ACP wire types "so you can build an Agent Client Protocol (ACP) client that talks to `goose acp` over stdio" (lib.rs docstring) plus uniffi provider bindings. Its **own example** (`examples/acp_client.rs`) spawns `goose acp` as a child process. Embedding Goose's loop would require git-depending on unpublished workspace internals — rejected (no stability contract, high maintenance). **Option B collapses into Option C.**

## Decision

1. **Skein's governed execution tier = Option A: native Skein-owned loop.** It is the only option satisfying C1 (byte-exact Ledger capture, design §4.11) — which is non-negotiable for the transparency/reversibility promise.
2. **Adopt ACP as Skein's client↔core boundary.** The ACP schema (message/thought/tool-call/plan updates, permission requests, cancellation, session ids) is almost exactly Skein's event contract; exposing the native loop *through* ACP buys free interop (Zed, goose clients) — consistent with the landscape recommendation.
3. **External ACP agents (goose acp, OpenCode, …) = reduced-assurance workers** (QUALITY-GATES G5 class): C2–C4 governable, C1 impossible → their runs carry a typed unavailable-evidence marker, never full Ledger fidelity.

## Consequences for ADR-0003

Spike 1 of 5 complete. The runtime-ownership question is answered with evidence; Spikes 2–5 (workflow reuse, context quality, tool governance via rmcp, install UX) remain before full ADR acceptance.
