# Skein — Phase 0 : Squelette vertical (Walking Skeleton) — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prouver la tranche verticale complète de Skein : `skein` CLI → cœur headless → Goose (dépendance, mode headless) → Gateway LiteLLM → modèle, avec persistance de session dans le **silo Local** et rechargement.

**Architecture:** Cœur headless en Rust (crate `skein-core`) exposant une API programmatique ; CLI de référence (`skein-cli`). Goose est intégré comme **dépendance upstream** via sa CLI headless (`goose run`) encapsulée derrière notre trait `AgentRuntime` (couche anti-corruption). LiteLLM sert de passerelle OpenAI-compatible (`:4000/v1`) vers un modèle local (Ollama par défaut). La persistance se fait dans un store SQLite **namespacé par silo** (`local`).

**Tech Stack:** Rust (édition 2021), Cargo workspace · `tokio` (async) · `rusqlite` (SQLite) · `reqwest` (HTTP) · `serde`/`serde_json` · `clap` (CLI) · `thiserror`/`anyhow` · `tracing` + `tracing-subscriber` (observabilité) · `wiremock` + `assert_cmd` + `tempfile` (tests) · Goose CLI (binaire externe) · LiteLLM (proxy Python externe).

## Global Constraints

- **Plateforme primaire : Windows** ; le code reste portable (Linux/macOS secondaires). Pas de chemin codé en dur — utiliser `std::path`.
- **Local-first** : Phase 0 = **mode `local` uniquement**. **Egress réseau OFF par défaut** → la Gateway pointe vers un modèle **local** (Ollama). Aucun appel cloud.
- **Isolation par silo** : toute donnée persistée est préfixée/namespacée par le silo (`local` en Phase 0). Aucune lecture hors silo.
- **Observabilité dès v1** : chaque crate initialise `tracing` ; les événements clés (démarrage run, tool-call, persistance) sont tracés.
- **Ledger dès v1 (event sourcing)** : chaque étape (prompt envoyé / réponse reçue) est capturée dans un journal **append-only, chaîné par hachage** (§4.11 du spec). Phase 0 = capture *au niveau étape* + inspection (`skein ledger log|show`) ; revert/branch/capture token-level via Gateway = phases suivantes.
- **Qualité** : `cargo fmt`, `cargo clippy -D warnings`, `cargo test` doivent passer. TDD strict (test rouge → code → test vert → commit).
- **Commits** : Conventional Commits. Commit fréquent (à chaque tâche minimum).
- **Édition Rust** : 2021. **MSRV** : 1.79.
- **Nom des binaires** : `skein` (CLI). **Crates** : `skein-core`, `skein-cli`.

---

## Structure de fichiers (Phase 0)

```
skein/
├─ Cargo.toml                      # workspace
├─ rust-toolchain.toml             # épingle la toolchain
├─ .github/workflows/ci.yml        # CI (fmt, clippy, test)
├─ crates/
│  ├─ skein-core/
│  │  ├─ Cargo.toml
│  │  └─ src/
│  │     ├─ lib.rs                 # ré-exports + init tracing
│  │     ├─ content.rs             # Content, Message, Role
│  │     ├─ event.rs               # Event (flux d'événements typé)
│  │     ├─ silo.rs                # SiloStore (SQLite, namespacé)
│  │     ├─ gateway.rs             # GatewayClient (OpenAI-compat)
│  │     ├─ runtime.rs             # AgentRuntime trait + GooseRuntime
│  │     ├─ ledger.rs              # LedgerStore (append-only, chaîné par hachage)
│  │     └─ error.rs               # SkeinError
│  └─ skein-cli/
│     ├─ Cargo.toml
│     └─ src/main.rs               # commandes: chat, session list, session show
├─ config/
│  └─ litellm.config.yaml          # Gateway → Ollama (local)
└─ docs/superpowers/
   ├─ specs/2026-07-15-skein-design.md
   └─ plans/2026-07-15-skein-phase0-walking-skeleton.md
```

---

### Task 0: Spike d'intégration Goose (décision d'architecture)

**But :** confirmer par les faits la meilleure voie d'intégration de Goose et figer les flags CLI headless réels. Produit un ADR (décision), pas du code de production.

**Files:**
- Create: `docs/superpowers/adr/0001-goose-integration.md`

- [ ] **Step 1: Installer/obtenir le binaire Goose**

