---
title: Skein
created: 2026-07-15
updated: 2026-07-15
---

# PRD: Skein
*Outil agentique IA local-first, unifiant chat, code et cowork.*

## 0. Document Purpose
Ce PRD s'adresse au PM, aux parties prenantes SodiusWillert et aux workflows aval (architecture, epics/stories). Il définit le **quoi** et le **pourquoi** ; le **comment** vit dans `architecture.md`. Le design exhaustif (référence) est `docs/superpowers/specs/2026-07-15-skein-design.md`. Vocabulaire ancré au §3 Glossaire ; features avec FRs imbriqués ; hypothèses indexées §9.

## 1. Vision
Skein est un **outil agentique IA unique, local-first**, réunissant **chat**, **code** et **cowork** (pilotage PC) derrière un cœur headless doté d'un harness poussé (contexte, tools, skills). Il se branche sur **tous les fournisseurs d'IA** (cloud et locaux), embarque sa propre inférence, intègre nativement les connecteurs métier (Atlassian, M365) via MCP, et maîtrise les méthodes **BMAD / Spec-Kit / powerskills**. Il donne à l'utilisateur une **transparence et une réversibilité totales** (chaque étape est un « commit » inspectable/rejouable) et une **conformité entreprise** (identité, RBAC, audit, RGPD/ISO/SOC2/AI Act/NIS2).

## 2. Target User

### 2.1 Jobs To Be Done
- Développer/assister sur du code de façon agentique, en TDD, avec subagents.
- Piloter Jira/Bitbucket/Confluence et M365 depuis un seul outil.
- Choisir librement le modèle (cloud souverain ou local hors-ligne) selon la sensibilité.
- Automatiser des tâches PC (cowork) et web.
- Garder la maîtrise : voir *tout* ce qui est envoyé aux modèles, pouvoir annuler/rejouer.
- Travailler en équipe avec gouvernance (rôles, config partagée, conformité).

### 2.2 Non-Users (v1)
Utilisateurs cherchant un simple chatbot web sans exécution locale ; usages sans poste de travail réel (le cowork exige un poste).

### 2.3 Key User Journeys
- **UJ-1. L'ingénieur enchaîne spec → code → PR → ticket.** Depuis la CLI ou l'UI, il lit une spec Confluence, génère un plan Spec-Kit, code en TDD, ouvre une PR Bitbucket, crée un ticket Jira — en basculant cloud↔local. **Climax :** la PR et le ticket existent, la session est persistée. **Edge :** hors-ligne, l'outil retombe en mode Local (modèle local).
- **UJ-2. Le chef de projet gouverne l'équipe.** Il édite la couche harness d'équipe, verrouille des réglages de sécurité, attribue des rôles ; les membres héritent de la base et surchargent localement le reste.
- **UJ-3. L'utilisateur audite une décision de l'agent.** Via `skein ledger`, il inspecte le prompt exact envoyé au modèle et la réponse brute, puis rejoue ou annule l'étape.

## 3. Glossary
- **Silo** — partition de données étanche liée à un mode (et une équipe en Remote).
- **Mode** — Local / En ligne-Serveur (leader) / En ligne-Remote (follower).
- **Harness** — configuration du comportement de l'agent (instructions, tools, skills, contexte, politiques), éditable en couches équipe/locale.
- **Ledger** — journal append-only chaîné par hachage (I/O modèles, tools, état), façon git.
- **Connecteur** — serveur MCP exposant un outil/ressource (Jira, M365, fs, git…).
- **Gateway** — passerelle LiteLLM OpenAI-compatible vers 100+ providers.
- **Controller** — abstraction du pilotage d'une surface externe (PC, navigateur).
- **Principal / RBAC / SecretProvider / IdP** — cf. `architecture.md` et design §7.

## 4. Features

### 4.1 Assistant de code agentique
**Description :** lire/éditer des fichiers, exécuter des commandes (sandbox), boucle agentique, subagents, TDD. Realizes UJ-1.
**Functional Requirements:**
#### FR-1 : Boucle agentique headless
L'utilisateur peut lancer une tâche (CLI/API/UI) qui exécute plan→tools→éval jusqu'à complétion. Realizes UJ-1.
- **Consequences (testable) :** une session lit/écrit un fichier et est persistée puis rechargée depuis le silo.
#### FR-2 : Connecteurs fs/git/shell (MCP)
L'agent peut manipuler fichiers, git et shell via connecteurs MCP, actions destructives sous confirmation.

### 4.2 Multi-provider & inférence locale
**Description :** basculer cloud↔local via la Gateway ; serveur d'inférence embarqué (Ollama/llama.cpp ; vLLM optionnel).
#### FR-3 : Sélection de provider
L'utilisateur peut router vers un provider cloud OU local ; en mode Local, seuls les providers locaux sont autorisés (egress OFF).

