# ADR-0004: Solo-v0 calibration — right-sized gates, spike authorization, brutal de-scoping

**Status:** Accepted
**Date:** 2026-07-16
**Decider:** Cédric Thedrez (`kamicoder`)
**Supersedes/amends:** calibrates `docs/QUALITY-GATES.md` (G0–G6) and the PRD MVP scope for the v0 reality; does not change ADR-0003's architecture direction.

## Context

The BMAD validation pass produced enterprise-grade quality gates (G0–G6: 100% evidence completeness, compliance registries, isolation matrices, threat models, truth tables). The architecture direction (Heddle-owned control plane, ADR-0003) is validated by three independent passes. But two calibration errors remain:

1. **Process weight exceeds a solo open-source v0.** The cure for the "platform-sized MVP" finding is to *shrink the scope to build*, not to *add gates*. Unchecked, G0–G6 produce governance theater and paralysis for a one-person project.
2. **The pipeline blocks the very experiments that unblock it.** The five ADR-0003 spikes produce code; `build_authorization: NOT_READY` reads as "no code", creating a deadlock.

Also observed: the bootstrap failed in one agent session (Python/uv absent from that shell's PATH) while working in another — the bootstrap must be shell-agnostic, and its verify step must fail loudly.

## Decision

### D1 — Two-tier gate calibration
- **Tier "solo-v0" (now):** the *only* mandatory gates before product code are:
  (a) bucket-A one-way-door contracts written and reviewed (Ledger/event schema, silo isolation + config resolution, loop ownership, egress boundary, erasure mechanism);
  (b) the five ADR-0003 spikes executed with evidence;
  (c) Spec-Kit clarify → plan → tasks → analyze green for the current slice only;
  (d) tri-OS CI green (fmt, clippy -D warnings, tests, isolation tests).
- **Tier "team/enterprise" (deferred, trigger-based):** the full G0–G6 regime (evidence completeness ratios, compliance registries, isolation matrices, formal threat models) activates when a real team deployment or enterprise adoption is on the table — not before. `QUALITY-GATES.md` remains the *target* contract; it is not the v0 entry bar.

### D2 — Spike code is explicitly authorized
- `build_authorization: NOT_READY` applies to **product code** only.
- **Spike code is authorized now**, under quarantine rules: lives in `spikes/` (never `crates/`), throwaway by default, no product dependency may import it, each spike has pre-registered exit criteria and produces an evidence note in `docs/superpowers/spikes/`. Deleting a spike after its evidence is captured is the normal outcome.

### D3 — v0 scope: strict-local coding agent core
The only honest v0 is a **strict-local coding agent**: Heddle-owned loop (per ADR-0003/landscape: ACP-shaped core boundary), MCP tools (fs/git/shell), one local model path (Ollama via gateway), silo Local + Ledger + SecretProvider foundation + LoopController budgets, CLI surface. **Everything else** — Atlassian/M365 connectors, team modes over network, UI, multimodal v2–v8, workflow engine full form, IdP/RBAC advanced, task trackers — stays specified but **out of v0 build scope**. Scope additions to v0 require an explicit ADR, not a conversation drift.

### D4 — Bootstrap hardening
`scripts/bootstrap.{ps1,sh}` must verify tool availability *in the same shell profile the agent/dev will use*, fail loudly with remediation hints, and never rely on incidental venvs from unrelated projects. PATH prerequisites are documented in `docs/DEVELOPMENT.md`.

## Consequences
- The next concrete step is **Spike 1 (runtime ownership)** under D2 quarantine rules — protocol in `docs/superpowers/spikes/spike-protocol.md`.
- The PRD stays canonical; its MVP section is to be read through D3 (strict-local core first). Epics/stories regeneration targets the v0 scope only.
- `QUALITY-GATES.md` gains a calibration note pointing here; its full regime is preserved for the team/enterprise trigger.