Suivre la doc officielle (https://block-goose.mintlify.app/). Vérifier :
```bash
goose --version
goose --help
goose run --help
```
Noter la présence des flags : `-t/--text`, `-i/--instructions`, `--no-session`, sélection du provider/modèle, activation d'extensions (developer/filesystem).

- [ ] **Step 2: Tester un run headless avec un provider OpenAI-compatible**

Configurer Goose pour pointer vers un endpoint OpenAI-compatible local (ce sera LiteLLM en Task 4 ; ici un Ollama direct suffit pour le spike). Lancer :
```bash
goose run --no-session -t "Write the text 'skein-ok' to a file named probe.txt in the current directory"
```
Vérifier que `probe.txt` est créé (l'extension developer/filesystem de Goose agit).

- [ ] **Step 3: Évaluer les 3 voies d'intégration**

Comparer, avec preuves du Step 1-2 :
1. **CLI subprocess** (`goose run`) — simplicité, stabilité, zéro fork.
2. **REST `goosed`** (`:3000`) — streaming, sessions riches ; schéma JSON à cartographier.
3. **Crate embarqué** (`goose`/`goose-cli` en dépendance Cargo) — contrôle max ; couplage fort à l'upstream.

- [ ] **Step 4: Rédiger l'ADR et trancher pour la Phase 0**

Écrire `docs/superpowers/adr/0001-goose-integration.md` : contexte, options, décision. **Décision par défaut attendue : CLI subprocess pour la Phase 0**, migration possible vers `goosed`/crate en phases ultérieures (dépendance upstream ; fork+PR seulement si un besoin cœur manque). Consigner les **flags CLI exacts** observés (ils paramètrent la Task 5).

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/adr/0001-goose-integration.md
git commit -m "docs(adr): 0001 stratégie d'intégration Goose (Phase 0 = CLI subprocess)"
```

---

### Task 1: Scaffolding du workspace Cargo + CI

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `crates/skein-core/Cargo.toml`, `crates/skein-core/src/lib.rs`, `crates/skein-cli/Cargo.toml`, `crates/skein-cli/src/main.rs`, `.github/workflows/ci.yml`

**Interfaces:**
- Produces: crates compilables `skein-core` (lib) et `skein-cli` (bin `skein`).

- [ ] **Step 1: Créer le manifeste workspace**

`Cargo.toml` :
```toml
[workspace]
resolver = "2"
members = ["crates/skein-core", "crates/skein-cli"]

[workspace.package]
edition = "2021"
rust-version = "1.79"
license = "Apache-2.0"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
rusqlite = { version = "0.31", features = ["bundled"] }
reqwest = { version = "0.12", features = ["json"] }
clap = { version = "4", features = ["derive"] }
```

- [ ] **Step 2: Épingler la toolchain**

`rust-toolchain.toml` :
```toml
[toolchain]
channel = "1.79"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 3: Créer `skein-core` (lib minimale)**

`crates/skein-core/Cargo.toml` :
```toml
[package]
name = "skein-core"
version = "0.0.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
rusqlite.workspace = true
reqwest.workspace = true
tokio.workspace = true

[dev-dependencies]
tempfile = "3"
wiremock = "0.6"
```

`crates/skein-core/src/lib.rs` :
```rust
//! Cœur headless de Skein.
pub mod content;
pub mod error;
pub mod event;
pub mod gateway;
pub mod runtime;
pub mod silo;

/// Initialise le tracing (idempotent). À appeler au démarrage de chaque surface.
pub fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .try_init();
}
```

- [ ] **Step 4: Créer `skein-cli` (bin minimal)**

`crates/skein-cli/Cargo.toml` :
```toml
[package]
name = "skein-cli"
version = "0.0.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "skein"
path = "src/main.rs"

[dependencies]
skein-core = { path = "../skein-core" }
clap.workspace = true
tokio.workspace = true
anyhow.workspace = true
tracing.workspace = true

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
tempfile = "3"
```

`crates/skein-cli/src/main.rs` :
```rust
fn main() {
    skein_core::init_tracing();
    println!("skein 0.0.0");
}
```

- [ ] **Step 5: Ajouter la CI**

`.github/workflows/ci.yml` :
```yaml
name: ci
on: [push, pull_request]
jobs:
  rust:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.79
        with: { components: rustfmt, clippy }
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test --all
```

- [ ] **Step 6: Vérifier la compilation**

Run: `cargo build --all`
Expected: build OK ; `cargo run -p skein-cli` affiche `skein 0.0.0`.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml rust-toolchain.toml crates .github
git commit -m "chore: scaffolding workspace Cargo (skein-core, skein-cli) + CI"
```

---

### Task 2: Types de domaine (`Content`, `Message`, `Role`, `Event`)

**Files:**
- Create: `crates/skein-core/src/content.rs`, `crates/skein-core/src/event.rs`, `crates/skein-core/src/error.rs`

**Interfaces:**
- Produces:
  - `enum Role { User, Assistant, System }`
  - `enum Content { Text(String) }` (v2 ajoutera Image/Audio/Doc/Video)
  - `struct Message { role: Role, parts: Vec<Content> }` + `Message::user_text(&str) -> Message`, `Message::text(&self) -> String`
  - `enum Event { Token(String), ToolCall { name: String, input: String }, Done, Error(String) }`
  - `enum SkeinError` (thiserror)

- [ ] **Step 1: Écrire le test rouge (contenu/message)**

`crates/skein-core/src/content.rs` :
```rust
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_text_roundtrips() {
        let m = Message::user_text("bonjour");
        assert_eq!(m.role, Role::User);
        assert_eq!(m.text(), "bonjour");
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(back.text(), "bonjour");
    }
}
```

- [ ] **Step 2: Lancer le test (échec attendu)**

Run: `cargo test -p skein-core content`
Expected: FAIL (types non définis).

- [ ] **Step 3: Implémenter les types**

Ajouter au-dessus du bloc `#[cfg(test)]` dans `content.rs` :
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role { User, Assistant, System }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Content { Text(String) }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub parts: Vec<Content>,
}

impl Message {
    pub fn user_text(s: &str) -> Self {
        Message { role: Role::User, parts: vec![Content::Text(s.to_string())] }
    }
    /// Concatène toutes les parties texte.
    pub fn text(&self) -> String {
        self.parts.iter().map(|p| match p { Content::Text(t) => t.as_str() }).collect()
    }
}
```

- [ ] **Step 4: Écrire les erreurs et événements**

`crates/skein-core/src/error.rs` :
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkeinError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("gateway: {0}")]
    Gateway(String),
    #[error("runtime: {0}")]
    Runtime(String),
    #[error("not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, SkeinError>;
```

`crates/skein-core/src/event.rs` :
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Token(String),
    ToolCall { name: String, input: String },
    Done,
    Error(String),
}
```

- [ ] **Step 5: Lancer les tests (vert attendu)**

Run: `cargo test -p skein-core`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/skein-core/src/content.rs crates/skein-core/src/event.rs crates/skein-core/src/error.rs
git commit -m "feat(core): types de domaine Content/Message/Role/Event + erreurs"
```

---

### Task 3: Store de silo (SQLite, namespacé par mode)

**Files:**
- Create: `crates/skein-core/src/silo.rs`

**Interfaces:**
- Consumes: `Message` (Task 2), `SkeinError`/`Result` (Task 2).
- Produces:
  - `struct SiloStore` avec `SiloStore::open(path: &Path, namespace: &str) -> Result<SiloStore>`
  - `fn create_session(&self) -> Result<String>` (retourne un id)
  - `fn append(&self, session_id: &str, msg: &Message) -> Result<()>`
  - `fn load(&self, session_id: &str) -> Result<Vec<Message>>`
  - `fn list_sessions(&self) -> Result<Vec<String>>`

- [ ] **Step 1: Écrire le test rouge (persistance + isolation)**

