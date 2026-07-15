# Skein — Document de conception (Spec)

- **Nom de code** : Skein *(« a skein of geese » = une volée d'oies, clin d'œil à Goose ; et un écheveau de fils entrelacés = les connecteurs/modèles tissés ensemble)*
- **Date** : 2026-07-15
- **Statut** : Conception validée — en attente de relecture avant plan d'implémentation
- **Auteur** : cthedrez@sodiuswillert.com (avec assistance Claude Code)
- **Méthode** : rédigé façon **Spec-Kit** (Spec → Plan → Tasks → Implement) ; artefacts vérifiables façon **BMAD**.

> ⚠️ Ce document décrit **quoi** construire et **pourquoi**. Le **comment détaillé** (tâches, séquencement) fera l'objet d'un plan d'implémentation séparé, produit après relecture de ce spec.

---

## 1. Vision & objectifs

### 1.1 Problème
Les équipes de SodiusWillert utilisent aujourd'hui des outils IA cloisonnés (chat, assistant de code, automatisations) sans harness unifié, sans intégration native à leur stack (Atlassian, M365), et sans maîtrise du choix des modèles (cloud vs local/souverain).

### 1.2 Vision
Un **outil agentique unique**, local-first, réunissant **chat**, **code** et **cowork** (pilotage PC), doté d'un harness poussé (gestion du contexte, tools, skills), intégrant nativement les connecteurs métier via **MCP**, capable de se brancher sur **tous les fournisseurs d'IA** (cloud et locaux) et embarquant sa propre inférence, avec les méthodes **BMAD / Spec-Kit / powerskills** comme compétences de premier ordre.

### 1.3 Objectifs (v1 / MVP)
1. **Assistant de code agentique** (lire/éditer/exécuter, TDD, subagents).
2. **Multi-provider + inférence locale** (cloud + Ollama/vLLM/llama.cpp/LM Studio).
3. **Connecteurs Atlassian + M365** via MCP, utilisables dans les workflows.
4. **Frameworks BMAD / Spec-Kit / powerskills** intégrés comme skills invocables.

