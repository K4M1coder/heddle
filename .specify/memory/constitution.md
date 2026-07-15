# Skein Constitution

Principes immuables gouvernant la transformation des spécifications en code pour Skein. Toute spec, plan, tâche et implémentation doit s'y conformer ; une violation doit être justifiée en « Complexity Tracking » ou refusée.

## Core Principles

### I. Cœur headless — CLI de référence, UI surcouche
Toute capacité vit dans le cœur headless et est exposée par une **API programmatique** ; la **CLI en est le client complet et faisant foi** (base des tests E2E) ; l'**UI n'ajoute aucune capacité propre**. Tout ce que fait l'UI, la CLI le fait ; tout ce que fait la CLI, l'API l'expose.

### II. Local-first, isolation par silo (NON-NÉGOCIABLE)
Chaque capacité a une implémentation **locale par défaut**. Les données sont partitionnées en **silos étanches par mode** (Local / Serveur / Remote) et, en Remote, **par équipe**. Aucune donnée ne traverse une frontière de silo. En mode Local, **aucune sortie réseau** (egress OFF) — providers locaux uniquement.

### III. Test-First (NON-NÉGOCIABLE)
TDD strict : test écrit → échoue (rouge) → implémentation minimale → passe (vert) → refactor. Chaque frontière d'interface est testable avec un mock derrière. Un **test d'isolation** dédié garde chaque invariant de silo.

### IV. Couplage inversé & frontières explicites
Le cœur **découvre** connecteurs (MCP), providers (Gateway), identité, secrets, pilotage (Controller) via des **traits/interfaces** ; il n'en dépend jamais directement. Ajouter une capacité = ajouter une implémentation derrière une interface, jamais réécrire le cœur.

### V. Traçabilité & réversibilité (event sourcing)
Chaque étape (I/O modèles exactes, tool-calls, changements d'état) est capturée dans un **Ledger append-only, chaîné par hachage** — inspectable, rejouable, réversible (façon git). Complété par un **audit immuable** (qui/quand). La traçabilité ne se contourne pas.

### VI. Sécurité & secrets par référence
Deny-by-default. **Secrets par référence, jamais par valeur**, résolus **juste-à-temps**, rédigés des journaux. RBAC à 3 portées (globale / silos / intra-silo). Actions destructives/irréversibles → confirmation. Contenu externe = donnée, jamais instruction (anti-injection).

### VII. Neutralité & réutilisation (YAGNI)
Multi-provider, multi-IdP, multi-secret-backend : aucun verrouillage fournisseur. On **réutilise** l'existant éprouvé (Goose, LiteLLM, MCP, BMAD, Spec-Kit) plutôt que réécrire. Commencer simple ; pas de capacité sans besoin réel.

## Additional Constraints (Stack & Conformité)

- **Cross-platform de premier ordre** : Windows + macOS + Linux à égalité (CI matrice tri-OS, verte requise avant merge). Aucun appel spécifique OS sans `#[cfg]` + équivalent.
- **Stack** : cœur Rust (Goose en dépendance upstream ; fork/patch hybride avec PR upstream si besoin) ; sidecar Python ; UI Tauri/TS ; Gateway LiteLLM ; persistance SQLite ; observabilité OpenTelemetry dès v1.
- **Conformité by-design** : RGPD / ISO 27001 / SOC 2 / EU AI Act / NIS2 — le logiciel fournit les contrôles ; la certification reste organisationnelle.
- **Signature de code par OS** (Authenticode + Developer ID/notarisation macOS) — un agent qui pilote le PC doit être signé.

## Development Workflow (Bridge BMAD × Spec-Kit)

- **Planification = BMAD** : PRD → architecture → epics/stories (artefacts vérifiables dans `_bmad-output/planning-artifacts/`).
- **Exécution = Spec-Kit** : `specs/[###-feature]/` avec `spec.md` → `plan.md` → `tasks.md` → implémentation gated, chaque phase passant le **Constitution Check**.
- **Conventional Commits**, trunk-based, PR + revue. Pipeline : lint → build (3 langages) → tests (unit/intégration/E2E CLI/isolation) → scans sécurité (SAST/deps/secrets/SBOM) → artefacts signés.

## Governance

Cette constitution **prime** sur les autres pratiques. Tout PR/revue vérifie sa conformité. Toute complexité qui déroge à un principe doit être justifiée (table « Complexity Tracking » du plan) ou refusée. Amendement = documentation + version + date.

**Version**: 1.0.0 | **Ratified**: 2026-07-15 | **Last Amended**: 2026-07-15