`crates/skein-core/src/silo.rs` :
```rust
use crate::content::Message;
use crate::error::{Result, SkeinError};
use rusqlite::Connection;
use std::path::Path;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Message;

    #[test]
    fn append_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("skein.db");
        let store = SiloStore::open(&db, "local").unwrap();
        let sid = store.create_session().unwrap();
        store.append(&sid, &Message::user_text("salut")).unwrap();
        let msgs = store.load(&sid).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].text(), "salut");
        assert_eq!(store.list_sessions().unwrap(), vec![sid]);
    }

    #[test]
    fn namespaces_are_isolated() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("skein.db");
        let local = SiloStore::open(&db, "local").unwrap();
        let sid = local.create_session().unwrap();
        local.append(&sid, &Message::user_text("secret local")).unwrap();

        // Un autre namespace, même fichier DB, ne voit rien.
        let remote = SiloStore::open(&db, "remote").unwrap();
        assert!(remote.list_sessions().unwrap().is_empty());
        assert!(remote.load(&sid).is_err());
    }
}
```

- [ ] **Step 2: Lancer le test (échec attendu)**

Run: `cargo test -p skein-core silo`
Expected: FAIL (SiloStore non défini).

- [ ] **Step 3: Implémenter `SiloStore`**

Ajouter au-dessus du bloc `#[cfg(test)]` :
```rust
pub struct SiloStore {
    conn: Connection,
    namespace: String,
}

impl SiloStore {
    pub fn open(path: &Path, namespace: &str) -> Result<SiloStore> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT NOT NULL,
                namespace TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (id, namespace)
            );
            CREATE TABLE IF NOT EXISTS messages (
                session_id TEXT NOT NULL,
                namespace TEXT NOT NULL,
                seq INTEGER NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY (session_id, namespace, seq)
            );",
        )?;
        Ok(SiloStore { conn, namespace: namespace.to_string() })
    }

    pub fn create_session(&self) -> Result<String> {
        // id déterministe-libre : compteur + namespace (pas de RNG requis en Phase 0)
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE namespace = ?1",
            [&self.namespace],
            |r| r.get(0),
        )?;
        let id = format!("s{:06}", count + 1);
        let now: i64 = self.conn.query_row("SELECT strftime('%s','now')", [], |r| r.get(0))?;
        self.conn.execute(
            "INSERT INTO sessions (id, namespace, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, self.namespace, now],
        )?;
        Ok(id)
    }

    pub fn append(&self, session_id: &str, msg: &Message) -> Result<()> {
        let seq: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE session_id = ?1 AND namespace = ?2",
            rusqlite::params![session_id, self.namespace],
            |r| r.get(0),
        )?;
        let payload = serde_json::to_string(msg)?;
        self.conn.execute(
            "INSERT INTO messages (session_id, namespace, seq, payload) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![session_id, self.namespace, seq, payload],
        )?;
        Ok(())
    }

    pub fn load(&self, session_id: &str) -> Result<Vec<Message>> {
        let exists: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE id = ?1 AND namespace = ?2",
            rusqlite::params![session_id, self.namespace],
            |r| r.get(0),
        )?;
        if exists == 0 {
            return Err(SkeinError::NotFound(format!("session {session_id}")));
        }
        let mut stmt = self.conn.prepare(
            "SELECT payload FROM messages WHERE session_id = ?1 AND namespace = ?2 ORDER BY seq",
        )?;
        let rows = stmt.query_map(rusqlite::params![session_id, self.namespace], |r| {
            r.get::<_, String>(0)
        })?;
        let mut out = Vec::new();
        for row in rows {
            let payload = row?;
            out.push(serde_json::from_str(&payload)?);
        }
        Ok(out)
    }

    pub fn list_sessions(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM sessions WHERE namespace = ?1 ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map([&self.namespace], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}
```

- [ ] **Step 4: Lancer les tests (vert attendu)**

