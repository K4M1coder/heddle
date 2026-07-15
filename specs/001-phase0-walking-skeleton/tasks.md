# Tasks: Phase 0 — Vertical Skeleton

> **Status: BLOCKED / REQUIRES REGENERATION.** Do not execute this task list. ADR-0003 requires evidence spikes and invalidates the former Goose CLI subprocess tasks. Regenerate after architecture readiness.

**Spec**: `specs/001-phase0-walking-skeleton/spec.md` | **Plan**: `specs/001-phase0-walking-skeleton/plan.md`

> Exhaustive TDD detail (complete code, commands, tests per step): `docs/superpowers/plans/2026-07-15-skein-phase0-walking-skeleton.md`. This file is the **Spec-Kit index**; each task links back to it. `[P]` = parallelizable (independent files). TDD is mandatory per task.

## Phase Setup & Discovery
- [ ] **T000** (Story 1.0) Runtime composition spike → evidence bundle + ADR-0003 decision: native Rust loop vs embedded/goosed Goose vs OpenCode/Cline worker surfaces. Must prove turn-level I/O, tool mediation, correlation and engine-enforced termination. *Blocking for all runtime implementation tasks.*
- [ ] **T001** (Story 1.1) Cargo workspace scaffolding (`skein-core`, `skein-cli`) + `rust-toolchain.toml` + CI **Windows/macOS/Linux matrix** (`fmt`/`clippy -D warnings`/`test`).

## Core (domain + ports) — TDD
- [ ] **T002** (Story 1.2) `Content`/`Message`/`Role`/`Event` types + `SkeinError` (serde, round-trip tested).
- [ ] **T003** [P] (Story 1.3) namespaced SQLite `SiloStore` — `open/create_session/append/load/list_sessions` + **inter-namespace isolation test** (FR-005).
- [ ] **T004** [P] (Story 1.4) OpenAI-compatible `GatewayClient` (`health`, `complete`) tested via **wiremock** + `config/litellm.config.yaml` (Ollama) (FR-003).
- [ ] **T005** [P] (Story 1.5) `AgentRuntime` + `GooseRuntime` (headless CLI adapter) tested via **binary stub** (FR-002). *Depends on T000.*

## Orchestration & surfaces — TDD
- [ ] **T006** (Story 1.6) `ChatService`: orchestrates run + user/assistant persistence in the silo (FR-001). *Depends on T002, T003, T005.*
- [ ] **T007** (Story 1.7) CLI `skein chat` / `session list|show` + `assert_cmd` E2E test (AD-1). *Depends on T006.*

## Traceability & secrets — TDD
- [ ] **T008** (Story 1.8) append-only SHA-256-chained `LedgerStore` (`append/log/show`) + prompt/response capture + `skein ledger log|show`; ledger isolation test (FR-006). *Depends on T003, T007.*
- [ ] **T009** (Story 1.9) `SecretProvider` + `OsKeychain` + `redact` + JIT resolution of the Gateway key (`skein secret-set`, `gateway-health`) (FR-007, FR-008). *Depends on T004.*

## Verification
- [ ] **T010** (Story 1.10) Real smoke test (Ollama+LiteLLM+Goose): exit criterion (file created, session reloaded, ledger inspectable, secret resolved without exposure, egress OFF offline) → `docs/superpowers/plans/phase0-smoke-test.md`. *Depends on everything.*

## Dependencies (summary)
```
T000 ─┐
T001  ├─ T002 ─┬─ T006 ─ T007 ─┬─ T008
T000 ─┴─ T005 ─┘               │
      T003 ───────────────────┴─ (T008)
      T004 ─────────────────────── T009
all ──────────────────────────────── T010
```

## Possible parallelization
After T001+T002: **T003, T004, T005 in parallel** (`[P]`, disjoint files). T008 and T009 can proceed in parallel once their deps are satisfied.
