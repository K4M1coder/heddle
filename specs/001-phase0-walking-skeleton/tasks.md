# Tasks: Phase 0 — Squelette vertical

**Spec**: `specs/001-phase0-walking-skeleton/spec.md` | **Plan**: `specs/001-phase0-walking-skeleton/plan.md`

> Détail TDD exhaustif (code complet, commandes, tests par étape) : `docs/superpowers/plans/2026-07-15-skein-phase0-walking-skeleton.md`. Ce fichier est l'**index Spec-Kit** ; chaque tâche y renvoie. `[P]` = parallélisable (fichiers indépendants). TDD obligatoire par tâche.

## Phase Setup & Discovery
- [ ] **T000** (Story 1.0) Spike Goose → ADR `docs/superpowers/adr/0001-goose-integration.md` : voie d'intégration (CLI subprocess) + flags headless confirmés. *Bloquant pour T005.*
- [ ] **T001** (Story 1.1) Scaffolding workspace Cargo (`skein-core`, `skein-cli`) + `rust-toolchain.toml` + CI **matrice Windows/macOS/Linux** (`fmt`/`clippy -D warnings`/`test`).

## Cœur (domaine + ports) — TDD
- [ ] **T002** (Story 1.2) Types `Content`/`Message`/`Role`/`Event` + `SkeinError` (serde, round-trip testé).
- [ ] **T003** [P] (Story 1.3) `SiloStore` SQLite namespacé — `open/create_session/append/load/list_sessions` + **test d'isolation inter-namespace** (FR-005).
- [ ] **T004** [P] (Story 1.4) `GatewayClient` OpenAI-compat (`health`, `complete`) testé via **wiremock** + `config/litellm.config.yaml` (Ollama) (FR-003).
- [ ] **T005** [P] (Story 1.5) `AgentRuntime` + `GooseRuntime` (adaptateur CLI headless) testé via **stub binaire** (FR-002). *Dépend de T000.*

## Orchestration & surfaces — TDD
- [ ] **T006** (Story 1.6) `ChatService` : orchestre run + persistance user/assistant dans le silo (FR-001). *Dépend de T002, T003, T005.*
- [ ] **T007** (Story 1.7) CLI `skein chat` / `session list|show` + test E2E `assert_cmd` (AD-1). *Dépend de T006.*

## Traçabilité & secrets — TDD
- [ ] **T008** (Story 1.8) `LedgerStore` append-only chaîné SHA-256 (`append/log/show`) + capture prompt/réponse + `skein ledger log|show` ; test isolation ledger (FR-006). *Dépend de T003, T007.*
- [ ] **T009** (Story 1.9) `SecretProvider` + `OsKeychain` + `redact` + résolution JIT clé Gateway (`skein secret-set`, `gateway-health`) (FR-007, FR-008). *Dépend de T004.*

## Vérification
- [ ] **T010** (Story 1.10) Smoke test réel (Ollama+LiteLLM+Goose) : critère de sortie (fichier créé, session rechargée, ledger inspectable, secret résolu sans exposition, egress OFF hors-ligne) → `docs/superpowers/plans/phase0-smoke-test.md`. *Dépend de tout.*

## Dépendances (résumé)
```
T000 ─┐
T001  ├─ T002 ─┬─ T006 ─ T007 ─┬─ T008
T000 ─┴─ T005 ─┘               │
      T003 ───────────────────┴─ (T008)
      T004 ─────────────────────── T009
tout ─────────────────────────────── T010
```

## Parallélisation possible
Après T001+T002 : **T003, T004, T005 en parallèle** (`[P]`, fichiers disjoints). T008 et T009 peuvent avancer en parallèle une fois leurs deps satisfaites.