Run: `cargo test -p skein-core silo`
Expected: PASS (les deux tests, dont l'isolation par namespace).

- [ ] **Step 5: Commit**

```bash
git add crates/skein-core/src/silo.rs
git commit -m "feat(core): SiloStore SQLite namespacé + test d'isolation inter-silo"
```

---

### Task 4: Client de passerelle (OpenAI-compatible) + config LiteLLM

**Files:**
- Create: `crates/skein-core/src/gateway.rs`, `config/litellm.config.yaml`

**Interfaces:**
- Consumes: `SkeinError`/`Result` (Task 2).
- Produces:
  - `struct GatewayClient { base_url: String, api_key: String, http: reqwest::Client }`
  - `GatewayClient::new(base_url: &str, api_key: &str) -> GatewayClient`
  - `async fn health(&self) -> Result<bool>` (GET `{base_url}/models`)
  - `async fn complete(&self, model: &str, prompt: &str) -> Result<String>` (POST `{base_url}/chat/completions`)

- [ ] **Step 1: Écrire le test rouge (contre un serveur stub wiremock)**

`crates/skein-core/src/gateway.rs` :
```rust
use crate::error::{Result, SkeinError};
use serde_json::json;

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn complete_parses_openai_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"role": "assistant", "content": "pong"}}]
            })))
            .mount(&server)
            .await;

        let client = GatewayClient::new(&server.uri(), "sk-test");
        let out = client.complete("local-model", "ping").await.unwrap();
        assert_eq!(out, "pong");
    }

    #[tokio::test]
    async fn health_true_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": []})))
            .mount(&server)
            .await;
        let client = GatewayClient::new(&server.uri(), "sk-test");
        assert!(client.health().await.unwrap());
    }
}
```

- [ ] **Step 2: Lancer le test (échec attendu)**

Run: `cargo test -p skein-core gateway`
Expected: FAIL (GatewayClient non défini).

- [ ] **Step 3: Implémenter `GatewayClient`**

Ajouter au-dessus du bloc `#[cfg(test)]` :
```rust
pub struct GatewayClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl GatewayClient {
    pub fn new(base_url: &str, api_key: &str) -> GatewayClient {
        GatewayClient {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            http: reqwest::Client::new(),
        }
    }

    pub async fn health(&self) -> Result<bool> {
        let resp = self.http
            .get(format!("{}/models", self.base_url))
            .bearer_auth(&self.api_key)
            .send().await.map_err(|e| SkeinError::Gateway(e.to_string()))?;
        Ok(resp.status().is_success())
    }

    pub async fn complete(&self, model: &str, prompt: &str) -> Result<String> {
        let body = json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}]
        });
        let resp = self.http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send().await.map_err(|e| SkeinError::Gateway(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(SkeinError::Gateway(format!("status {}", resp.status())));
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| SkeinError::Gateway(e.to_string()))?;
        v["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| SkeinError::Gateway("réponse sans contenu".into()))
    }
}
```

- [ ] **Step 4: Écrire la config LiteLLM (Gateway → Ollama local)**

`config/litellm.config.yaml` :
```yaml
model_list:
  - model_name: local-model
    litellm_params:
      model: ollama/llama3.1
      api_base: http://localhost:11434
general_settings:
  master_key: sk-skein-local
```

- [ ] **Step 5: Lancer les tests (vert attendu)**

Run: `cargo test -p skein-core gateway`
Expected: PASS (les deux tests, sans réseau réel — wiremock).

- [ ] **Step 6: Commit**

```bash
git add crates/skein-core/src/gateway.rs config/litellm.config.yaml
git commit -m "feat(core): GatewayClient OpenAI-compat + config LiteLLM locale"
```

---

### Task 5: Runtime agentique (adaptateur Goose headless)

**Files:**
- Create: `crates/skein-core/src/runtime.rs`

**Interfaces:**
- Consumes: `Event` (Task 2), `SkeinError`/`Result` (Task 2). Flags CLI confirmés par l'ADR (Task 0).
- Produces:
  - `trait AgentRuntime { async fn run(&self, workdir: &Path, instruction: &str) -> Result<Vec<Event>>; }`
  - `struct GooseRuntime { bin: String, extra_args: Vec<String> }`
  - `GooseRuntime::new(bin: &str) -> GooseRuntime`
  - Impl `AgentRuntime for GooseRuntime` : exécute `<bin> run --no-session -t <instruction>` dans `workdir`, mappe stdout → `Event::Token`, code retour ≠ 0 → `Event::Error`, fin → `Event::Done`.

- [ ] **Step 1: Écrire le test rouge (avec un binaire Goose factice)**

`crates/skein-core/src/runtime.rs` :
```rust
use crate::error::{Result, SkeinError};
use crate::event::Event;
use std::path::Path;

pub trait AgentRuntime {
    fn run(
        &self,
        workdir: &Path,
        instruction: &str,
    ) -> impl std::future::Future<Output = Result<Vec<Event>>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // Crée un faux binaire "goose" qui écrit dans un fichier puis imprime une ligne.
    fn fake_goose(dir: &Path) -> String {
        #[cfg(windows)]
        {
            let p = dir.join("goose.bat");
            let mut f = std::fs::File::create(&p).unwrap();
            // %* = tous les args ; on prouve juste que le binaire est invoqué et écrit un fichier.
            writeln!(f, "@echo off").unwrap();
            writeln!(f, "echo assistant: file written").unwrap();
            writeln!(f, "echo done> probe.txt").unwrap();
            p.to_string_lossy().to_string()
        }
        #[cfg(not(windows))]
        {
            let p = dir.join("goose.sh");
            let mut f = std::fs::File::create(&p).unwrap();
            writeln!(f, "#!/bin/sh").unwrap();
            writeln!(f, "echo 'assistant: file written'").unwrap();
            writeln!(f, "echo done > probe.txt").unwrap();
            let mut perms = std::fs::metadata(&p).unwrap().permissions();
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o755);
            std::fs::set_permissions(&p, perms).unwrap();
            p.to_string_lossy().to_string()
        }
    }

    #[tokio::test]
    async fn run_streams_tokens_and_writes_file() {
        let dir = tempfile::tempdir().unwrap();
        let bin = fake_goose(dir.path());
        let rt = GooseRuntime::new(&bin);
        let events = rt.run(dir.path(), "write a file").await.unwrap();

        assert!(events.iter().any(|e| matches!(e, Event::Token(t) if t.contains("file written"))));
        assert!(matches!(events.last().unwrap(), Event::Done));
        assert!(dir.path().join("probe.txt").exists());
    }
}
```

- [ ] **Step 2: Lancer le test (échec attendu)**

Run: `cargo test -p skein-core runtime`
Expected: FAIL (GooseRuntime non défini).

- [ ] **Step 3: Implémenter `GooseRuntime`**

Ajouter au-dessus du bloc `#[cfg(test)]` :
```rust
pub struct GooseRuntime {
    bin: String,
    /// Flags confirmés par l'ADR 0001 (Task 0). Défaut : run headless sans session.
    extra_args: Vec<String>,
}

impl GooseRuntime {
    pub fn new(bin: &str) -> GooseRuntime {
        GooseRuntime {
            bin: bin.to_string(),
            extra_args: vec!["run".into(), "--no-session".into()],
        }
    }
}

impl AgentRuntime for GooseRuntime {
    async fn run(&self, workdir: &Path, instruction: &str) -> Result<Vec<Event>> {
        use tokio::process::Command;
        tracing::info!(bin = %self.bin, "goose run");
        let output = Command::new(&self.bin)
            .args(&self.extra_args)
            .arg("-t")
            .arg(instruction)
            .current_dir(workdir)
            .output()
            .await
            .map_err(|e| SkeinError::Runtime(format!("spawn goose: {e}")))?;

        let mut events = Vec::new();
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().filter(|l| !l.trim().is_empty()) {
            events.push(Event::Token(line.to_string()));
        }
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            events.push(Event::Error(format!("goose exit {}: {}", output.status, stderr.trim())));
            return Ok(events);
        }
        events.push(Event::Done);
        Ok(events)
    }
}
```

- [ ] **Step 4: Lancer le test (vert attendu)**

Run: `cargo test -p skein-core runtime`
Expected: PASS (le faux binaire écrit `probe.txt`, les tokens sont capturés, `Done` en fin).

- [ ] **Step 5: Commit**

```bash
git add crates/skein-core/src/runtime.rs
git commit -m "feat(core): AgentRuntime + GooseRuntime (adaptateur CLI headless, testé via stub)"
```

---

### Task 6: Orchestration cœur (`chat` : run + persistance silo)

**Files:**
- Modify: `crates/skein-core/src/lib.rs` (ajouter `pub mod session;`)
- Create: `crates/skein-core/src/session.rs`

**Interfaces:**
- Consumes: `SiloStore` (Task 3), `AgentRuntime` (Task 5), `Message`/`Event` (Task 2).
- Produces:
  - `struct ChatService<R: AgentRuntime> { store: SiloStore, runtime: R }`
  - `ChatService::new(store: SiloStore, runtime: R) -> Self`
  - `async fn chat(&self, workdir: &Path, session_id: Option<String>, prompt: &str) -> Result<(String, Vec<Event>)>` — crée/charge la session, persiste le message user, exécute le runtime, persiste la réponse assistant (texte concaténé des `Event::Token`), retourne `(session_id, events)`.

- [ ] **Step 1: Écrire le test rouge (orchestration + persistance)**

`crates/skein-core/src/session.rs` :
```rust
use crate::content::{Message, Role, Content};
use crate::error::Result;
use crate::event::Event;
use crate::runtime::AgentRuntime;
use crate::silo::SiloStore;
use std::path::Path;

#[cfg(test)]
mod tests {
    use super::*;

    struct StubRuntime;
    impl AgentRuntime for StubRuntime {
        async fn run(&self, _workdir: &Path, _instruction: &str) -> Result<Vec<Event>> {
            Ok(vec![Event::Token("réponse ".into()), Event::Token("assistant".into()), Event::Done])
        }
    }

    #[tokio::test]
    async fn chat_persists_user_and_assistant() {
        let dir = tempfile::tempdir().unwrap();
        let store = SiloStore::open(&dir.path().join("skein.db"), "local").unwrap();
        let svc = ChatService::new(store, StubRuntime);

        let (sid, events) = svc.chat(dir.path(), None, "bonjour").await.unwrap();
        assert!(matches!(events.last().unwrap(), Event::Done));

        // Recharger depuis un nouveau store prouve la persistance.
        let store2 = SiloStore::open(&dir.path().join("skein.db"), "local").unwrap();
        let msgs = store2.load(&sid).unwrap();
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[0].text(), "bonjour");
        assert_eq!(msgs[1].role, Role::Assistant);
        assert_eq!(msgs[1].text(), "réponse assistant");
    }
}
```

- [ ] **Step 2: Déclarer le module**

Dans `crates/skein-core/src/lib.rs`, ajouter après les autres `pub mod` :
```rust
pub mod session;
```

- [ ] **Step 3: Lancer le test (échec attendu)**

Run: `cargo test -p skein-core session`
Expected: FAIL (ChatService non défini).

- [ ] **Step 4: Implémenter `ChatService`**

Ajouter au-dessus du bloc `#[cfg(test)]` dans `session.rs` :
```rust
pub struct ChatService<R: AgentRuntime> {
    store: SiloStore,
    runtime: R,
}

impl<R: AgentRuntime> ChatService<R> {
    pub fn new(store: SiloStore, runtime: R) -> Self {
        ChatService { store, runtime }
    }

    pub async fn chat(
        &self,
        workdir: &Path,
        session_id: Option<String>,
        prompt: &str,
    ) -> Result<(String, Vec<Event>)> {
        let sid = match session_id {
            Some(s) => s,
            None => self.store.create_session()?,
        };
        self.store.append(&sid, &Message::user_text(prompt))?;

        let events = self.runtime.run(workdir, prompt).await?;

        let assistant_text: String = events.iter().filter_map(|e| match e {
            Event::Token(t) => Some(t.as_str()),
            _ => None,
        }).collect();

        if !assistant_text.is_empty() {
            let msg = Message { role: Role::Assistant, parts: vec![Content::Text(assistant_text)] };
            self.store.append(&sid, &msg)?;
        }
        Ok((sid, events))
    }
}
```

- [ ] **Step 5: Lancer le test (vert attendu)**

Run: `cargo test -p skein-core`
Expected: PASS (toute la crate core).

- [ ] **Step 6: Commit**

```bash
git add crates/skein-core/src/session.rs crates/skein-core/src/lib.rs
git commit -m "feat(core): ChatService orchestre run + persistance user/assistant dans le silo"
```

---

### Task 7: CLI de référence (`skein chat`, `skein session list|show`)

**Files:**
- Modify: `crates/skein-cli/src/main.rs`

**Interfaces:**
- Consumes: `ChatService` (Task 6), `SiloStore` (Task 3), `GooseRuntime` (Task 5).
- Produces: binaire `skein` avec sous-commandes `chat`, `session list`, `session show`.

- [ ] **Step 1: Écrire le test rouge (E2E CLI avec faux binaire goose)**

`crates/skein-cli/tests/cli.rs` :
```rust
use assert_cmd::Command;
use std::io::Write;

fn fake_goose(dir: &std::path::Path) -> String {
    #[cfg(windows)]
    { let p = dir.join("goose.bat");
      let mut f = std::fs::File::create(&p).unwrap();
      writeln!(f, "@echo off").unwrap();
      writeln!(f, "echo assistant hello").unwrap();
      p.to_string_lossy().to_string() }
    #[cfg(not(windows))]
    { let p = dir.join("goose.sh");
      let mut f = std::fs::File::create(&p).unwrap();
      writeln!(f, "#!/bin/sh").unwrap();
      writeln!(f, "echo 'assistant hello'").unwrap();
      use std::os::unix::fs::PermissionsExt;
      let mut perms = std::fs::metadata(&p).unwrap().permissions();
      perms.set_mode(0o755); std::fs::set_permissions(&p, perms).unwrap();
      p.to_string_lossy().to_string() }
}

#[test]
fn chat_then_session_show_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let bin = fake_goose(dir.path());
    let db = dir.path().join("skein.db");

    // chat
    let mut cmd = Command::cargo_bin("skein").unwrap();
    cmd.args(["--db", db.to_str().unwrap(), "--goose-bin", &bin,
              "chat", "-t", "bonjour"])
       .current_dir(dir.path());
    let out = cmd.assert().success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("assistant hello"));
    assert!(stdout.contains("session s000001"));

    // session show
    let mut cmd2 = Command::cargo_bin("skein").unwrap();
    cmd2.args(["--db", db.to_str().unwrap(), "session", "show", "s000001"]);
    cmd2.assert().success()
        .stdout(predicates::str::contains("bonjour"))
        .stdout(predicates::str::contains("assistant hello"));
}
```

- [ ] **Step 2: Lancer le test (échec attendu)**

Run: `cargo test -p skein-cli`
Expected: FAIL (CLI ne gère pas encore les sous-commandes).

- [ ] **Step 3: Implémenter la CLI**

`crates/skein-cli/src/main.rs` :
```rust
use clap::{Parser, Subcommand};
use skein_core::runtime::GooseRuntime;
use skein_core::session::ChatService;
use skein_core::silo::SiloStore;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "skein", version)]
struct Cli {
    /// Chemin du fichier de base (silo). Défaut: ./skein.db
    #[arg(long, default_value = "skein.db")]
    db: PathBuf,
    /// Binaire Goose à invoquer. Défaut: goose
    #[arg(long, default_value = "goose")]
    goose_bin: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Dialogue avec l'agent
    Chat {
        #[arg(short = 't', long)]
        text: String,
        /// Réutiliser une session existante
        #[arg(long)]
        session: Option<String>,
    },
    /// Gestion des sessions
    #[command(subcommand)]
    Session(SessionCmd),
}

#[derive(Subcommand)]
enum SessionCmd {
    List,
    Show { id: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    skein_core::init_tracing();
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Chat { text, session } => {
            let store = SiloStore::open(&cli.db, "local")?;
            let runtime = GooseRuntime::new(&cli.goose_bin);
            let svc = ChatService::new(store, runtime);
            let workdir = std::env::current_dir()?;
            let (sid, events) = svc.chat(&workdir, session, &text).await?;
            for ev in &events {
                match ev {
                    skein_core::event::Event::Token(t) => println!("{t}"),
                    skein_core::event::Event::ToolCall { name, .. } => println!("[tool] {name}"),
                    skein_core::event::Event::Error(e) => eprintln!("[erreur] {e}"),
                    skein_core::event::Event::Done => {}
                }
            }
            println!("session {sid}");
        }
        Cmd::Session(SessionCmd::List) => {
            let store = SiloStore::open(&cli.db, "local")?;
            for id in store.list_sessions()? {
                println!("{id}");
            }
        }
        Cmd::Session(SessionCmd::Show { id }) => {
            let store = SiloStore::open(&cli.db, "local")?;
            for m in store.load(&id)? {
                println!("{:?}: {}", m.role, m.text());
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Lancer le test (vert attendu)**

Run: `cargo test -p skein-cli`
Expected: PASS (chat écrit la sortie + `session s000001`, `session show` recharge user+assistant).

- [ ] **Step 5: Vérifier fmt + clippy**

Run: `cargo fmt --all && cargo clippy --all-targets -- -D warnings`
Expected: aucun warning.

- [ ] **Step 6: Commit**

```bash
git add crates/skein-cli/src/main.rs crates/skein-cli/tests/cli.rs
git commit -m "feat(cli): commandes chat + session list/show (client de référence)"
```

---

### Task 8: Ledger d'exécution (event-sourced) — capture & inspection

**Files:**
- Create: `crates/skein-core/src/ledger.rs`
- Modify: `crates/skein-core/src/lib.rs` (ajouter `pub mod ledger;`), `crates/skein-core/Cargo.toml` (ajouter `sha2`), `crates/skein-cli/src/main.rs` (câbler l'append + sous-commande `ledger`)

**Interfaces:**
- Consumes: `SkeinError`/`Result` (Task 2), `SiloStore` DB (Task 3, même fichier), `ChatService::chat` sortie `(sid, events)` (Task 6).
- Produces:
  - `enum StepKind { LlmRequest, LlmResponse, ToolCall, ToolResult, StateChange }`
  - `struct Step { id: String, parent: Option<String>, seq: i64, kind: StepKind, payload: String }`
  - `struct LedgerStore` avec `open(path, namespace)`, `append(session_id, kind, payload) -> Result<String>` (retourne l'id = hash chaîné), `log(session_id) -> Result<Vec<Step>>`, `show(id) -> Result<Step>`.

- [ ] **Step 1: Ajouter la dépendance de hachage**

Dans `crates/skein-core/Cargo.toml`, section `[dependencies]`, ajouter :
```toml
sha2 = "0.10"
```

- [ ] **Step 2: Écrire le test rouge (append-only + chaînage par hachage)**

`crates/skein-core/src/ledger.rs` :
```rust
use crate::error::{Result, SkeinError};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_chains_by_hash_and_is_ordered() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("skein.db");
        let led = LedgerStore::open(&db, "local").unwrap();

        let id1 = led.append("s1", StepKind::LlmRequest, "prompt exact").unwrap();
        let id2 = led.append("s1", StepKind::LlmResponse, "réponse brute").unwrap();

        let steps = led.log("s1").unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].parent, None);
        assert_eq!(steps[1].parent.as_deref(), Some(id1.as_str()));
        assert_eq!(steps[1].id, id2);

        // Le show restitue le payload EXACT (in/out), pas seulement un résultat.
        assert_eq!(led.show(&id1).unwrap().payload, "prompt exact");
        assert_eq!(led.show(&id2).unwrap().payload, "réponse brute");
    }

    #[test]
    fn ledger_respects_namespace_isolation() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("skein.db");
        let local = LedgerStore::open(&db, "local").unwrap();
        local.append("s1", StepKind::LlmRequest, "secret").unwrap();
        let remote = LedgerStore::open(&db, "remote").unwrap();
        assert!(remote.log("s1").unwrap().is_empty());
    }
}
```

- [ ] **Step 3: Lancer le test (échec attendu)**

Run: `cargo test -p skein-core ledger`
Expected: FAIL (LedgerStore non défini).

- [ ] **Step 4: Implémenter `LedgerStore`**

Ajouter au-dessus du bloc `#[cfg(test)]` :
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind { LlmRequest, LlmResponse, ToolCall, ToolResult, StateChange }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    pub parent: Option<String>,
    pub seq: i64,
    pub kind: StepKind,
    pub payload: String,
}

