# Feature Specification: Phase 0 — Squelette vertical (Walking Skeleton)

**Feature Branch**: `001-phase0-walking-skeleton`

**Created**: 2026-07-15

**Status**: Draft

**Input**: Dérivé de `_bmad-output/planning-artifacts/epics.md` (Epic 1) et du design `docs/superpowers/specs/2026-07-15-skein-design.md` (§8 Phase 0).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Conversation persistée qui agit sur des fichiers (Priority: P1)
En tant qu'utilisateur, je lance une conversation depuis le terminal ; l'agent lit/écrit un fichier, et ma session est persistée puis rechargeable.

**Why this priority**: c'est le critère de sortie de la Phase 0 — il prouve la tranche verticale complète (CLI → cœur → Goose → Gateway → modèle → silo).

**Independent Test**: `skein chat -t "..."` puis `skein session show <id>` recharge user+assistant ; un fichier attendu existe.

**Acceptance Scenarios**:
1. **Given** un modèle local configuré, **When** j'exécute `skein chat -t "crée hello.txt contenant skein"`, **Then** `hello.txt` est créé et la session `s000001` est persistée.
2. **Given** une session existante, **When** j'exécute `skein session show s000001`, **Then** les messages user et assistant sont affichés.

### User Story 2 - Isolation stricte du silo (Priority: P1)
En tant qu'utilisateur, mes données de mode Local ne sont visibles dans aucun autre silo.

**Why this priority**: invariant de sécurité fondateur (constitution II) ; doit être vrai dès le squelette.

**Independent Test**: test automatisé écrivant dans le silo `local` et prouvant l'invisibilité dans un autre namespace.

**Acceptance Scenarios**:
1. **Given** une écriture dans `local`, **When** j'ouvre le namespace `remote`, **Then** aucune session/message n'est visible et `load` échoue.

### User Story 3 - Transparence & secrets (Priority: P2)
En tant qu'utilisateur, je vois tout ce qui est envoyé/reçu du modèle (Ledger) et aucun secret n'apparaît en clair.

**Why this priority**: transparence/réversibilité (constitution V) et secrets JIT (VI) — fondations à ancrer tôt.

**Independent Test**: `skein ledger log <session>` liste LlmRequest+LlmResponse ; `skein gateway-health` résout une clé du trousseau sans jamais l'afficher.

**Acceptance Scenarios**:
1. **Given** un chat effectué, **When** `skein ledger log s000001`, **Then** LlmRequest ET LlmResponse apparaissent (chaînés par hachage).
2. **Given** une clé stockée dans le trousseau, **When** `skein gateway-health`, **Then** la santé est vérifiée sans exposer la clé.

### Edge Cases
- Hors-ligne : le mode Local (egress OFF) fonctionne avec un modèle local ; la résolution de secret via trousseau OS marche sans réseau.
- Binaire Goose absent/échec : `Event::Error` propre, code de sortie non nul géré.
- Session inexistante : `session show` retourne une erreur `NotFound` explicite.

## Requirements *(mandatory)*

### Functional Requirements
- **FR-001**: Le système DOIT exposer un cœur headless piloté par une CLI (`skein chat`, `session list|show`, `ledger log|show`, `secret-set`, `gateway-health`).
- **FR-002**: Le système DOIT exécuter une boucle agentique via Goose en mode headless (adaptateur derrière `AgentRuntime`).
- **FR-003**: Le système DOIT router les appels modèle via une passerelle OpenAI-compatible (LiteLLM) vers un modèle local.
- **FR-004**: Le système DOIT persister les sessions dans un store SQLite **namespacé par silo** (`local`).
- **FR-005**: Le système DOIT garantir l'isolation inter-silo (aucune lecture croisée) — vérifiée par test.
- **FR-006**: Le système DOIT capturer chaque étape (LlmRequest/LlmResponse) dans un Ledger append-only chaîné par hachage, inspectable.
- **FR-007**: Le système DOIT résoudre les secrets **juste-à-temps** via `SecretProvider` (trousseau OS), sans jamais les persister/afficher, avec `redact` avant journalisation.
- **FR-008**: En mode Local, le système NE DOIT PAS sortir sur le réseau (providers locaux uniquement ; secrets offline).

### Key Entities
- **Session**: suite ordonnée de `Message` dans un silo (id `s%06d`).
- **Message**: `{role, parts: [Content]}` ; Content typé (text en Phase 0).
- **Step (Ledger)**: `{id(hash), parent, seq, kind, payload}` chaîné.
- **SecretRef / SecretValue**: référence (`keychain://…`) → valeur éphémère rédigée.

## Success Criteria *(mandatory)*

### Measurable Outcomes
- **SC-001**: Le scénario US1 réussit depuis la CLI, avec fichier créé + session rechargée.
- **SC-002**: Le test d'isolation (US2) passe en CI sur Windows, macOS et Linux.
- **SC-003**: `skein ledger log` montre in ET out modèle (US3), pas seulement le résultat.
- **SC-004**: `skein gateway-health` fonctionne hors-ligne sans exposer la clé.

## Assumptions
- Ollama est le modèle local par défaut (cross-platform) ; LiteLLM tourne en local (`:4000`).
- Goose est intégré via sa CLI headless (dépendance upstream ; cf. ADR 0001).
- Aucun secret cloud en Phase 0 ; le seul secret géré est la clé de la Gateway (trousseau OS).
- La v1 est texte ; le multimodal est hors périmètre (v2+).
