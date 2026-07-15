# ADR-0003: Platform composition and worker strategy

**Status:** Proposed — requires architecture-readiness gate  
**Date:** 2026-07-15  
**Decider:** Cédric Thedrez (`kamicoder`)  
**Research:** `docs/research/agent-platform-landscape.md`

## Context

Skein aims to combine the useful behavior of coding agents, personal assistants, workflow engines, chat platforms, local inference systems and enterprise connectors while remaining simple, local-first, governable and cross-platform.

Making Goose, OpenCode, Cline, Hermes, Archon or another product the internal source of truth would hide important events or couple Skein's security and persistence semantics to an external runtime. Reimplementing every commodity capability would be equally risky and wasteful.

## Decision

Skein will be distributed as a **single product and modular monorepo**, implemented as a **modular monolith with supervised optional sidecars and workers**.

Skein owns the control plane and canonical contracts. Existing agents may be:

- optional workers;
- sources of compatible libraries or crates;
- references for UX and behavior;
- never the owner of policy, workflow state, artifacts, context manifests, evidence or completion.

The default runtime path is a Skein-owned loop. Embedded Goose crates, goosed, OpenCode, Cline, Hermes, Claude Code and other agents are evaluated as `WorkerAdapter` implementations. A worker is accepted only if Skein can observe and govern every model turn, tool request, approval, effect and termination signal required by the selected execution contract.

## Options considered

### A. Fork Goose as the product core

**Pros:** Rust, Apache-2.0, MCP-native, broad providers and desktop components.  
**Cons:** Skein requirements exceed Goose's ownership model; turn-level visibility and event semantics may not match; a long-lived fork increases upstream merge cost.

**Decision:** Reject as default. Reuse crates or adapter paths only after spike evidence.

### B. Build on OpenCode/Cline/Hermes

**Pros:** mature agent features, open licenses, fast route to parity.  
**Cons:** language/runtime mismatch, different persistence and security boundaries, product-specific assumptions.

**Decision:** Optional workers and selective code inspiration/reuse, not control-plane dependencies.

### C. Embed Archon as workflow engine

**Pros:** workflow semantics closely match Skein; YAML, deterministic nodes, loops, worktrees and approvals.  
**Cons:** canonical artifacts, Ledger, policy and team hierarchy differ; TypeScript/Bun runtime adds a mandatory sidecar if embedded unchanged.

**Decision:** Build an Archon-inspired canonical workflow engine; investigate parser/schema/code reuse and import/export compatibility.

### D. Skein-owned control plane with replaceable adapters

**Pros:** satisfies local-first, evidence, RBAC, loop control and enterprise requirements; allows gradual reuse of best components.  
**Cons:** more initial design work and responsibility for stable contracts.

**Decision:** Accepted direction, pending spikes.

## Consequences

- The Phase 0 runtime spike must compare at least native Rust, embedded/goosed Goose, OpenCode and Cline integration surfaces.
- A `WorkerAdapter` and `CapabilityDescriptor` become one-way-door contracts and require examples, contract tests and versioning rules before implementation.
- The UI may borrow interaction patterns but must communicate only with Skein's API.
- Open WebUI is not embedded under its current license.
- LiteLLM is an initial replaceable adapter; Skein's capability and policy model remains independent.
- The repository may exceed one million tokens over time; context selection, not whole-repository injection, is a core product capability.

## Quality gate

This ADR becomes **Accepted** only when the five spikes listed in `docs/research/agent-platform-landscape.md` have evidence bundles and the BMAD architecture-readiness and Spec-Kit analysis gates report no unresolved critical contradiction.