pub struct LedgerStore {
    conn: Connection,
    namespace: String,
}

impl LedgerStore {
    pub fn open(path: &Path, namespace: &str) -> Result<LedgerStore> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS ledger (
                id TEXT NOT NULL,
                namespace TEXT NOT NULL,
                session_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                parent TEXT,
                kind TEXT NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY (id, namespace)
            );",
        )?;
        Ok(LedgerStore { conn, namespace: namespace.to_string() })
    }

    pub fn append(&self, session_id: &str, kind: StepKind, payload: &str) -> Result<String> {
        let (seq, parent): (i64, Option<String>) = {
            let count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM ledger WHERE session_id = ?1 AND namespace = ?2",
                rusqlite::params![session_id, self.namespace],
                |r| r.get(0),
            )?;
            let parent = if count == 0 {
                None
            } else {
                Some(self.conn.query_row(
                    "SELECT id FROM ledger WHERE session_id = ?1 AND namespace = ?2 ORDER BY seq DESC LIMIT 1",
                    rusqlite::params![session_id, self.namespace],
                    |r| r.get::<_, String>(0),
                )?)
            };
            (count, parent)
        };

        // id = hash de (parent + kind + payload) → adressage de contenu chaîné (comme un commit).
        let kind_str = serde_json::to_string(&kind)?;
        let mut hasher = Sha256::new();
        hasher.update(parent.as_deref().unwrap_or("").as_bytes());
        hasher.update(kind_str.as_bytes());
        hasher.update(payload.as_bytes());
        let id = format!("{:x}", hasher.finalize());

        self.conn.execute(
            "INSERT INTO ledger (id, namespace, session_id, seq, parent, kind, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, self.namespace, session_id, seq, parent, kind_str, payload],
        )?;
        Ok(id)
    }

    pub fn log(&self, session_id: &str) -> Result<Vec<Step>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parent, seq, kind, payload FROM ledger
             WHERE session_id = ?1 AND namespace = ?2 ORDER BY seq",
        )?;
        let rows = stmt.query_map(rusqlite::params![session_id, self.namespace], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?,
                r.get::<_, i64>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, parent, seq, kind, payload) = row?;
            out.push(Step { id, parent, seq, kind: serde_json::from_str(&kind)?, payload });
        }
        Ok(out)
    }

    pub fn show(&self, id: &str) -> Result<Step> {
        self.conn.query_row(
            "SELECT id, parent, seq, kind, payload FROM ledger WHERE id = ?1 AND namespace = ?2",
            rusqlite::params![id, self.namespace],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?,
                    r.get::<_, i64>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?)),
        )
        .map_err(|_| SkeinError::NotFound(format!("step {id}")))
        .and_then(|(id, parent, seq, kind, payload)| {
            Ok(Step { id, parent, seq, kind: serde_json::from_str(&kind)?, payload })
        })
    }
}
```

- [ ] **Step 5: Déclarer le module**

Dans `crates/skein-core/src/lib.rs`, ajouter :
```rust
pub mod ledger;
```

- [ ] **Step 6: Lancer les tests (vert attendu)**

Run: `cargo test -p skein-core ledger`
Expected: PASS (chaînage par hachage + isolation par namespace).

- [ ] **Step 7: Câbler le ledger dans la CLI (capture au niveau étape + sous-commande)**

Dans `crates/skein-cli/src/main.rs` : (a) après le `chat`, enregistrer le prompt (LlmRequest) et la réponse assistant (LlmResponse) ; (b) ajouter `ledger log|show`.

Ajouter dans l'`enum Cmd` :
```rust
    /// Journal d'exécution (façon git)
    #[command(subcommand)]
    Ledger(LedgerCmd),
