# Implementation Plan: Phase 0 — Squelette vertical

**Branch**: `001-phase0-walking-skeleton` | **Date**: 2026-07-15 | **Spec**: `specs/001-phase0-walking-skeleton/spec.md`

## Summary
Prouver la tranche verticale de Skein (CLI → cœur headless → Goose headless → Gateway LiteLLM → modèle local) avec persistance silo Local, Ledger event-sourced et fondation SecretProvider. Approche : anti-corruption layer (types & ports Skein), Goose intégré via CLI subprocess (ADR 0001), tests TDD sans réseau (stubs + wiremock + provider mock), smoke test manuel pour la boucle LLM réelle.

## Technical Context
**Language/Version**: Rust 1.79 (MSRV)
**Primary Dependencies**: tokio, rusqlite (bundled), reqwest, serde/serde_json, clap, thiserror/anyhow, tracing, sha2, keyring, zeroize ; Goose (binaire externe), LiteLLM (proxy externe)
**Storage**: SQLite (fichier local), namespacé par silo
**Testing**: cargo test ; wiremock (HTTP), assert_cmd/predicates (CLI E2E), tempfile ; stub `goose` binaire ; provider mock
**Target Platform**: Windows + macOS + Linux (cross-platform de premier ordre)
**Project Type**: desktop-app / CLI (workspace Cargo multi-crates)
**Performance Goals**: N/A en Phase 0 (correction fonctionnelle d'abord)
**Constraints**: offline-capable (mode Local, egress OFF) ; secrets par référence ; isolation par silo
**Scale/Scope**: squelette — 1 silo (local), 1 provider (local), 1 connecteur (fs via Goose)

## Constitution Check
*GATE : doit passer avant implémentation.*
- **I. Cœur headless / CLI foi / UI surcouche** : ✅ CLI = seule surface en Phase 0 ; pas d'UI.
- **II. Local-first / isolation silo** : ✅ mode Local uniquement ; test d'isolation (Story 1.3).
- **III. Test-First** : ✅ chaque tâche suit red→green→refactor.
- **IV. Couplage inversé** : ✅ Goose derrière `AgentRuntime`, modèle derrière `ModelGateway`, secrets derrière `SecretProvider`.
- **V. Traçabilité (event sourcing)** : ✅ Ledger dès Phase 0 (Story 1.8).
- **VI. Sécurité / secrets par référence** : ✅ SecretProvider JIT + redact (Story 1.9).
- **VII. Neutralité / YAGNI** : ✅ Goose/LiteLLM réutilisés ; back-ends secrets avancés différés.
- **Cross-platform** : ✅ CI matrice tri-OS.
→ **Aucune violation.**

## Project Structure

### Documentation (this feature)
```text
specs/001-phase0-walking-skeleton/
├── spec.md      # présent
├── plan.md      # ce fichier
└── tasks.md     # découpage exécutable
```
Le plan TDD **bite-sized exhaustif** (code complet par étape) vit dans `docs/superpowers/plans/2026-07-15-skein-phase0-walking-skeleton.md` et fait foi pour l'exécution ; `tasks.md` en est l'index Spec-Kit.

### Source Code (repository root)
```text
crates/
  skein-core/
    src/{lib,content,event,error,silo,gateway,runtime,session,ledger,secrets}.rs
  skein-cli/
    src/main.rs
    tests/cli.rs
config/litellm.config.yaml
.github/workflows/ci.yml
```
**Structure Decision** : workspace Cargo à deux crates (cœur + CLI). Sidecar/UI/connecteurs additionnels arrivent aux phases suivantes.

## Complexity Tracking
*Aucune violation de constitution → table vide.*

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| — | — | — |
