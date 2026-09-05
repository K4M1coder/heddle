# Feature Specification: Phase 0 — Vertical Skeleton (Walking Skeleton)

**Feature Branch**: `001-phase0-walking-skeleton`

**Created**: 2026-07-15

**Status**: Draft — architecture-spike blocked; plan/tasks require regeneration

**Input**: Derived from `_bmad-output/planning-artifacts/epics.md` (Epic 1) and the design `docs/superpowers/specs/2026-07-15-heddle-design.md` (§8 Phase 0).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Persisted conversation that acts on files (Priority: P1)
As a user, I start a conversation from the terminal; the agent reads/writes a file, and my session is persisted and can be reloaded.

**Why this priority**: this is the exit criterion for Phase 0 — it proves the complete vertical slice (CLI → Heddle control plane → governed per-turn runtime/worker → Gateway → model → silo).

**Independent Test**: `heddle chat -t "..."` then `heddle session show <id>` reloads user+assistant; an expected file exists.

**Acceptance Scenarios**:
1. **Given** a configured local model, **When** I run `heddle chat -t "create hello.txt containing heddle"`, **Then** `hello.txt` is created and the session `s000001` is persisted.
2. **Given** an existing session, **When** I run `heddle session show s000001`, **Then** the user and assistant messages are displayed.

### User Story 2 - Strict silo isolation (Priority: P1)
As a user, my Local mode data is not visible in any other silo.

**Why this priority**: foundational security invariant (constitution II); it must hold from the skeleton onward.

**Independent Test**: an automated test that writes to the `local` silo and proves invisibility in another namespace.

**Acceptance Scenarios**:
1. **Given** a write to `local`, **When** I open the `remote` namespace, **Then** no session/message is visible and `load` fails.

### User Story 3 - Transparency & secrets (Priority: P2)
As a user, I can see everything sent to/received from the model (Ledger) and no secret appears in cleartext.

**Why this priority**: transparency/reversibility (constitution V) and JIT secrets (VI) — foundations to anchor early.

**Independent Test**: `heddle ledger log <session>` lists LlmRequest+LlmResponse; `heddle gateway-health` resolves a key from the keyring without ever displaying it.

**Acceptance Scenarios**:
1. **Given** a completed chat, **When** `heddle ledger log s000001`, **Then** both LlmRequest AND LlmResponse appear (hash-chained).
2. **Given** a key stored in the keyring, **When** `heddle gateway-health`, **Then** health is verified without exposing the key.

### Edge Cases
- Offline: Local mode (egress OFF) works with a local model; secret resolution via the OS keyring works without a network.
- Goose binary missing/failing: clean `Event::Error`, non-zero exit code handled.
- Nonexistent session: `session show` returns an explicit `NotFound` error.

## Requirements *(mandatory)*

### Functional Requirements
- **FR-001**: The system MUST expose a headless core driven by a CLI (`heddle chat`, `session list|show`, `ledger log|show`, `secret-set`, `gateway-health`).
- **FR-002**: The system MUST run an agentic loop via Goose in headless mode (adapter behind `AgentRuntime`).
- **FR-003**: The system MUST route model calls through an OpenAI-compatible gateway (LiteLLM) to a local model.
- **FR-004**: The system MUST persist sessions in a SQLite store **namespaced per silo** (`local`).
- **FR-005**: The system MUST guarantee inter-silo isolation (no cross-reads) — verified by test.
- **FR-006**: The system MUST capture every step (LlmRequest/LlmResponse) in an append-only, hash-chained Ledger that is inspectable.
- **FR-007**: The system MUST resolve secrets **just-in-time** via `SecretProvider` (OS keyring), without ever persisting/displaying them, with `redact` applied before logging.
- **FR-008**: In Local mode, the system MUST NOT reach the network (local providers only; offline secrets).

### Traceability crosswalk (this feature's FRs → PRD / architecture)
| This spec | PRD | Architecture AD |
|---|---|---|
| FR-001 (headless core + CLI) | FR-1 | AD-1 |
| FR-002 (agentic loop via Goose) | FR-1 | AD-3 (loop ownership per ADR 0002 D1) |
| FR-003 (Gateway/provider) | FR-3 | AD-3, AD-4 |
| FR-004/005 (silo persistence + isolation) | FR-6 | AD-2 |
| FR-006 (Ledger) | FR-10 | AD-5 (schema per ADR 0002 D2) |
| FR-007 (secrets JIT) | FR-11 | AD-4, AD-5 |
| FR-008 (Local egress OFF) | FR-3 | AD-4 (boundary per ADR 0002 D4) |

### Key Entities
- **Session**: an ordered sequence of `Message` within a silo (id `s%06d`).
- **Message**: `{role, parts: [Content]}`; typed Content (text in Phase 0).
- **Step (Ledger)**: `{id(hash), parent, seq, kind, payload}`, chained.
- **SecretRef / SecretValue**: reference (`keychain://…`) → redacted ephemeral value.

## Success Criteria *(mandatory)*

### Measurable Outcomes
- **SC-001**: The US1 scenario succeeds from the CLI, with a file created + session reloaded.
- **SC-002**: The isolation test (US2) passes in CI on Windows, macOS, and Linux.
- **SC-003**: `heddle ledger log` shows both model in AND out (US3), not just the result.
- **SC-004**: `heddle gateway-health` works offline without exposing the key.

## Assumptions
- Ollama is the default local model (cross-platform); LiteLLM runs locally (`:4000`).
- The runtime/worker path is selected by ADR-0003 spikes. Batch CLI subprocess execution is explicitly disallowed for the governed core loop because it hides turn-level model/tool events.
- No cloud secrets in Phase 0; the only managed secret is the Gateway key (OS keyring).
- v1 is text-only; multimodal is out of scope (v2+).