```
Ajouter :
```rust
#[derive(clap::Subcommand)]
enum LedgerCmd {
    Log { session: String },
    Show { id: String },
}
```
Dans le bras `Cmd::Chat`, après avoir obtenu `(sid, events)` et **avant** le `println!("session {sid}")`, insérer :
```rust
            let ledger = skein_core::ledger::LedgerStore::open(&cli.db, "local")?;
            ledger.append(&sid, skein_core::ledger::StepKind::LlmRequest, &text)?;
            let assistant: String = events.iter().filter_map(|e| match e {
                skein_core::event::Event::Token(t) => Some(t.as_str()),
                _ => None,
            }).collect::<Vec<_>>().join("\n");
            ledger.append(&sid, skein_core::ledger::StepKind::LlmResponse, &assistant)?;
```
Ajouter les bras de commande :
```rust
        Cmd::Ledger(LedgerCmd::Log { session }) => {
            let ledger = skein_core::ledger::LedgerStore::open(&cli.db, "local")?;
            for s in ledger.log(&session)? {
                println!("{} {:?} [{}]", &s.id[..12.min(s.id.len())], s.kind, s.payload.len());
            }
        }
        Cmd::Ledger(LedgerCmd::Show { id }) => {
            let ledger = skein_core::ledger::LedgerStore::open(&cli.db, "local")?;
            let s = ledger.show(&id)?;
            println!("{:?}\n{}", s.kind, s.payload);
        }
