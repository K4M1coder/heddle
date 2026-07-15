---
stepsCompleted: []
inputDocuments: ['_bmad-output/planning-artifacts/PRD.md', '_bmad-output/planning-artifacts/architecture.md']
---

# Skein - Epic Breakdown

## Overview
Décomposition en epics/stories, dérivée du PRD et de l'architecture. **Cette itération détaille l'Epic 1 (Phase 0 — squelette vertical)** ; les epics 2+ (axes v1, puis v2→v8) seront détaillés à leur tour, chacun avec son propre spine hérité.

## Requirements Inventory
### Functional Requirements (couverts par l'Epic 1)
- FR-1 (boucle agentique headless), FR-3 (sélection provider via Gateway), FR-6 (silo Local + isolation), FR-10 (Ledger), FR-11 (fondation SecretProvider).
### NonFunctional Requirements
- TDD, cross-platform (CI tri-OS), observabilité (tracing), egress OFF en Local, isolation testée.

### FR Coverage Map (Epic 1)
FR-1 → Stories 1.6/1.7 · FR-3 → Story 1.4 · FR-6 → Story 1.3 · FR-10 → Story 1.8 · FR-11 → Story 1.9.

## Epic List
- **Epic 1 — Phase 0 : Squelette vertical (Walking Skeleton)** *(détaillé ci-dessous)*
- Epic 2 — v1/1a Assistant de code agentique (fs/git/shell, TDD, subagents)
- Epic 3 — v1/1b Multi-provider + inférence locale
- Epic 4 — v1/1c Connecteurs Atlassian + M365
- Epic 5 — v1/1d Frameworks BMAD/Spec-Kit/powerskills
- **Epic 6 — v1/1e Moteur de workflow natif (event-sourcé Ledger) + TaskTracker (local/Vikunja/Jira) + hiérarchie & résolution de config** → feature Spec-Kit `specs/002-workflow-engine/`
- Epic 7 — v1 Modes/silos (Serveur/Remote + authz équipe) & UI Chat/Code
- Epics 7+ — v2→v8 (perception, cowork, génération, vidéo, omni, voix, traduction) & piste entreprise

## Epic 1: Phase 0 — Squelette vertical
**Goal :** prouver la tranche verticale complète `CLI → cœur headless → Goose (headless) → Gateway LiteLLM → modèle`, avec persistance silo Local, Ledger et fondation secrets. Livrable testable seul. Détail d'implémentation TDD : `specs/001-phase0-walking-skeleton/tasks.md`.

### Story 1.0: Spike d'intégration Goose (ADR)
As a mainteneur, I want trancher l'intégration Goose sur des faits, So that les tâches d'implémentation soient concrètes.
**Acceptance Criteria:**
**Given** Goose installé **When** on teste `goose run` headless **Then** un ADR (`docs/superpowers/adr/0001`) fixe la voie (CLI subprocess) et les flags exacts.

### Story 1.1: Scaffolding workspace + CI tri-OS
As a dev, I want un workspace Cargo + CI, So that le code compile et est vérifié sur Windows/macOS/Linux.
**Given** le dépôt **When** la CI s'exécute **Then** `fmt`/`clippy -D warnings`/`test` passent sur les 3 OS.

### Story 1.2: Types de domaine (Content/Message/Event)
As a dev, I want des types typés sérialisables, So that le pipeline transporte du contenu structuré.
**Given** un `Message` **When** sérialisé/désérialisé **Then** il round-trip sans perte.

### Story 1.3: SiloStore SQLite (isolation)
As a utilisateur, I want mes sessions persistées et isolées par silo, So that aucune donnée ne fuit entre modes.
**Given** une écriture dans le silo `local` **When** on ouvre un autre namespace **Then** rien n'est visible (test d'isolation vert). Realizes FR-6.

### Story 1.4: GatewayClient (OpenAI-compat) + config LiteLLM
As a utilisateur, I want appeler un modèle via une passerelle unique, So that je peux basculer cloud↔local.
**Given** un endpoint OpenAI-compat **When** `complete()` est appelé **Then** le contenu est extrait ; `health()` reflète l'état. Realizes FR-3.

### Story 1.5: GooseRuntime (adaptateur CLI headless)
As a dev, I want encapsuler Goose derrière `AgentRuntime`, So that le cœur ne dépende pas de Goose.
**Given** un binaire goose (stub) **When** `run()` s'exécute **Then** stdout→`Event::Token`, fin→`Event::Done`.

### Story 1.6: ChatService (orchestration + persistance)
As a utilisateur, I want une conversation persistée, So that je peux la recharger.
**Given** un prompt **When** `chat()` s'exécute **Then** messages user+assistant persistés dans le silo, rechargeables. Realizes FR-1.

### Story 1.7: CLI de référence (chat, session)
As a utilisateur, I want tout piloter au terminal, So that l'outil soit scriptable et testable.
**Given** `skein chat -t ...` **When** exécuté **Then** sortie affichée + `session show` recharge user+assistant. Realizes FR-1/AD-1.

### Story 1.8: Ledger event-sourced (capture & inspection)
As a utilisateur, I want inspecter tout ce qui est envoyé/reçu, So that je garde transparence et réversibilité.
**Given** un chat **When** `skein ledger log` **Then** LlmRequest ET LlmResponse apparaissent, chaînés par hachage, isolés par silo. Realizes FR-10.

### Story 1.9: Fondation SecretProvider (JIT)
As a utilisateur, I want résoudre les secrets juste-à-temps, So that aucun secret ne soit en clair ni journalisé.
**Given** une clé stockée (trousseau OS) **When** `skein gateway-health` la résout **Then** la clé n'apparaît jamais en clair ; `redact` la masque. Realizes FR-11.

### Story 1.10: Vérification du critère de sortie (smoke test)
As a PM, I want valider Phase 0 de bout en bout, So that l'architecture est prouvée.
**Given** Ollama+LiteLLM+Goose configurés **When** `skein chat` crée un fichier **Then** fichier créé, session persistée/rechargée, ledger inspectable, egress OFF confirmé.
