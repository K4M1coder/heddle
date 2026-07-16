# Spike Protocol — ADR-0003 evidence spikes (authorized by ADR-0004 D2)

Rules (all spikes): code lives in `spikes/<name>/`, throwaway, no product imports, bounded budget (time-boxed, one retry per blocker), exit criteria **pre-registered below** (ground truth = observable behavior, never self-judgment), evidence note written to `docs/superpowers/spikes/<name>-evidence.md` before the spike is considered done. Order: Spike 1 first (it gates the others' shape); 2–5 may run after or in parallel where independent.

## Spike 1 — Runtime ownership (`spikes/runtime-loop/`)
**Question:** which execution tier lets Skein own the loop — (A) native Rust loop (`rmcp` + OpenAI-compat client), (B) embedded `goose-sdk` crates, (C) ACP worker (drive goosed/OpenCode via `agent-client-protocol`)?
**Exit criteria (all must be observable per option):**
1. Capture the **exact request payload** sent to the model and the **raw response**, per turn.
2. Intercept **every tool call + result** before execution (mediation point exists).
3. **Terminate the loop externally** mid-run (budget enforcement) without process kill.
4. Correlate all events of one run under one run-id (Ledger-ready).
5. Rough effort/maintenance estimate per option.
**Decision output:** pick A/B/C (or A-behind-ACP-facade), record in ADR-0003 → Accepted/Revised.

## Spike 2 — Workflow reuse (`spikes/workflow-archon/`)
**Question:** can one Archon-style YAML workflow map losslessly onto Skein's canonical graph (nodes agent/tool/approval/cond/parallel/loop)?
**Exit:** one real workflow parsed → executed as a stub graph → round-tripped back to YAML with no semantic loss; gaps listed.

## Spike 3 — Context quality (`spikes/context-repomap/`)
**Question:** does repo-map (tree-sitter) + hybrid retrieval beat naive full-file context on a representative repo?
**Exit:** on ≥10 queries over a mid-size repo: selected-context answers ≥ full-context answers on correctness while using ≤20% of the tokens; middle-position retrieval tested.

## Spike 4 — Tool governance (`spikes/mcp-gateway/`)
**Question:** can Skein proxy MCP servers (1 local stdio + 1 remote OAuth) through policy/approval/redaction/Ledger capture using `rmcp`?
**Exit:** a tool call is (1) blocked by policy, (2) allowed after approval, (3) logged with redacted secret, (4) replayed from the captured record.

## Spike 5 — Single-package UX (`spikes/install-offline/`)
**Question:** fresh install + first offline run on Windows, macOS, Linux?
**Exit:** on each OS (VM/CI): bootstrap script → build → `skein`-equivalent binary runs a local-model turn with network disabled; failures documented per OS.