```

- [ ] **Step 8: Écrire le test E2E CLI du ledger**

Ajouter dans `crates/skein-cli/tests/cli.rs` :
```rust
#[test]
fn ledger_captures_prompt_and_response() {
    let dir = tempfile::tempdir().unwrap();
    let bin = fake_goose(dir.path());
    let db = dir.path().join("skein.db");

    let mut cmd = Command::cargo_bin("skein").unwrap();
    cmd.args(["--db", db.to_str().unwrap(), "--goose-bin", &bin, "chat", "-t", "question exacte"])
       .current_dir(dir.path());
    cmd.assert().success();

    // Le journal contient l'entrée ET la sortie modèle, pas juste le résultat.
    let mut cmd2 = Command::cargo_bin("skein").unwrap();
    cmd2.args(["--db", db.to_str().unwrap(), "ledger", "log", "s000001"]);
    cmd2.assert().success()
        .stdout(predicates::str::contains("LlmRequest"))
        .stdout(predicates::str::contains("LlmResponse"));
}
```

- [ ] **Step 9: Lancer les tests (vert attendu)**

Run: `cargo test -p skein-core && cargo test -p skein-cli`
Expected: PASS. Puis `cargo fmt --all && cargo clippy --all-targets -- -D warnings` sans warning.

- [ ] **Step 10: Commit**

```bash
git add crates/skein-core/src/ledger.rs crates/skein-core/src/lib.rs crates/skein-core/Cargo.toml crates/skein-cli/src/main.rs crates/skein-cli/tests/cli.rs
git commit -m "feat(ledger): journal event-sourced chaîné par hachage + capture prompt/réponse + skein ledger log|show"
```

---

### Task 9: Vérification du critère de sortie Phase 0 (smoke test réel + doc)

**Files:**
- Create: `docs/superpowers/plans/phase0-smoke-test.md`

**But :** valider le critère de sortie du spec (§8 Phase 0) de bout en bout avec un **vrai** modèle local, et documenter la procédure. (Les tests automatisés couvrent les composants ; le run agentique réel est non déterministe → smoke test manuel documenté.)

- [ ] **Step 1: Documenter les prérequis**

Écrire `docs/superpowers/plans/phase0-smoke-test.md` avec :
- Installer Ollama + `ollama pull llama3.1`.
- Installer LiteLLM (`pip install litellm`) et démarrer : `litellm --config config/litellm.config.yaml` (écoute sur `:4000`).
- Configurer Goose pour utiliser un provider OpenAI-compatible `http://localhost:4000/v1` (clé `sk-skein-local`), extension developer/filesystem activée (réf. ADR 0001).

