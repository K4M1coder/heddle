# ADR-0003: Platform composition and worker strategy

**Status:** **Accepted** (2026-07-16) — architecture direction confirmed by 4/5 spikes on tested code (1 runtime→native loop+ACP boundary; 2 workflow→lossless Archon↔canonical; 3 context→repo-map beats full-context at representative scale; 4 tool governance→rmcp gateway deny/approve/redact/replay). Spike 5 (tri-OS offline install) is *operational, not architectural*: it does not gate this decision and is now mechanized by `.github/workflows/spikes.yml` (must go green on Windows/macOS/Linux before product build authorization). Evidence: `docs/superpowers/spikes/*-evidence.md`.

**Rationale for accepting before Spike 5:** the one-way-door questions ADR-0003 exists to answer — who owns the loop, tool governance, workflow reuse, context strategy — are all resolved by Spikes 1–4. Spike 5 tests packaging/portability, which can revise the *bootstrap*, never the *architecture*. Deferring acceptance for it would be process theater (ADR-0004 D1).  
**Date:** 2026-07-15  
**Decider:** Cédric Thedrez (`kamicoder`)  
**Research:** `docs/research/agent-platform-landscape.md`

## Context

Heddle aims to combine the useful behavior of coding agents, personal assistants, workflow engines, chat platforms, local inference systems and enterprise connectors while remaining simple, local-first, governable and cross-platform.

Making Goose, OpenCode, Cline, Hermes, Archon or another product the internal source of truth would hide important events or couple Heddle's security and persistence semantics to an external runtime. Reimplementing every commodity capability would be equally risky and wasteful.

## Decision

Heddle will be distributed as a **single product and modular monorepo**, implemented as a **modular monolith with supervised optional sidecars and workers**.

Heddle owns the control plane and canonical contracts. Existing agents may be:

- optional workers;
- sources of compatible libraries or crates;
- references for UX and behavior;
- never the owner of policy, workflow state, artifacts, context manifests, evidence or completion.

The default runtime path is a Heddle-owned loop. Embedded Goose crates, goosed, OpenCode, Cline, Hermes, Claude Code and other agents are evaluated as `WorkerAdapter` implementations. A worker is accepted only if Heddle can observe and govern every model turn, tool request, approval, effect and termination signal required by the selected execution contract.

## Options considered

### A. Fork Goose as the product core

**Pros:** Rust, Apache-2.0, MCP-native, broad providers and desktop components.  
**Cons:** Heddle requirements exceed Goose's ownership model; turn-level visibility and event semantics may not match; a long-lived fork increases upstream merge cost.

**Decision:** Reject as default. Reuse crates or adapter paths only after spike evidence.

### B. Build on OpenCode/Cline/Hermes

**Pros:** mature agent features, open licenses, fast route to parity.  
**Cons:** language/runtime mismatch, different persistence and security boundaries, product-specific assumptions.

**Decision:** Optional workers and selective code inspiration/reuse, not control-plane dependencies.

### C. Embed Archon as workflow engine

**Pros:** workflow semantics closely match Heddle; YAML, deterministic nodes, loops, worktrees and approvals.  
**Cons:** canonical artifacts, Ledger, policy and team hierarchy differ; TypeScript/Bun runtime adds a mandatory sidecar if embedded unchanged.

**Decision:** Build an Archon-inspired canonical workflow engine; investigate parser/schema/code reuse and import/export compatibility.

### D. Heddle-owned control plane with replaceable adapters

**Pros:** satisfies local-first, evidence, RBAC, loop control and enterprise requirements; allows gradual reuse of best components.  
**Cons:** more initial design work and responsibility for stable contracts.

**Decision:** Accepted direction, pending spikes.

## Consequences

- The Phase 0 runtime spike must compare at least native Rust, embedded/goosed Goose, OpenCode and Cline integration surfaces.
- A `WorkerAdapter` and `CapabilityDescriptor` become one-way-door contracts and require examples, contract tests and versioning rules before implementation.
- The UI may borrow interaction patterns but must communicate only with Heddle's API.
- Open WebUI is not embedded under its current license.
- LiteLLM is an initial replaceable adapter; Heddle's capability and policy model remains independent.
- The repository may exceed one million tokens over time; context selection, not whole-repository injection, is a core product capability.

## Quality gate

This ADR becomes **Accepted** only when the five spikes listed in `docs/research/agent-platform-landscape.md` have evidence bundles and the BMAD architecture-readiness and Spec-Kit analysis gates report no unresolved critical contradiction.