### 4.3 Connecteurs Atlassian & M365
#### FR-4 : Jira/Bitbucket/Confluence + Outlook/SharePoint/Teams via MCP, utilisables dans les workflows.

### 4.4 Frameworks BMAD / Spec-Kit / powerskills
#### FR-5 : Ces méthodes sont packagées en recipes/skills invocables (`/spec`, `/bmad`, …).

### 4.5 Modes, silos & gouvernance du harness
#### FR-6 : 3 modes auto-détectés, bascule proposée (jamais imposée), fallback local ; silos étanches ; partage cloisonné par équipe en Remote.
#### FR-7 : Harness éditable en couches équipe (chefs, verrouillable) + locale (surcharge sauf verrous).

### 4.6 Identité, RBAC, observabilité, conformité
#### FR-8 : IdP pluggable (local/LDAP/OIDC/Entra/Google) + RBAC 3 portées (globale/silos/intra-silo), deny-by-default.
#### FR-9 : Observabilité OpenTelemetry + audit immuable dès v1.

### 4.7 Traçabilité & réversibilité (Ledger)
#### FR-10 : Chaque étape (I/O modèles exactes, tools, état) est capturée, inspectable, rejouable, réversible (`skein ledger log|show|replay|revert|branch`).

### 4.8 Gestion des secrets
#### FR-11 : SecretProvider pluggable (SOPS+age/1Password/OpenBao/Infisical/OS keychain), **résolution JIT**, référence-pas-valeur, rédaction des journaux, offline-only en mode Local.

### 4.9 Cowork & multimodal (v2+)
#### FR-12 : Pilotage PC + compagnon navigateur (Controller hybride) ; perception (doc/image/audio/grounding) ; génération (image/TTS/Office/vidéo) ; omni (orchestration) ; voix temps réel ; traduction multilingue. Détail : roadmap §6 + design §8.

## 5. Non-Goals (Explicit)
- Pas de réécriture d'un harness from scratch (on adopte Goose).
- Pas de produit serveur séparé (le backend d'équipe = une instance exposée).
- Pas de dépendance à un fournisseur unique (IA, IdP, secrets).
- Pas de simple chatbot web sans exécution locale.

## 6. MVP Scope

### 6.1 In Scope (v1)
Assistant de code agentique · multi-provider + inférence locale · connecteurs Atlassian+M365 · frameworks BMAD/Spec-Kit/powerskills · modes & silos (Local complet, Serveur/Remote de base + authz équipe) · UI Chat+Code · identité locale + RBAC de base · observabilité · Ledger · fondation SecretProvider · compliance-by-design.

### 6.2 Out of Scope for MVP (roadmap)
- **v2** Perception (entrées multimodales) · **v3** Cowork/pilotage · **v4** Génération médias · **v5** Vidéo · **v6** Omni · **v7** Voix temps réel · **v8** Traduction multilingue.
- IdP externes + RBAC avancé + certifications : piste entreprise (parallèle).

## 7. Success Metrics
**Primary**
- **SM-1** : réaliser UJ-1 de bout en bout (spec→PR→ticket) depuis CLI *et* UI *et* API, en basculant cloud↔local. Valide FR-1..FR-5.
- **SM-2** : isolation des silos prouvée par test (écriture invisible hors silo/équipe). Valide FR-6.
**Secondary**
- **SM-3** : toute I/O modèle inspectable via `skein ledger`. Valide FR-10.
**Counter-metrics (do not optimize)**
- **SM-C1** : ne pas gagner en vitesse en contournant confirmations/rédaction/egress — la sécurité prime sur la latence.

## 8. Open Questions
1. Migration future de l'intégration Goose (CLI subprocess → goosed REST → crate) — tranchée par spikes ultérieurs.
2. Format exact de capture token-level via logging LiteLLM (ingestion Gateway→Ledger).
3. Modèle d'identité local-first (paires de clés) → passage OIDC entreprise.

## 9. Assumptions Index
- [ASSUMPTION §2] Le cowork exige un poste réel (pas de client léger pur).
- [ASSUMPTION §4.2] Ollama est le moteur d'inférence local par défaut, cross-platform.
- [ASSUMPTION §6] La v1 est texte ; le multimodal est strictement v2+.

## Compliance & Regulatory *(adapt-in : regulated domain)*
RGPD (minimisation via mode Local, droit à l'effacement admin, résidence, rétention), ISO 27001 & SOC 2 (RBAC, audit, chiffrement, gestion du changement config-as-code), EU AI Act (transparence « contenu IA », supervision humaine, traçabilité Ledger), NIS2 (journalisation/incident, chaîne d'appro. MCP, gouvernance). Le logiciel fournit les contrôles ; la certification est organisationnelle.

## Audit Trail / Decision Provenance *(adapt-in)*
Deux journaux complémentaires : **audit** (qui/quand) et **Ledger** (quoi exactement, in/out). Base transversale de la conformité.