- [ ] **Step 2: Exécuter le scénario de bout en bout**

Documenter et exécuter :
```bash
cargo build --release
./target/release/skein chat -t "Crée un fichier hello.txt contenant le mot skein"
```
Attendu : Goose (via LiteLLM+Ollama) crée `hello.txt` ; la CLI affiche la sortie + `session s000001`.

- [ ] **Step 3: Vérifier la persistance et l'isolation**

```bash
cat hello.txt                                   # contient "skein"
./target/release/skein session list             # liste s000001
./target/release/skein session show s000001      # montre user + assistant
```
Consigner les résultats dans le doc (capture de sortie).

- [ ] **Step 4: Vérifier le Ledger (transparence in/out) + capture token-level via Gateway**

```bash
./target/release/skein ledger log s000001    # montre LlmRequest ET LlmResponse
./target/release/skein ledger show <id>       # affiche le contenu exact (in ou out)
```
Pour la capture **token-level** de l'I/O modèle réelle (au-delà du niveau étape), activer la journalisation LiteLLM (callback fichier JSONL) dans `config/litellm.config.yaml` et confirmer qu'une paire requête/réponse par appel est écrite. Consigner le format observé (il paramétrera l'ingestion Gateway→Ledger d'une phase ultérieure). *Limite Phase 0 assumée : le ledger CLI capture le niveau étape ; l'ingestion token-level complète via la Gateway est une phase suivante.*

- [ ] **Step 5: Vérifier l'egress OFF (local uniquement)**

Confirmer dans `config/litellm.config.yaml` qu'aucun provider cloud n'est listé ; le run fonctionne hors ligne (couper le réseau et refaire le Step 2). Consigner.

- [ ] **Step 6: Commit**

```bash
git add docs/superpowers/plans/phase0-smoke-test.md
git commit -m "docs: procédure et résultats du smoke test Phase 0 (critère de sortie)"
```

---

## Self-Review (couverture du spec pour la Phase 0)

- **Cœur headless + contrat d'événements** → Tasks 2 (Event), 6 (ChatService), 7 (CLI consomme le core). ✅
- **CLI = client de référence** → Task 7. ✅
- **1 provider via LiteLLM** → Task 4 + Task 9 (Ollama). ✅
- **Connecteur filesystem** → assuré par l'extension developer/filesystem de **Goose** (Task 0 spike + Task 9) ; Skein orchestre. ✅
- **Persistance silo Local** → Task 3 + Task 6. ✅
- **Isolation par silo** → Task 3 (test `namespaces_are_isolated`) + Task 8 (test ledger namespace). ✅
- **Ledger event-sourced dès v1** (§4.11 : capture in/out modèles, inspectable) → Task 8 (`LedgerStore` + `skein ledger log|show`) + Task 9 Step 4. ✅
- **Egress OFF / local-first** → Task 4 (config locale) + Task 9 Step 5. ✅
- **Observabilité dès v1** → `init_tracing` (Task 1) + `tracing::info!` (Task 5). ✅
- **Décision Goose (dépendance upstream)** → Task 0 (ADR). ✅
- **Critère de sortie Phase 0** (conversation qui lit/écrit un fichier, persistée & rechargée) → Task 9. ✅

**Hors périmètre Phase 0 (phases suivantes, non couverts ici volontairement)** : UI Tauri, sidecar Python/RAG, modes Serveur/Remote, RBAC/IdP, connecteurs Atlassian/M365, skills BMAD/Spec-Kit, multimodal v2+, **gestion des secrets `SecretProvider`** (§7.13 du spec — arrive avec les premiers secrets réels : providers cloud & connecteurs ; la Phase 0 n'utilise qu'Ollama local sans clé). Le principe *référence-pas-valeur* + rédaction du ledger est néanmoins un invariant dès qu'un secret existe. Chacun aura son propre plan.

## Notes de risque (Phase 0)

- **Flags CLI Goose** : la Task 5 suppose `run --no-session -t <texte>` ; l'ADR (Task 0) confirme/corrige `extra_args`. Si l'API diffère, seul ce vecteur d'arguments change.
- **Goose ↔ LiteLLM** : la configuration du provider OpenAI-compatible côté Goose est faite via la config Goose (Task 8), pas dans notre code — validée au smoke test.
