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
Le v1 est **texte**. L'évolution multimodale et collaborative est planifiée en versions v2→v8 + une piste parallèle entreprise — voir la **§8 Roadmap d'évolution**. Progression logique : *percevoir → agir → générer → animer → unifier → parler → traduire*.
- **v2 — Perception** (entrées) : abstraction de contenu typé + documents/images (OCR+vision) + audio (STT) + grounding visuel + ingestion/mémoire web.
- **v3 — Action** (cowork) : pilotage PC (local / Computer Use) + compagnon navigateur (Chrome/Edge) + navigation web temps réel.
- **v4 — Génération** de médias : image + audio/TTS + fichiers Office.
- **v5 — Temporel** : images animées + vidéo.
- **v6 — Omni** : **orchestration multi-modèles** (parallèle/séquentiel en arrière-plan) donnant l'illusion d'un modèle unique ; un vrai modèle omni est un cas particulier branché via la Gateway.
- **v7 — Voix temps réel** : audio streaming duplex faible latence.
- **v8 — Traduction** temps réel multilingue (Teams / chat d'équipe, langue maternelle par membre).
- **Piste ⟂** : durcissement équipe/entreprise (IdP externes LDAP/OIDC/Entra/Google + RBAC avancé §7.9-7.10, audit avancé, RAG avancé, vLLM GPU, catalogue de recipes, certifications), cadencée par l'adoption d'équipe. *Identité locale + RBAC de base + observabilité + compliance-by-design sont dès v1.*

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
| **Stratégie Goose** | **Dépendance upstream** par défaut ; **fork/patch hybride** si un besoin cœur n'est pas exposé, **avec PR remontée à l'upstream** | Coût de maintenance minimal ; le fork converge vers l'upstream au lieu de diverger ; bon citoyen open-source. |
| **Harness éditable** | Config **en couches** : base **équipe** (chefs, verrouillable) + surcharges **locales** (utilisateur) — voir §5.4 | Gouvernance d'équipe + liberté locale, sans casser l'isolation des silos. |
| **Identité** | Fournisseur **pluggable** : base locale (défaut) / LDAP-AD / OIDC / Entra ID / Google Workspace — §7.9 | Local-first hors ligne ; IdP entreprise + groupes en mode Serveur/Remote. |
| **Autorisation** | **RBAC** rôles+permissions à **3 portées** (globale / silos / intra-silo) — §7.10 | Contrôle fin de l'usage, de l'accès aux silos et des fonctions/paramètres. |
| **Observabilité** | **OpenTelemetry** + audit immuable, dès v1 — §7.11 | Standard exportable ; base transversale de la conformité. |
| **Conformité** | **Compliance-by-design** : RGPD / ISO 27001 / SOC 2 / EU AI Act / NIS2 — §7.12 | Le logiciel fournit les contrôles ; la certification reste organisationnelle. |
| **Traçabilité** | **Ledger event-sourced façon git** : chaque étape (I/O modèles, tools, état) immuable, inspectable, rejouable, réversible — §4.11 | Transparence totale (tout l'in/out modèles, pas que les résultats) ; capturé à la Gateway. |

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
**Chokepoint de traçabilité** : toute I/O modèle transite ici → la Gateway **capture entrées/sorties modèles vers le Ledger (§4.11)**, quel que soit le runtime émetteur.

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

### 4.9 Controller cowork (`controller/`, interface posée en v1 ; capture v2, pilotage v3)
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
Les trois usages — captures en entrée (grounding, v2), pilotage PC et compagnon navigateur (v3) — réutilisent **la même brique de grounding visuel**. Ce sont trois implémentations d'un même trait, pas trois développements distincts.

### 4.10 Modalités génératives, orchestration omni & flux temps réel (v4+)
- **Génération** (v4/v5) : tools/connecteurs spécialisés — image (modèle via Gateway), **TTS** (audio), **fichiers Office** (docx/pptx/xlsx via bibliothèques matures), **vidéo** (v5). Chaque sortie est un `Content` typé produit par un tool ; le cœur n'en dépend pas.
- **Orchestrateur omni** (v6) : couche entre `Agent` et `Gateway` qui **décompose** une requête multimodale, route chaque sous-tâche vers le **modèle spécialisé** approprié (vision, ASR, TTS, image, LLM) — **en parallèle** quand les sous-tâches sont indépendantes, **en séquentiel** quand elles dépendent l'une de l'autre — puis **recompose** une réponse unifiée. Donne l'**illusion d'un modèle omni unique tout en restant multi-provider**. Un vrai modèle omni = une route parmi d'autres.
  ```rust
  trait OmniOrchestrator {
    fn plan(&self, input: Message) -> Vec<SubTask>;         // décomposition
    fn dispatch(&self, tasks: Vec<SubTask>) -> Vec<Content>; // // parallèle/séquentiel via Gateway
    fn compose(&self, parts: Vec<Content>) -> Message;       // recomposition
  }
  ```
- **Canal duplex streaming** (v7, *nouveauté du modèle d'exécution*) : l'audio temps réel exige un flux **bidirectionnel continu** (in et out simultanés), distinct de la boucle requête→réponse. Introduit une interface `RealtimeSession` (WebRTC/streaming ou API omni-realtime) — c'est le **seul jalon qui modifie le modèle d'exécution du cœur** (cf. risques §10).
- **Traduction d'équipe** (v8) : composition STT→traduire→TTS (voix) + traduction texte, **par participant** selon un profil « langue maternelle » porté par le membre dans la **partition d'équipe** (§5), via le connecteur Teams/chat.

### 4.11 Ledger d'exécution (event-sourced, façon git) — transversal, dès v1
**Chaque étape est une révision immuable.** Skein enregistre, dans un journal **append-only, adressé par hachage et chaîné (parent→enfant)**, *tout* ce qui compose une exécution — pas seulement les résultats produits :
- **Entrées modèles** : le contexte/prompt **exact** envoyé à chaque modèle.
- **Sorties modèles** : la réponse **brute** de chaque modèle (avant post-traitement).
- **Tool-calls** : appel (nom + arguments) **et** résultat.
- **Changements d'état** : mutations de session/fichiers (avec snapshot pré-mutation quand c'est réversible).

```rust
struct StepId(String);                 // hash de contenu (comme un SHA de commit)
struct Step {
  id: StepId, parent: Option<StepId>,  // chaîne/DAG
  ts: i64, principal: PrincipalId, silo: SiloRef,
  kind: StepKind,                      // LlmRequest | LlmResponse | ToolCall | ToolResult | StateChange
  payload: Content,                    // le contenu intégral (in ou out)
}
trait Ledger {
  fn append(&self, step: Step) -> StepId;        // append-only
  fn history(&self, session: SessionId) -> Vec<Step>;   // "git log"
  fn show(&self, id: StepId) -> Step;            // inspecter in/out exacts
  fn replay(&self, from: StepId) -> EventStream; // rejouer depuis un point
  fn revert(&self, to: StepId) -> Result<()>;    // annuler (effets réversibles) + restaurer snapshot
  fn branch(&self, from: StepId) -> SessionId;   // explorer une alternative
}
```
- **Point de capture** : les entrées/sorties modèles sont capturées à la **Gateway (§4.5)** — chokepoint unique traversé par tout runtime (Goose inclus) → aucune I/O modèle n'échappe au journal.
- **Réversibilité honnête** : effets internes (fichiers/session) **annulables** par snapshot ; effets externes irréversibles (e-mail envoyé, ticket créé) **enregistrés et signalés** comme non annulables (action compensatoire proposée, jamais auto).
- **Isolation & sécurité** : le journal vit **dans le silo** (§5.3) ; il contient des prompts potentiellement sensibles → soumis au **trousseau/rédaction, à l'egress et à la rétention** (§7). C'est aussi la pièce maîtresse de la traçabilité RGPD/AI Act (§7.11-7.12).
- **Surfaces** : `skein ledger log|show|replay|revert|branch` (CLI de référence) ; l'UI n'est qu'une vue de ce journal.

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

### 5.4 Gouvernance & configuration du harness (éditable local + équipe)
Le harness est **configurable et versionné** (config-as-code : instructions système, tools activés + permissions, skills/recipes, paramètres de contexte, routage modèles, politiques sécurité/egress, garde-fous).

**Rôles** (dans la partition d'équipe, §5.3) : `membre`, `chef d'équipe`, `chef de projet`, `admin`. Seuls chefs/admin éditent la couche équipe.

**Configuration en deux couches, fusionnées à la résolution :**

| Couche | Éditée par | Stockage (silo) | Effet |
|---|---|---|---|
| **Équipe** | chef d'équipe / chef de projet / admin | partition d'équipe (mode Remote) | base commune ; réglages **verrouillables** |
| **Locale** | l'utilisateur | silo local | surcharge/complète la base ; seule couche en mode Local pur |

**Règles de résolution :**
- Précédence : le **local surcharge l'équipe**, *sauf* réglages marqués **verrouillés** par un chef/admin (non surchargeables — gouvernance).
- Isolation respectée : config équipe en partition d'équipe, config locale en silo local. En **mode Local pur**, aucune couche équipe (cohérent avec §5.3).
- **Versionné** : historisé, revu, réversible (édité comme du code).
- **Sécurité (lien §7)** : les réglages de sécurité (egress, garde-fous, connecteurs interdits) sont **verrouillables** par chef/admin ; un utilisateur local **ne peut pas desserrer** une contrainte imposée par l'équipe. Toute modification de config sécurité est **auditée**.

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

## 7. Sécurité, identité, observabilité & conformité

1. **AuthN (Remote/leader)** : identité vérifiée à l'attache via un **fournisseur d'identité pluggable** (§7.9). Accès **deny-by-default**. **TLS obligatoire** dès exposition.
2. **Secrets** : coffre OS (Windows Credential Manager/DPAPI ; Keychain/secret-service). **Jamais en clair**. **Un trousseau par silo**. L'agent n'entre jamais de credentials dans un formulaire.
3. **Egress par mode** : Local = aucune sortie (local uniquement) ; Serveur/Remote = sortie selon **politique explicite** (allow-list d'endpoints).
4. **Garde-fous d'exécution** : actions destructives/irréversibles → **confirmation** (ou allow-list en CI) ; **bac à sable** pour code/shell ; journal d'audit des tool-calls (quoi/quand/quel silo).
5. **Défense injection de prompt** : tout contenu rapporté par un tool (e-mail, page, issue, capture) est **donnée, pas instruction**. Consignes trouvées dans du contenu externe → signalées, jamais exécutées.
6. **Chaîne d'appro. (MCP & recipes)** : registre de confiance, épinglage de versions, revue avant activation. Pas de chargement silencieux.
7. **Sûreté cowork / navigateur / voix (v2+)** : confirmations avant actions irréversibles ; pas de saisie de credentials ; captures d'écran, contenu web et audio confinés au silo + politique egress ; le compagnon navigateur n'agit jamais sur instruction trouvée *dans* une page (frontière §7.5).
8. **Gouvernance du harness (§5.4)** : les réglages de sécurité sont **verrouillables** par chef d'équipe/projet/admin et **non surchargeables** en local ; toute édition de config (surtout sécurité) est **auditée et versionnée**. L'édition du harness est elle-même une action gouvernée, pas un contournement des règles ci-dessus.
9. **Ledger d'exécution (§4.11)** : contient l'intégralité des prompts/réponses modèles → **donnée sensible**. Confiné au **silo**, soumis à l'**egress**, au **trousseau** (rédaction des secrets avant persistance), à une **politique de rétention** configurable, et à l'**accès RBAC** (qui peut lire le journal). Le ledger est la source de vérité de la traçabilité (§7.11-7.12) — il ne se contourne pas.

### 7.9 Identité (fournisseurs pluggables)
Abstraction unique, plusieurs back-ends interchangeables — même pattern de couplage inversé que le reste :
```rust
trait IdentityProvider {
  fn authenticate(&self, cred: Credential) -> Principal;      // qui es-tu
  fn groups(&self, p: &Principal) -> Vec<Group>;              // tes groupes
}
impl LocalUserStore     // base d'utilisateurs locale (défaut, hors ligne)
impl LdapProvider       // annuaire LDAP/AD
impl OidcProvider       // OIDC générique
impl EntraIdProvider    // Microsoft Entra ID (+ groupes)
impl GoogleWorkspace    // Google Workspace (+ groupes)
```
- **Mapping groupes → rôles** : les groupes Entra/Google/LDAP/OIDC sont mappés vers les rôles RBAC (§7.10) par une table de correspondance gérée par un admin.
- **Défaut local-first** : `LocalUserStore` fonctionne sans réseau ; les IdP externes sont activés en mode Serveur/Remote (piste entreprise §8).

### 7.10 RBAC (rôles + permissions, à trois portées)
Autorisation **deny-by-default**, évaluée à trois niveaux imbriqués :

| Portée | Contrôle | Exemples de permissions |
|---|---|---|
| **Globale (outil)** | usage général de l'outil | se connecter, créer une session, utiliser le cowork, exposer un backend |
| **Accès aux silos** | quels silos un principal peut voir/utiliser | lire/écrire `team:alpha`, refuser `team:beta` |
| **Intra-silo** | fonctions & paramétrages dans un silo | activer tel connecteur, éditer le harness, changer l'egress, invoquer telle skill, utiliser tel provider |

- **Rôles** (composables) : `membre`, `chef d'équipe`, `chef de projet`, `admin` (+ rôles custom) ; chaque rôle = un ensemble de permissions.
- **Cohérence avec §5.4** : les « réglages verrouillables » du harness sont exprimés comme des permissions intra-silo (ex. `harness.egress.edit` réservé aux chefs/admin).
- **Fournie tôt** en version locale (base locale + rôles de base) ; RBAC avancé + IdP externes sur la **piste entreprise**.

### 7.11 Observabilité
- **Traces / métriques / logs** via **OpenTelemetry** (standard, exportable vers l'outillage de l'entreprise). Intégrée **dès v1** (peu coûteux tôt, pénible à rétrofit).
- **Journal d'audit** immuable : authN/authZ, tool-calls, éditions de harness/sécurité, accès aux silos — horodaté, attribué au principal, borné au silo. Complémentaire du **Ledger d'exécution (§4.11)** qui, lui, capture le *contenu* (I/O modèles) : l'audit dit « qui a fait quoi », le ledger dit « quoi exactement a été envoyé/reçu ».
- **Métriques produit** : coût/tokens par provider (via LiteLLM), latence, taux d'échec des tools ; **respectent la politique egress** (pas d'export hors politique).

### 7.12 Conformité (compliance-by-design)
> Le logiciel **fournit les contrôles** qui *permettent* la conformité ; la **certification** (ISO 27001, SOC 2) reste un processus **organisationnel**. Skein est conçu pour ne pas être le maillon bloquant.

| Cadre | Ce que Skein apporte |
|---|---|
| **RGPD** | Minimisation (mode Local sans egress), **droit à l'effacement** & export/portabilité par admin, résidence des données (local/on-prem), base légale/consentement, registre de traitement, chiffrement au repos & en transit. |
| **ISO 27001** | Contrôle d'accès (RBAC §7.10), gestion des secrets (§7.2), audit (§7.11), gestion du changement (config-as-code versionnée §5.4), chaîne d'appro. (§7.6). |
| **SOC 2** | Critères *Security/Confidentiality/Availability* : RBAC, audit immuable, chiffrement, isolation des silos, fallback local (§5.2). |
| **EU AI Act** | Transparence (divulgation « contenu généré par IA »), **supervision humaine** (confirmations §7.4), traçabilité des décisions IA (audit §7.11), documentation des modèles/routage (via Gateway), classification de risque des usages. |
| **NIS2** | Mesures techniques (chiffrement, MFA via IdP, durcissement), **journalisation & remontée d'incident**, sécurité de la chaîne d'appro. (§7.6), gouvernance (rôles/responsabilités). |

- **Rétention & résidence** : politiques de rétention par silo ; les données restent où le mode l'impose (Local = jamais de sortie).
- **Traçabilité** : l'audit (§7.11) est la pièce transversale qui sert RGPD, ISO, SOC 2, AI Act et NIS2 à la fois.

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

> À partir d'ici, **roadmap d'évolution multimodale & collaborative**. Progression logique : *percevoir → agir → générer → animer → unifier → parler → traduire*. Chaque version reste local-first et respecte silos/egress/authz, et s'appuie sur la précédente. Pivot technique : le *grounding visuel* (v2), réutilisé par captures/cowork/navigateur.

### v2 — Perception (entrées multimodales)
D'abord l'**abstraction de contenu typé** (`Content = text|image|audio|doc|video`, §4.2) — seul ajout cœur de toute la roadmap — puis :
- **Documents + images** : parsing/OCR + vision.
- **Audio en entrée** : speech-to-text.
- **Grounding visuel** (ancrage sur capture) — *brique pivot* réutilisée en v3.
- **Web** : ingestion/mémorisation de contenu web dans le RAG du silo.
**Sortie** : résumer, dans une même requête, un PDF + une image + un extrait audio + une page web, persisté sans perte de type.

### v3 — Action (cowork & pilotage)
Réutilise le grounding v2 pour **agir** sur des surfaces externes :
- **Pilotage PC** : `LocalController` (enigo/xcap) + `ComputerUseController` (API).
- **Compagnon navigateur** : `BrowserController` (extension Chrome/Edge) + **navigation web temps réel**.
**Sortie** : piloter une appli tierce **et** une page web sur une tâche scriptée, avec confirmations sur actions irréversibles.

### v4 — Génération de médias (sorties)
Image (via Gateway), audio/**TTS**, **fichiers Office** (docx/pptx/xlsx). Indépendant de v3 — peut chevaucher.
**Sortie** : produire un .docx + une image + un clip audio à partir d'un prompt, artefacts persistés au silo.

### v5 — Temporel (animation & vidéo)
Images animées + **vidéo** (dépend de la génération d'image v4).
**Sortie** : générer un court clip vidéo à partir d'une consigne + assets.

### v6 — Omni (illusion d'un modèle unique)
**Orchestrateur omni** (§4.10) : décompose une requête multimodale, route vers les modèles spécialisés (parallèle/séquentiel en arrière-plan), recompose. Illusion d'un modèle unique **sans dépendance à un modèle omni propriétaire** ; un vrai omni est une route parmi d'autres.
**Sortie** : une conversation unique mêlant texte/image/audio en entrée et sortie, servie par plusieurs modèles orchestrés de façon transparente.

### v7 — Voix temps réel (audio streaming duplex)
Interface `RealtimeSession` (canal bidirectionnel continu) — **modifie le modèle d'exécution du cœur** (cf. §10). S'appuie sur l'orchestration v6.
**Sortie** : conversation vocale en direct, faible latence, interruptible.

### v8 — Traduction temps réel multilingue
Chacun **écrit/lit/parle/entend dans sa langue maternelle** dans Teams / un chat d'équipe. Composition STT→traduction→TTS + traduction texte, par participant (profil langue porté par la partition d'équipe §5).
**Sortie** : deux membres de langues différentes échangent (texte + voix), chacun dans sa langue, via le connecteur Teams/chat.

### Piste ⟂ — Durcissement équipe/entreprise (parallèle)
IdP externes (LDAP/OIDC/Entra/Google) + **RBAC avancé** (§7.9-7.10), audit avancé, RAG avancé, vLLM GPU, connecteurs additionnels, catalogue de recipes, préparation aux **certifications** (ISO 27001 / SOC 2). **Cadencée par l'adoption d'équipe**, pas par les modalités. *NB : identité locale + RBAC de base + observabilité (OpenTelemetry) + compliance-by-design sont intégrés dès v1.*
**Sortie** : déploiement multi-postes (1 leader, N followers, 2 équipes isolées) avec IdP entreprise + RBAC à 3 portées validé.

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
| ~~Intégrer vs forker Goose~~ **(DÉCIDÉ)** | — | **Dépendance upstream par défaut** ; fork/patch hybride si un besoin cœur n'est pas exposé, **avec PR remontée à l'upstream** (le fork converge, ne diverge pas). |
| Fiabilité du *grounding* cowork (ancrage des clics) | Moyen | Hybride Computer Use d'abord, local ensuite. |
| Packaging inférence locale sur Windows (llama.cpp/vLLM) | Moyen | Ollama comme défaut robuste ; vLLM GPU optionnel. |
| Compatibilité licences (Apache/MIT/…) pour distribution commerciale | Moyen | Audit licences en CI (`cargo deny`, équivalents). |
| Périmètre MVP large (4 axes) | Moyen | Phase 0 dérisque l'archi ; scénario de sortie force l'intégration. |
| Complexité RBAC à 3 portées × IdP multiples | **Élevé** | Modèle de permissions unique et testé ; deny-by-default ; base locale d'abord, IdP externes ensuite ; suite de tests d'autorisation dédiée. |
| Effacement RGPD vs blocage des suppressions destructives (§7.4) | Moyen | L'**agent** ne hard-delete pas ; l'**effacement RGPD** est une fonction *admin* gouvernée + auditée — pas une contradiction, deux chemins distincts. |
| Classification EU AI Act selon l'usage (peut passer « haut risque ») | Moyen | Transparence + supervision humaine + audit par défaut ; documenter les usages ; laisser l'org classifier son déploiement. |
| **Canal duplex streaming (v7)** modifie le modèle d'exécution du cœur | **Élevé** | Isoler dans `RealtimeSession` ; s'appuyer d'abord sur une API omni-realtime avant un stack WebRTC maison. |
| Complexité de l'orchestrateur omni (v6) : latence de composition, cohérence | Moyen | Router simple d'abord (règles par type de `Content`) ; paralléliser l'indépendant ; mesurer la latence de recomposition. |
| Coût/latence de la génération vidéo (v5) | Moyen | Providers cloud d'abord ; local GPU optionnel ; jobs asynchrones. |
| Publication & sécurité de l'extension navigateur (v3) | Moyen | Périmètre de permissions minimal ; revue store Chrome/Edge ; frontière anti-injection stricte. |
| Qualité/latence traduction temps réel (v8) | Moyen | Modèles dédiés + fallback texte ; profil langue explicite par membre. |
| Croissance du Ledger (§4.11) : volume des prompts/réponses stockés | Moyen | Rétention configurable, compaction/archivage, adressage par hachage (dédup), stockage de gros blobs hors DB. |
| Secrets présents dans les prompts capturés par le Ledger | **Élevé** | Rédaction/masquage avant persistance ; chiffrement au repos ; accès RBAC ; jamais de sortie hors egress. |
| Event sourcing = décision d'archi fondatrice (coûteuse à rétrofit) | Moyen | Capturer dès v1 (Phase 0) même si revert/branch avancés viennent après — voir plan Phase 0. |

---

## 11. Glossaire
- **Silo** : partition de données étanche associée à un mode (et une équipe en Remote).
- **Leader / Follower** : instance exposant son backend / instance rattachée à un leader.
- **Recipe** : bundle YAML Goose (instructions + extensions + prompt) = une skill.
- **Controller** : abstraction du pilotage PC (capture + clavier/souris).
- **Sidecar** : process Python auxiliaire (embeddings/RAG/éval).
- **Principal** : entité authentifiée (utilisateur/service) portant une identité et des groupes.
- **IdP** : fournisseur d'identité pluggable (local, LDAP/AD, OIDC, Entra ID, Google Workspace).
- **RBAC** : contrôle d'accès par rôles+permissions, à 3 portées (globale / silos / intra-silo).
- **Omni-orchestrateur** : couche qui compose plusieurs modèles spécialisés pour simuler un modèle unique.
- **Harness** : configuration du comportement de l'agent (instructions, tools, skills, contexte, politiques) — éditable en couches équipe/locale.
- **Ledger d'exécution** : journal append-only, adressé par hachage et chaîné (façon commits git), capturant chaque étape (I/O modèles, tool-calls, changements d'état) ; inspectable, rejouable, réversible, révisable.
- **Event sourcing** : l'état est dérivé d'un journal d'événements immuables plutôt que muté en place — permet historique, replay et time-travel.
