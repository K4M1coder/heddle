---
name: 'Skein'
type: architecture-spine
purpose: build-substrate
altitude: initiative
paradigm: 'hexagonal (ports & adapters) + event sourcing'
scope: "Headless agentic core, modes/silos, connectors, providers, identity, secrets, ledger, surfaces"
status: draft
created: '2026-07-15'
updated: '2026-07-15'
binds: [FR-1, FR-2, FR-3, FR-4, FR-5, FR-6, FR-7, FR-8, FR-9, FR-10, FR-11, FR-12, FR-13, FR-14, FR-15, FR-16]
sources: ['docs/superpowers/specs/2026-07-15-skein-design.md', '_bmad-output/planning-artifacts/PRD.md']
companions: ['.specify/memory/constitution.md']
---

# Architecture Spine — Skein

## Design Paradigm
**Hexagonal (ports & adapters)** + **event sourcing**. The core (agentic domain) depends only on **ports** (traits); external building blocks are interchangeable **adapters**. State is derived from an **immutable event log** (Ledger).

Layers → directories: `crates/skein-core/` (domain + ports), `crates/skein-*-adapter*` & `connectors/` (adapters), `crates/skein-cli/` + `ui/` (surfaces), `sidecar/` (Python), `gateway/` (LiteLLM).

## Invariants & Rules

### AD-1 — Headless core; CLI is authoritative; UI as an overlay
- **Binds:** all surfaces (FR-1)
- **Prevents:** UI-exclusive capabilities that cannot be automated or tested.
- **Rule:** every capability goes through the core API; the CLI exposes it in full; the UI only emits CLI/API commands. [ADOPTED]

### AD-2 — Per-silo isolation at I/O boundaries
- **Binds:** FR-6, FR-10, FR-11, backend
- **Prevents:** data leakage between modes/teams.
- **Rule:** all data access is resolved via `Backend.store(mode, team)`; reads/writes are confined; no cross-silo queries. Enforced by an isolation test. [ADOPTED]

### AD-3 — Inverted coupling through ports
- **Binds:** connectors, providers, identity, secrets, controller
- **Prevents:** dependency of the core on a concrete implementation.
- **Rule:** the core knows only the traits `AgentRuntime`, `ModelGateway`, `Backend`, `IdentityProvider`, `SecretProvider`, `Controller`, `Ledger`; concrete implementations are injected.

### AD-4 — Egress governed by mode
- **Binds:** FR-3, FR-11, security
- **Prevents:** unintended network egress in Local mode.
- **Rule:** in Local mode, only adapters with `requires_network()==false` may be used; cloud egress requires Server/Remote mode plus an explicit policy. [ADOPTED]

### AD-5 — Append-only traceability, secrets by reference
- **Binds:** FR-10, FR-11
- **Prevents:** loss of traceability; secrets in cleartext within logs.
- **Rule:** every step is logged (Ledger, hash-chained); secrets are references resolved JIT and **redacted** before any persistence/log.

### AD-6 — Deny-by-default authorization across 3 scopes
- **Binds:** FR-8, FR-7
- **Prevents:** implicit access.
- **Rule:** RBAC evaluated global → silo → intra-silo; deny by default; harness locks = intra-silo permissions.

### AD-7 — Event-sourced workflow on the Ledger
- **Binds:** FR-13
- **Prevents:** non-resumable / non-traceable orchestration; a proprietary engine being tightly coupled.
- **Rule:** every workflow executes by logging each step as a Ledger `Step`; resumption happens from the last Step; the concrete engine sits behind the `WorkflowEngine` trait (native by default; Temporal/Windmill optional).

### AD-9 — Engine-enforced loop control (loop engineering)
- **Binds:** FR-16, FR-13, FR-1
- **Prevents:** runaway loops; reflect/retry that trusts model self-judgment (unreliable, can degrade output).
- **Rule:** every agent loop / loop node runs under a `LoopController` — termination/budgets are engine-enforced (the model never decides to stop), and reflect/retry is anchored to external ground truth (tests/compiler/linters/tool results). Loop state + reflections persist to the Ledger (AD-5, AD-7); breaches escalate to a human (AD-6, §7.4). Verify at 3 levels: action / iteration / terminal.