### 1.4 Hors périmètre v1 (versions ultérieures)
Le v1 est **texte**. L'évolution multimodale et collaborative est planifiée en versions v2→v7 + une piste parallèle entreprise — voir la **§8 Roadmap d'évolution**. Résumé :
- **v2** : entrées multimodales (documents/images, audio, grounding visuel) + cowork/pilotage PC + compagnon navigateur + ingestion web.
- **v3** : sorties génératives (image, audio/TTS, fichiers Office).
- **v4** : images animées + vidéo.
- **v5** : modèle omni (in+out unifié).
- **v6** : audio temps réel (voix streaming duplex).
- **v7** : traduction temps réel multilingue (Teams / chat d'équipe).
- **Piste ⟂** : durcissement équipe/entreprise (SSO/OIDC, audit avancé, RAG avancé, vLLM GPU, catalogue de recipes), cadencée par l'adoption d'équipe.

### 1.5 Non-objectifs (principes YAGNI)
- Pas de réécriture d'un harness agentique from scratch (on adopte une base neutre).
- Pas de produit serveur séparé (le « backend d'équipe » est une instance de l'app exposée).
- Pas de dépendance à un fournisseur unique (neutralité multi-provider).

---

## 2. Décisions fondatrices (et justification)

| Décision | Choix retenu | Justification |
|---|---|---|
| **Stratégie** | Bâtir sur une **base open-source neutre** | Évite le verrouillage fournisseur + réutilise un harness éprouvé. |
| **Socle / langage cœur** | **Goose (Rust)** + **sidecar Python** | Goose : Rust, MCP-natif, Apache-2.0, Linux Foundation, multi-provider, Windows OK. Python pour l'écosystème IA. |
| **Passerelle modèles** | **LiteLLM** | Point d'entrée OpenAI-compatible vers 100+ providers cloud **et** locaux ; coût/quotas/guardrails. |
| **Frameworks** | BMAD / Spec-Kit / powerskills packagés en **recipes/skills** | Intégration = packaging + orchestration, pas réécriture. |
| **Pilotage PC** | Interface `Controller` **hybride abstraite** | Back-ends interchangeables (Computer Use API **ou** local enigo/xcap). Pas de verrouillage. |
| **Déploiement** | **Local-first**, backend d'équipe **activable** | Le desktop est autonome ; le mode équipe est une surcouche. |
| **Surfaces** | **Cœur headless → CLI (référence) → UI (surcouche)** | Automatisable, testable ; l'UI n'ajoute aucune capacité propre. |

### 2.1 Sources (état vérifié au 2026-07-15)
- Goose : https://github.com/aaif-goose/goose · https://block-goose.mintlify.app/
- LiteLLM : https://github.com/BerriAI/litellm · https://docs.litellm.ai/docs/providers
- Spec-Kit : https://github.com/github/spec-kit
- BMAD-METHOD : https://docs.bmad-method.org/

---

## 3. Architecture d'ensemble

Principe directeur : **cœur headless**, la CLI est le client complet de référence, l'UI est une surcouche. Toute capacité de l'UI existe dans la CLI ; toute capacité de la CLI est exposée par l'API.

```
        ┌───────────────── CŒUR HEADLESS (Goose, Rust) ─────────────────┐
        │  API programmatique complète (JSON-RPC / HTTP local)           │
        │  Runtime agentique · Contexte · Dispatch tools/skills/providers│
        └───────────────────────────┬───────────────────────────────────┘
                                     │ (même contrat pour tous)
        ┌──────────────┬─────────────┴─────────────┬────────────────────┐
        ▼              ▼                             ▼
   API (socket/    CLI (client COMPLET,        UI Tauri (surcouche,
   HTTP)           référence des tests)         zéro capacité propre)
                                     │
   ┌─────────────────────────────────┼────────────────────────────────┐
   ▼                 ▼               ▼                ▼                 ▼
 Sidecar         Gateway         Connecteurs      Backend +          Controller
 Python          LiteLLM         MCP (Atlassian,  Superviseur        cowork
 (embeddings/    (100+ prov.     M365, fs, git,   de mode            (v2)
 RAG)            cloud+local)    shell, …)        (silos)
                     │
              Inférence : Ollama / vLLM / llama.cpp / LM Studio (+ cloud)
```

**Langages** : Rust (cœur + UI Tauri) · Python (sidecar IA/inférence) · TypeScript (front UI). Polyglotte assumé, chaque langage sur son terrain de force.

---

## 4. Composants & interfaces

Chaque composant : **un rôle**, une **interface explicite**, **testable isolément**. Couplage inversé (le cœur découvre connecteurs/providers/pilotage, il n'en dépend pas).

### 4.1 Surfaces d'accès
- **Cœur headless** : expose l'`Agent` via API locale (JSON-RPC + HTTP optionnel). Surface unique.
- **CLI** (`skein …`) : client complet, faisant foi ; 100% scriptable ; base des tests E2E.
- **API** : même surface, pour automatisation/CI/tiers ; soumise à l'exposition/authz du mode.
- **UI (Tauri)** : n'émet que des commandes CLI/API, affiche des événements. Aucune logique métier.

### 4.2 Cœur agentique (`core/`, Rust — Goose)
```rust
enum Content { Text(..), Image(..), Audio(..), Doc(..), Video(..) }  // abstraction typée (dès v2)
struct Message { role: Role, parts: Vec<Content> }

trait Agent {
  fn run(&self, session: SessionId, input: Message) -> EventStream;
  fn register_extension(&mut self, ext: McpServer);   // = un connecteur
  fn register_skill(&mut self, skill: Recipe);         // = BMAD/Spec-Kit/…
}
```
Dépend de : `ModelGateway`, `Backend`, extensions MCP, moteur de skills.

**Abstraction de contenu typé** (introduite en v2, transversale) : un `Message` porte des `parts` de types `text | image | audio | doc | video`. C'est le **seul ajout cœur** requis par toute la roadmap multimodale ; les modalités concrètes sont ensuite des capacités de providers (Gateway) ou des tools spécialisés — jamais une réécriture de la boucle agentique.

### 4.3 Connecteurs (`connectors/`, serveurs MCP)
Chaque connecteur = un serveur MCP (Jira/Bitbucket/Confluence, Outlook/SharePoint/Teams, `fs`, `git`, `shell`). Ajout par **config**, pas de code cœur. Protocole : MCP (`tools/list`, `tools/call`, `resources/*`).

### 4.4 Moteur de skills / recipes (`skills/`)
Charge BMAD (21+ agents, artefacts), Spec-Kit (Spec→Plan→Tasks→Implement), powerskills/superpowers comme **recipes Goose YAML** invocables (`/spec`, `/bmad`, …).
```
Recipe = { name, description, instructions, required_extensions[], params[], prompt }
```

### 4.5 Passerelle de modèles (`gateway/`, LiteLLM)
Point d'entrée OpenAI-compatible (`POST /v1/chat/completions`) → 100+ providers cloud/locaux ; routage, coût/quotas, load-balancing, guardrails. Goose s'y branche via un provider « openai-compatible ».

### 4.6 Couche d'inférence (`inference/`)
Modèles locaux (Ollama / llama.cpp / vLLM / LM Studio) exposés en endpoints OpenAI-compat, enregistrés dans LiteLLM. « Serveur d'inférence embarqué » = Ollama/llama.cpp packagé (+ vLLM optionnel GPU).

### 4.7 Sidecar Python (`sidecar/`)
Embeddings, RAG/indexation, orchestration d'inférence avancée, éval. Process séparé (gRPC/HTTP local) : `embed()`, `index()`, `search()`. Le stockage d'index est routé par silo/mode.

### 4.8 Backend & silos (`backend/`) + Superviseur de mode
```rust
trait Backend { fn store(&self, mode: Mode, team: Option<TeamId>) -> Silo; }
impl EmbeddedBackend  // SQLite + fichiers locaux, par namespace
impl RemoteBackend    // client du leader, partition d'équipe
struct ModeSupervisor { fn detect()->NetState; fn switch(Mode); fn heartbeat(); }
```

### 4.9 Controller cowork (`controller/`, interface posée en v1, impl. v2)
Abstraction unique du pilotage d'une surface externe (capture + actions), avec **plusieurs canaux interchangeables** :
```rust
trait Controller {
  fn screenshot(&self) -> Frame;
  fn click(&self, target: Point); fn type_text(&self, s: &str);
}
impl ComputerUseController   // API Anthropic (grounding fourni)
impl LocalController         // desktop : enigo (clavier/souris) + xcap (capture)
impl BrowserController       // compagnon navigateur (extension Chrome/Edge, type "Claude for Chrome")
```
Les trois — captures en entrée (v2c), pilotage PC, compagnon navigateur — réutilisent **la même brique de grounding visuel**. Ce sont trois implémentations d'un même trait, pas trois développements distincts.

### 4.10 Modalités génératives & flux temps réel (v3+)
- **Génération** (v3/v4) : tools/connecteurs spécialisés — image (modèle via Gateway), **TTS** (audio), **fichiers Office** (docx/pptx/xlsx via bibliothèques matures), **vidéo** (v4). Chaque sortie est un `Content` typé produit par un tool ; le cœur n'en dépend pas.
- **Canal duplex streaming** (v6, *nouveauté du modèle d'exécution*) : l'audio temps réel exige un flux **bidirectionnel continu** (in et out simultanés), distinct de la boucle requête→réponse. Introduit une interface `RealtimeSession` (WebRTC/streaming ou API omni-realtime) — c'est le **seul jalon qui modifie le modèle d'exécution du cœur** (cf. risques §10).
- **Traduction d'équipe** (v7) : composition STT→traduire→TTS (voix) + traduction texte, **par participant** selon un profil « langue maternelle » porté par le membre dans la **partition d'équipe** (§5), via le connecteur Teams/chat.

---

## 5. Connectivité, modes & isolation

### 5.1 Backend embarqué, toujours fonctionnel
Chaque instance embarque un backend toujours actif. Ce qui est configurable = son **exposition réseau**. Pas de produit serveur séparé.

### 5.2 Trois modes (auto-détectés, bascule proposée)

| Mode | Backend local | Exposition | Rôle |
|---|---|---|---|
| **Local** | Actif | OFF | Autonome, hors réseau |
| **En ligne — Serveur** | Actif + partagé | **ON** | *Leader* (sert d'autres instances) |
| **En ligne — Remote** | En **veille** | client | *Follower* (utilise un leader distant) |

- **Superviseur de mode** : détecte connectivité + présence de leader ; propose la bascule (jamais imposée) ; **fallback local automatique** si hors-ligne ou perte du leader.
- Modèle **leader/follower auto-élu** : à tout instant, exactement **un** backend actif par instance (pas de split-brain).

### 5.3 Isolation des données
- **Inter-modes** : silos **étanches** (namespace par mode : DB/schéma + répertoire + trousseau séparés). Aucune session ne traverse la frontière. Changer de mode = changer de silo, jamais fusionner.
- **Intra-Remote** : partage **cloisonné par équipe** — un follower n'accède qu'à `team:<sien>` sur le leader. Deux équipes sur un même leader restent invisibles l'une à l'autre.
- **Invariants testés** : écrire dans un silo → prouver l'invisibilité dans les autres silos et les autres équipes.

---

## 6. Flux de données (nominal)

```
Entrée (UI/CLI/API)
 └─1─ Superviseur : résout le silo (local / server / remote+team)
 └─2─ Contexte : charge mémoire/historique DU SILO
 └─3─ Boucle agentique :
        a) LLM → Gateway LiteLLM → provider (cloud/local)
        b) demande de tool → dispatch MCP → connecteur → résultat
        c) évalue, répète
 └─4─ Effets de bord via connecteurs (soumis aux règles §7)
 └─5─ Persistance DANS LE SILO courant uniquement
 └──► Stream d'événements typé (token / tool-call / diff / erreur) → CLI & UI identiques
```

- L'isolation est appliquée aux **bornes** (étapes 1 et 5).
- **Politique par défaut** : mode Local ⇒ **aucune sortie réseau** ⇒ providers **locaux uniquement**.
- Sidecar RAG : `embed(query)` (modèle local par défaut) → `search()` dans l'index du silo → passages réinjectés à l'étape 2.

---

## 7. Sécurité

1. **Authz (Remote/leader)** : identité + appartenance équipe vérifiées à l'attache (défaut : paires de clés ; SSO/OIDC en piste entreprise). Accès **deny-by-default** à la partition d'équipe. **TLS obligatoire** dès exposition.
2. **Secrets** : coffre OS (Windows Credential Manager/DPAPI ; Keychain/secret-service). **Jamais en clair**. **Un trousseau par silo**. L'agent n'entre jamais de credentials dans un formulaire.
3. **Egress par mode** : Local = aucune sortie (local uniquement) ; Serveur/Remote = sortie selon **politique explicite** (allow-list d'endpoints).
4. **Garde-fous d'exécution** : actions destructives/irréversibles → **confirmation** (ou allow-list en CI) ; **bac à sable** pour code/shell ; journal d'audit des tool-calls (quoi/quand/quel silo).
5. **Défense injection de prompt** : tout contenu rapporté par un tool (e-mail, page, issue, capture) est **donnée, pas instruction**. Consignes trouvées dans du contenu externe → signalées, jamais exécutées.
6. **Chaîne d'appro. (MCP & recipes)** : registre de confiance, épinglage de versions, revue avant activation. Pas de chargement silencieux.
7. **Sûreté cowork / navigateur / voix (v2+)** : confirmations avant actions irréversibles ; pas de saisie de credentials ; captures d'écran, contenu web et audio confinés au silo + politique egress ; le compagnon navigateur n'agit jamais sur instruction trouvée *dans* une page (frontière §7.5).

---

## 8. Phasage & jalons (critères de sortie vérifiables)

### Phase 0 — Squelette qui marche (tranche verticale)
Cœur headless + contrat d'API/événements figés ; CLI minimale ; 1 provider via LiteLLM ; connecteur `filesystem` ; persistance silo Local.
**Sortie** : au terminal, conversation qui lit/écrit un fichier, persistée & rechargée.

### Phase 1 — MVP (4 axes)
- **1a** Code agentique (`fs`/`git`/`shell` sandbox, édition+diff, TDD, subagents).
- **1b** Multi-provider + inférence locale (LiteLLM + Ollama/llama.cpp embarqués, bascule, egress Local).
- **1c** Connecteurs Atlassian + M365 (MCP).
- **1d** Frameworks BMAD + Spec-Kit + powerskills (recipes/skills).
- **Transversal** Modes & silos (superviseur, isolation stricte, Local complet ; Serveur/Remote de base + authz équipe).
- **UI** Tauri (Chat + Code).
**Sortie** : depuis UI *et* CLI *et* API, scénario réel — « lire spec Confluence → plan Spec-Kit → code TDD → PR Bitbucket → ticket Jira », en basculant cloud↔local, isolation des silos vérifiée par test.

> À partir d'ici, **roadmap d'évolution multimodale & collaborative**. Chaque version reste local-first et respecte silos/egress/authz. L'ordre suit les **dépendances techniques**, pas les catégories — le pivot est le *grounding visuel* (v2c), réutilisé par captures/cowork/navigateur.

### Socle v2 — Abstraction de contenu typé (transversal, prérequis)
Le cœur passe de texte à `Content` typé (`text|image|audio|doc|video`, §4.2). Seul ajout cœur de toute la roadmap.
**Sortie** : un `Message` multi-parts traverse le pipeline et est persisté/rechargé sans perte de type.

### v2 — Entrées multimodales + cowork + web
- **v2a** Documents + images : parsing/OCR + vision.
- **v2b** Audio en entrée : speech-to-text.
- **v2c** **Grounding visuel** (ancrage sur capture) — *brique pivot*.
- **Cowork + compagnon web** (regroupés ici car même brique) : `LocalController`/`ComputerUseController` (pilotage PC) + `BrowserController` (extension Chrome/Edge) + **ingestion/mémorisation web** (RAG du silo) et **navigation temps réel**.
**Sortie** : (a) résumer un PDF + une image + un extrait audio dans une même requête ; (b) piloter une appli tierce et une page web sur une tâche scriptée, avec confirmations.

### v3 — Sorties génératives
Image (via Gateway), audio/**TTS**, **fichiers Office** (docx/pptx/xlsx). Indépendant — peut chevaucher v2.
**Sortie** : produire un .docx + une image + un clip audio à partir d'un prompt, artefacts persistés au silo.

### v4 — Temporel
Images animées + **vidéo** (dépend de la génération d'image v3).
**Sortie** : générer un court clip vidéo à partir d'une consigne + assets.

### v5 — Omni (in+out unifié)
Branchement d'un **modèle omni** via la Gateway une fois le pipeline typé complet des deux côtés.
**Sortie** : une conversation unique mêlant texte/image/audio en entrée et sortie via un seul modèle.

### v6 — Audio temps réel (voix streaming duplex)
Interface `RealtimeSession` (canal bidirectionnel continu) — **modifie le modèle d'exécution du cœur** (cf. §10).
**Sortie** : conversation vocale en direct, faible latence, interruptible.

### v7 — Traduction temps réel multilingue
Chacun **écrit/lit/parle/entend dans sa langue maternelle** dans Teams / un chat d'équipe. Composition STT→traduction→TTS + traduction texte, par participant (profil langue porté par la partition d'équipe §5).
**Sortie** : deux membres de langues différentes échangent (texte + voix), chacun dans sa langue, via le connecteur Teams/chat.

### Piste ⟂ — Durcissement équipe/entreprise (parallèle)
SSO/OIDC, audit avancé, RAG avancé, vLLM GPU, connecteurs additionnels, catalogue de recipes. **Cadencée par l'adoption d'équipe**, pas par les modalités.
**Sortie** : déploiement multi-postes (1 leader, N followers, 2 équipes isolées) validé.

---

## 9. CI/CD & best practices

- **Monorepo** polyglotte : `core/` (Cargo), `sidecar/` (uv), `ui/` (pnpm), `connectors/`, `skills/`, `docs/`. Trunk-based, Conventional Commits, PR + revue.
- **Tests** : pyramide (unitaires par frontière + intégration MCP factice + **E2E via CLI** golden path) + **tests d'isolation** dédiés ; TDD sur le cœur.
- **Qualité par langage** : Rust (`fmt`/`clippy -D warnings`/`cargo audit`/`cargo deny`) · Python (`ruff`/`mypy`/`pytest`/`pip-audit`) · TS (`eslint`/`prettier`/`vitest`/`playwright`/`tsc`). Pre-commit unifié.
- **Pipeline** : lint → build 3 langages → unit → intégration → E2E CLI → **scans sécurité (SAST, deps, secrets, SBOM)** → artefacts. Matrice OS : **Windows primaire**, Linux/macOS secondaires.
- **Release** : SemVer, changelog auto, **signature de code** (indispensable pour un agent qui pilote le PC), canaux nightly/stable.
- **Méthode** : dogfooding — Skein est conçu avec Spec-Kit + BMAD ; ADR pour les décisions d'archi.

---

## 10. Risques & questions ouvertes

| Risque / question | Impact | Piste |
|---|---|---|
| Intégrer vs forker Goose (rythme upstream) | Élevé | À trancher en Phase 0 : dépendance vs fork maintenu. |
| Fiabilité du *grounding* cowork (ancrage des clics) | Moyen | Hybride Computer Use d'abord, local ensuite. |
| Packaging inférence locale sur Windows (llama.cpp/vLLM) | Moyen | Ollama comme défaut robuste ; vLLM GPU optionnel. |
| Compatibilité licences (Apache/MIT/…) pour distribution commerciale | Moyen | Audit licences en CI (`cargo deny`, équivalents). |
| Périmètre MVP large (4 axes) | Moyen | Phase 0 dérisque l'archi ; scénario de sortie force l'intégration. |
| Maturité de l'authz équipe local-first | Moyen | Clés en v1, SSO en piste entreprise. |
| **Canal duplex streaming (v6)** modifie le modèle d'exécution du cœur | **Élevé** | Isoler dans `RealtimeSession` ; s'appuyer d'abord sur une API omni-realtime avant un stack WebRTC maison. |
| Coût/latence de la génération vidéo (v4) | Moyen | Providers cloud d'abord ; local GPU optionnel ; jobs asynchrones. |
| Publication & sécurité de l'extension navigateur (v2) | Moyen | Périmètre de permissions minimal ; revue store Chrome/Edge ; frontière anti-injection stricte. |
| Qualité/latence traduction temps réel (v7) | Moyen | Modèles dédiés + fallback texte ; profil langue explicite par membre. |

---

## 11. Glossaire
- **Silo** : partition de données étanche associée à un mode (et une équipe en Remote).
- **Leader / Follower** : instance exposant son backend / instance rattachée à un leader.
- **Recipe** : bundle YAML Goose (instructions + extensions + prompt) = une skill.
- **Controller** : abstraction du pilotage PC (capture + clavier/souris).
- **Sidecar** : process Python auxiliaire (embeddings/RAG/éval).
