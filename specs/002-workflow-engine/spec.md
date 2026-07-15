# Feature Specification: Moteur de workflow natif + TaskTracker + hiérarchie

**Feature Branch**: `002-workflow-engine`

**Created**: 2026-07-15

**Status**: Draft

**Input**: Epic 6 (`_bmad-output/planning-artifacts/epics.md`) ; design §4.12, §4.13, §5.5.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Séquencer une chaîne multi-agentique reprenable (Priority: P1)
En tant qu'utilisateur, je définis un workflow qui enchaîne plusieurs agents/outils sur la chaîne SDLC (ex. lire spec → coder → tester → packager) ; s'il est interrompu, il **reprend** là où il s'était arrêté.

**Why this priority**: c'est la capacité demandée — séquencement multi-agentique natif via le harness ; la reprise prouve la synchro Ledger.

**Independent Test**: lancer un workflow à ≥3 nœuds, l'interrompre, le reprendre ; vérifier qu'aucune étape déjà journalisée n'est ré-exécutée.

**Acceptance Scenarios**:
1. **Given** un workflow à nœuds séquentiels, **When** je l'exécute, **Then** chaque étape produit un `Step` dans le Ledger et le résultat final est atteint.
2. **Given** un workflow interrompu après le nœud 2, **When** je `resume`, **Then** l'exécution reprend au nœud 3 (idempotence des étapes journalisées).
3. **Given** un nœud `Approval`, **When** l'exécution l'atteint, **Then** elle attend une validation humaine avant de continuer.

### User Story 2 - Choisir le tracker de tâches par la hiérarchie (Priority: P1)
En tant que chef de projet, je fixe le TaskTracker (Vikunja local ou Jira cloud) au niveau silo/projet ; les niveaux inférieurs héritent, et un verrou supérieur s'impose.

**Independent Test**: fixer Jira au silo → un projet enfant l'utilise ; ne rien fixer → un projet peut choisir Vikunja.

**Acceptance Scenarios**:
1. **Given** TaskTracker=Jira fixé au silo, **When** une conversation d'un projet enfant crée une tâche, **Then** elle est créée dans Jira (le réglage silo verrouille).
2. **Given** aucun réglage au-dessus du projet, **When** le projet choisit Vikunja, **Then** ses conversations utilisent Vikunja.
3. **Given** mode Local, **When** on résout la config, **Then** la hiérarchie s'applique sans échelon Équipe.

### User Story 3 - Progression reflétée dans le tracker (Priority: P2)
En tant qu'utilisateur, l'avancement d'un workflow crée/met à jour des tâches dans le tracker actif.

**Acceptance Scenarios**:
1. **Given** un workflow en cours, **When** un nœud se termine, **Then** la tâche correspondante passe au statut adéquat dans le TaskTracker résolu.

### Edge Cases
- Reprise après crash : l'état est reconstruit depuis le Ledger (pas de double effet sur les étapes idempotentes ; les effets externes non-idempotents sont marqués et non rejoués sans confirmation).
- Back-end tracker indisponible (Jira hors-ligne en mode Local) : bascule/erreur explicite ; le tracker local reste disponible.
- Conflit de verrou : un niveau inférieur tente de surcharger un réglage verrouillé plus haut → refus explicite.

## Requirements *(mandatory)*

### Functional Requirements
- **FR-013**: Le système DOIT exécuter des workflows (nœuds agent/tool/subagent/approbation/condition/parallèle/boucle) via un `WorkflowEngine`.
- **FR-013a**: Chaque étape de workflow DOIT être journalisée comme `Step` du Ledger ; un workflow DOIT être **reprenable** depuis le dernier Step.
- **FR-013b**: Les recipes Goose et les flux BMAD/Spec-Kit DOIVENT être exécutables comme workflows.
- **FR-014**: Le système DOIT fournir un `TaskTracker` pluggable : local (silo), Vikunja (embarqué), Jira (via MCP).
- **FR-015**: La config (dont le TaskTracker) DOIT être résolue selon la hiérarchie Silo▸Équipe▸Projet▸Conversation, un réglage fixé à un niveau **verrouillant** les niveaux inférieurs.
- **FR-016**: Les workflows DOIVENT pouvoir orchestrer la chaîne SDLC via connecteurs MCP (conception, dev/git, tests, packaging, déploiement) et le TaskTracker.

### Key Entities
- **Workflow**: `{name, params, graph: [Node]}` ; **Node**: agent/tool/subagent/approval/cond/parallel/loop.
- **WorkflowRun**: instance exécutée, adressée par `RunId`, dérivée du Ledger.
- **Task**: unité de suivi (`{id, title, status, links}`) dans un TaskTracker.
- **ConfigScope**: niveau de résolution (Silo/Équipe/Projet/Conversation) + drapeau `locked`.

## Success Criteria *(mandatory)*

### Measurable Outcomes
- **SC-001**: un workflow ≥3 nœuds interrompu puis repris ne ré-exécute aucune étape journalisée (US1).
- **SC-002**: la résolution hiérarchique du TaskTracker respecte « le plus haut verrouille » (US2), testée sur les 4 niveaux et en mode Local (3 niveaux).
- **SC-003**: la progression d'un workflow est visible dans le tracker résolu (US3).
- **SC-004**: un workflow orchestre au moins un enchaînement SDLC réel (ex. code → test → PR) via connecteurs (US1/FR-016).

## Assumptions
- Le moteur natif (event-sourcé Ledger) est le défaut ; Temporal/Windmill sont des back-ends optionnels derrière `WorkflowEngine`.
- Vikunja est le tracker OSS embarqué par défaut ; Jira via le connecteur MCP existant.
- La hiérarchie vit dans un silo (jamais inter-silo) ; l'appartenance équipe reste la frontière d'autorisation (§7.10).
- En mode Local, l'échelon Équipe n'existe pas.
