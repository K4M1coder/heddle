---
stepsCompleted: []
inputDocuments: ['_bmad-output/planning-artifacts/PRD.md', '_bmad-output/planning-artifacts/architecture.md']
---

# Skein - Epic Breakdown

## Overview
Breakdown into epics/stories, derived from the PRD and the architecture. **This iteration details Epic 1 (Phase 0 — vertical skeleton)**; epics 2+ (v1 axes, then v2→v8) will be detailed in turn, each with its own inherited spine.

## Requirements Inventory
### Functional Requirements (covered by Epic 1)
- FR-1 (headless agentic loop), FR-3 (provider selection via Gateway), FR-6 (Local silo + isolation), FR-10 (Ledger), FR-11 (SecretProvider foundation).
### NonFunctional Requirements
- TDD, cross-platform (tri-OS CI), observability (tracing), egress OFF in Local, tested isolation.

### FR Coverage Map (Epic 1)
FR-1 → Stories 1.6/1.7 · FR-3 → Story 1.4 · FR-6 → Story 1.3 · FR-10 → Story 1.8 · FR-11 → Story 1.9.

## Epic List
- **Epic 1 — Phase 0: Vertical skeleton (Walking Skeleton)** *(detailed below)*
- Epic 2 — v1/1a Agentic code assistant (fs/git/shell, TDD, subagents)
- Epic 3 — v1/1b Multi-provider + local inference
- Epic 4 — v1/1c Atlassian + M365 connectors
- Epic 5 — v1/1d BMAD/Spec-Kit/powerskills frameworks
- **Epic 6 — v1/1e Native workflow engine (Ledger event-sourced) + TaskTracker (local/Vikunja/Jira) + hierarchy & config resolution** → Spec-Kit feature `specs/002-workflow-engine/`
- Epic 7 — v1 Modes/silos (Server/Remote + team authz) & Chat/Code UI
- Epics 7+ — v2→v8 (perception, cowork, generation, video, omni, voice, translation) & enterprise track

## Epic 1: Phase 0 — Vertical skeleton
**Goal:** prove the complete vertical slice `CLI → Skein control plane → selected per-turn runtime/worker → model gateway → model`, with Local silo persistence, Ledger, secrets foundation and turn-level governance. The runtime path is selected by ADR-0003 evidence before implementation. Independently testable deliverable. TDD implementation detail: `specs/001-phase0-walking-skeleton/tasks.md` after regeneration.

### Story 1.0: Goose integration spike (ADR)
As a maintainer, I want to settle the Goose integration on facts, So that the implementation tasks are concrete.
**Acceptance Criteria:**
**Given** candidate runtimes/workers are available **When** the ADR-0003 spike suite measures turn-level events, tool mediation, correlation, termination and local packaging **Then** an evidence bundle selects the Phase 0 path or defaults to the native Rust loop.

### Story 1.1: Workspace scaffolding + tri-OS CI
As a dev, I want a Cargo workspace + CI, So that the code compiles and is verified on Windows/macOS/Linux.
**Given** the repository **When** CI runs **Then** `fmt`/`clippy -D warnings`/`test` pass on all 3 OSes.

### Story 1.2: Domain types (Content/Message/Event)
As a dev, I want serializable typed types, So that the pipeline carries structured content.
**Given** a `Message` **When** serialized/deserialized **Then** it round-trips without loss.

### Story 1.3: SiloStore SQLite (isolation)
As a user, I want my sessions persisted and isolated per silo, So that no data leaks between modes.
**Given** a write in the `local` silo **When** another namespace is opened **Then** nothing is visible (isolation test green). Realizes FR-6.

### Story 1.4: GatewayClient (OpenAI-compat) + LiteLLM config
As a user, I want to call a model through a single gateway, So that I can switch cloud↔local.
**Given** an OpenAI-compat endpoint **When** `complete()` is called **Then** the content is extracted; `health()` reflects the state. Realizes FR-3.

### Story 1.5: GooseRuntime (headless CLI adapter)
As a developer, I want a versioned `WorkerAdapter` contract and one evidence-selected implementation, so that the control plane remains independent from every agent runtime.
**Given** a goose binary (stub) **When** `run()` executes **Then** stdout→`Event::Token`, end→`Event::Done`.

### Story 1.6: ChatService (orchestration + persistence)
As a user, I want a persisted conversation, So that I can reload it.
**Given** a prompt **When** `chat()` executes **Then** user+assistant messages are persisted in the silo and reloadable. Realizes FR-1.

### Story 1.7: Reference CLI (chat, session)
As a user, I want to drive everything from the terminal, So that the tool is scriptable and testable.
**Given** `skein chat -t ...` **When** executed **Then** output is displayed + `session show` reloads user+assistant. Realizes FR-1/AD-1.

### Story 1.8: Event-sourced Ledger (capture & inspection)
As a user, I want to inspect everything sent/received, So that I retain transparency and reversibility.
**Given** a chat **When** `skein ledger log` **Then** both LlmRequest AND LlmResponse appear, hash-chained, isolated per silo. Realizes FR-10.

### Story 1.9: SecretProvider foundation (JIT)
As a user, I want to resolve secrets just-in-time, So that no secret is in cleartext or logged.
**Given** a stored key (OS keyring) **When** `skein gateway-health` resolves it **Then** the key never appears in cleartext; `redact` masks it. Realizes FR-11.

### Story 1.10: Exit-criteria verification (smoke test)
As a PM, I want to validate Phase 0 end-to-end, So that the architecture is proven.
**Given** Ollama+LiteLLM+Goose configured **When** `skein chat` creates a file **Then** the file is created, the session persisted/reloaded, the ledger inspectable, egress OFF confirmed.