### AD-8 — Hierarchical config resolution, "the highest level locks"
- **Binds:** FR-14, FR-15, FR-7 (harness), FR-3/FR-11 (egress/secrets)
- **Prevents:** config divergence between levels; bypassing a higher-level lock.
- **Rule:** config is resolved along Silo▸Team▸Project▸Conversation; a setting fixed at one level is authoritative for the levels below it; otherwise the lowest level applies. A single resolver for harness/tracker/egress/providers/secrets. Remains bounded to the silo (AD-2).

### Allowed dependencies (who may depend on whom)

```mermaid
graph TD
  UI[UI Tauri] --> CLI[CLI / API]
  CLI --> CORE[skein-core: domain + ports]
  CORE --> RT[AgentRuntime port]
  CORE --> GW[ModelGateway port]
  CORE --> BK[Backend/Silos port]
  CORE --> ID[IdentityProvider port]
  CORE --> SEC[SecretProvider port]
  CORE --> LED[Ledger port]
  CORE --> CTL[Controller port]
  RT --> GOOSE[Goose adapter]
  GW --> LITELLM[LiteLLM adapter]
  BK --> SQLITE[SQLite / Remote adapter]
  SEC --> KEYRING[OS/SOPS/1Password/OpenBao/Infisical adapters]
  GOOSE --> LITELLM
```

## Consistency Conventions

| Concern | Convention |
| --- | --- |
| Naming | crates `skein-*`; traits in `PascalCase` (`AgentRuntime`); modules `snake_case` |
| Data & formats | session ids `s%06d`; epoch timestamps (i64); errors via `SkeinError` (thiserror); JSON for persisted payloads |
| State & cross-cutting | mutation via event append (Ledger); logging with `tracing`/OpenTelemetry; versioned config-as-code; deny-by-default auth; secrets by reference |

## Stack

| Name | Version |
| --- | --- |
| Rust | 1.79 (MSRV) |
| Goose | upstream dependency (v6.x) |
| LiteLLM | proxy (100+ providers) |
| SQLite (rusqlite) | 0.31 (bundled) |
| Tauri | 2.x (UI, v1+) |
| Python (sidecar) | 3.11+ (uv) |
| OpenTelemetry | via `tracing` |

## Structural Seed

```text
skein/
  crates/
    skein-core/   # domain + ports (traits) + local implementations
    skein-cli/    # reference surface (bin `skein`)
  connectors/     # MCP servers (Atlassian, M365, fs, git, shell)
  gateway/        # LiteLLM config
  sidecar/        # Python (embeddings/RAG, v2+)
  ui/             # Tauri (v1 Chat+Code)
  _bmad-output/   # BMAD artifacts (planning/implementation)
  specs/          # Spec-Kit artifacts (per-feature)
  .specify/       # Spec-Kit constitution + templates + workflows
```

## Capability → Architecture Map

| Capability / FR | Lives in | Governed by |
| --- | --- | --- |
| FR-1 agentic loop | skein-core + Goose adapter | AD-1, AD-3 |
| FR-3 multi-provider | ModelGateway port + LiteLLM | AD-3, AD-4 |
| FR-6 modes/silos | Backend port + ModeSupervisor | AD-2 |
| FR-8 identity/RBAC | IdentityProvider port + RBAC | AD-6 |
| FR-10 ledger | Ledger port + Gateway capture | AD-5 |
| FR-11 secrets | SecretProvider port | AD-4, AD-5 |
| FR-12 cowork/multimodal | Controller port + typed Content | AD-3 |
| FR-13 workflow | WorkflowEngine port + Ledger | AD-7, AD-5 |
| FR-14 task tracking | TaskTracker port (local/Vikunja/Jira) | AD-3, AD-8 |
| FR-15 hierarchy/config | Silo▸Team▸Project▸Conversation resolver | AD-8, AD-2 |

## Deferred
- Python/RAG sidecar, full Tauri UI, networked Server/Remote modes, external IdPs, multimodal v2+, v7 duplex channel: pushed to their respective versions (see PRD §6 & design §8). The *initiative* altitude establishes the invariants; each epic/feature spine will inherit the ADs above through their original ids.
