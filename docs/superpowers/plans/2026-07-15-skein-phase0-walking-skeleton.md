# Skein — Phase 0: Vertical Slice (Walking Skeleton) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove Skein's complete vertical slice: `skein` CLI → headless core → Goose (dependency, headless mode) → LiteLLM Gateway → model, with session persistence in the **Local silo** and reload.

**Architecture:** Headless core in Rust (crate `skein-core`) exposing a programmatic API; reference CLI (`skein-cli`). Goose is integrated as an **upstream dependency** via its headless CLI (`goose run`) wrapped behind our `AgentRuntime` trait (anti-corruption layer). LiteLLM serves as an OpenAI-compatible gateway (`:4000/v1`) to a local model (Ollama by default). Persistence is handled by a SQLite store **namespaced per silo** (`local`).

**Tech Stack:** Rust (2021 edition), Cargo workspace · `tokio` (async) · `rusqlite` (SQLite) · `reqwest` (HTTP) · `serde`/`serde_json` · `clap` (CLI) · `thiserror`/`anyhow` · `tracing` + `tracing-subscriber` (observability) · `wiremock` + `assert_cmd` + `tempfile` (tests) · Goose CLI (external binary) · LiteLLM (external Python proxy).

## Global Constraints

- **First-class cross-platform: Windows + macOS + Linux** (on equal footing). Green CI required on all three before merge. No hard-coded paths — use `std::path`; no OS-specific call without `#[cfg(...)]` + an equivalent on the others. Already multi-OS dependencies: `rusqlite` (bundled), `keyring`, `enigo`/`xcap` (v3), Tauri.
- **Local-first**: Phase 0 = **`local` mode only**. **Network egress OFF by default** → the Gateway points to a **local** model (Ollama). No cloud calls.
- **Per-silo isolation**: all persisted data is prefixed/namespaced by the silo (`local` in Phase 0). No cross-silo reads.
- **Observability from v1**: each crate initializes `tracing`; key events (run startup, tool-call, persistence) are traced.
- **Ledger from v1 (event sourcing)**: each step (prompt sent / response received) is captured in an **append-only, hash-chained** ledger (§4.11 of the spec). Phase 0 = capture *at the step level* + inspection (`skein ledger log|show`); revert/branch/token-level capture via the Gateway = later phases.
- **Secrets by reference from Phase 0**: never a cleartext secret in the config/code; **just-in-time** resolution via `SecretProvider` (OS keychain back-end in Phase 0), value zeroized after use, **redaction before logging** (§7.13 of the spec).
- **Quality**: `cargo fmt`, `cargo clippy -D warnings`, `cargo test` must pass. Strict TDD (red test → code → green test → commit).
- **Commits**: Conventional Commits. Commit frequently (at each task at minimum).
- **Rust edition**: 2021. **MSRV**: 1.79.
- **Binary names**: `skein` (CLI). **Crates**: `skein-core`, `skein-cli`.

---

## File Structure (Phase 0)

```
skein/
├─ Cargo.toml                      # workspace
├─ rust-toolchain.toml             # pins the toolchain
├─ .github/workflows/ci.yml        # CI (fmt, clippy, test)
├─ crates/
│  ├─ skein-core/
│  │  ├─ Cargo.toml
│  │  └─ src/
│  │     ├─ lib.rs                 # re-exports + init tracing
│  │     ├─ content.rs             # Content, Message, Role
│  │     ├─ event.rs               # Event (typed event stream)
│  │     ├─ silo.rs                # SiloStore (SQLite, namespaced)
│  │     ├─ gateway.rs             # GatewayClient (OpenAI-compat)
│  │     ├─ runtime.rs             # AgentRuntime trait + GooseRuntime
│  │     ├─ ledger.rs              # LedgerStore (append-only, hash-chained)
│  │     ├─ secrets.rs             # SecretProvider + OsKeychain + redact (JIT resolution)
│  │     └─ error.rs               # SkeinError
│  └─ skein-cli/
│     ├─ Cargo.toml
│     └─ src/main.rs               # commands: chat, session list, session show
├─ config/
│  └─ litellm.config.yaml          # Gateway → Ollama (local)
└─ docs/superpowers/
   ├─ specs/2026-07-15-skein-design.md
   └─ plans/2026-07-15-skein-phase0-walking-skeleton.md
```

---

### Task 0: Goose Integration Spike (architecture decision)

**Purpose:** Confirm through evidence the best path for integrating Goose and lock down the actual headless CLI flags. Produces an ADR (decision), not production code.

**Files:**
- Create: `docs/superpowers/adr/0001-goose-integration.md`

- [ ] **Step 1: Install/obtain the Goose binary**

Follow the official documentation (https://block-goose.mintlify.app/). Verify:
```bash
goose --version
goose --help
goose run --help
```
Note the presence of the flags: `-t/--text`, `-i/--instructions`, `--no-session`, provider/model selection, extension activation (developer/filesystem).

- [ ] **Step 2: Test a headless run with an OpenAI-compatible provider**

Configure Goose to point to a local OpenAI-compatible endpoint (this will be LiteLLM in Task 4; here a direct Ollama is enough for the spike). Run:
```bash
goose run --no-session -t "Write the text 'skein-ok' to a file named probe.txt in the current directory"
```
Verify that `probe.txt` is created (Goose's developer/filesystem extension is acting).

- [ ] **Step 3: Evaluate the integration paths against the loop-ownership requirement (ADR 0002 D1)**

**Hard requirement (from adversarial review):** Skein must OWN the reason→act→observe loop so that (a) `LoopController` can enforce termination/budgets per step, (b) the Ledger can capture exact per-turn model I/O with a propagated `trace_id`, and (c) tool calls/results are captured as ground truth. A `goose run` **CLI subprocess runs its own opaque loop → it CANNOT satisfy (a)(b)(c)** and is therefore rejected for the core loop. Evaluate, with evidence:
1. **`goosed` HTTP/streaming API** (`:3000`) — can Skein drive one turn at a time and read per-turn model I/O? Map the request/response schema and confirm a correlation id can be threaded.
2. **Embedded `goose` crate** — can Skein call a single model+tool turn and own iteration itself? Preferred if the API exposes turn-level primitives.
3. **Skein-hosted MCP proxy** — route Goose's tool traffic through a Skein MCP endpoint so `ToolCall`/`ToolResult` become Ledger events regardless of path.
(CLI subprocess remains acceptable ONLY for a throwaway smoke check, never as the core runtime.)

- [ ] **Step 4: Write the ADR and decide for Phase 0**

Write `docs/superpowers/adr/0001-goose-integration.md`: context, options, decision. **Expected decision (per ADR 0002 D1): Skein owns the loop** — Goose as a per-turn / tool executor via goosed or the embedded crate (whichever exposes turn-level model I/O + correlation), with tool traffic via the MCP proxy. Record the exact API surface + how `trace_id` is propagated (these parameterize Task 5). If neither path exposes turn-level I/O, escalate: the loop-ownership promises (LoopController, per-step Ledger) must be re-scoped before proceeding.

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/adr/0001-goose-integration.md
git commit -m "docs(adr): 0001 Goose integration strategy (Phase 0 = CLI subprocess)"
```

---

### Task 1: Cargo Workspace Scaffolding + CI

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `crates/skein-core/Cargo.toml`, `crates/skein-core/src/lib.rs`, `crates/skein-cli/Cargo.toml`, `crates/skein-cli/src/main.rs`, `.github/workflows/ci.yml`

**Interfaces:**
- Produces: compilable crates `skein-core` (lib) and `skein-cli` (bin `skein`).

- [ ] **Step 1: Create the workspace manifest**

`Cargo.toml`:
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

- [ ] **Step 2: Pin the toolchain**

`rust-toolchain.toml`:
```toml
[toolchain]
channel = "1.79"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 3: Create `skein-core` (minimal lib)**

`crates/skein-core/Cargo.toml`:
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

`crates/skein-core/src/lib.rs`:
```rust
//! Skein headless core.
pub mod content;
pub mod error;
pub mod event;
pub mod gateway;
pub mod runtime;
pub mod silo;

/// Initializes tracing (idempotent). Call at the startup of each surface.
pub fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .try_init();
}
```

- [ ] **Step 4: Create `skein-cli` (minimal bin)**

`crates/skein-cli/Cargo.toml`:
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

`crates/skein-cli/src/main.rs`:
```rust
fn main() {
    skein_core::init_tracing();
    println!("skein 0.0.0");
}
```

- [ ] **Step 5: Add the CI**

`.github/workflows/ci.yml` (cross-platform matrix: Windows + macOS + Linux, on equal footing):
```yaml
name: ci
on: [push, pull_request]
jobs:
  rust:
    strategy:
      fail-fast: false
      matrix:
        os: [windows-latest, macos-latest, ubuntu-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.79
        with: { components: rustfmt, clippy }
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test --all
```

- [ ] **Step 6: Verify compilation**

Run: `cargo build --all`
Expected: build OK; `cargo run -p skein-cli` prints `skein 0.0.0`.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml rust-toolchain.toml crates .github
git commit -m "chore: scaffold Cargo workspace (skein-core, skein-cli) + CI"
```

---

### Task 2: Domain Types (`Content`, `Message`, `Role`, `Event`)

**Files:**
- Create: `crates/skein-core/src/content.rs`, `crates/skein-core/src/event.rs`, `crates/skein-core/src/error.rs`

**Interfaces:**
- Produces:
  - `enum Role { User, Assistant, System }`
  - `enum Content { Text(String) }` (v2 will add Image/Audio/Doc/Video)
  - `struct Message { role: Role, parts: Vec<Content> }` + `Message::user_text(&str) -> Message`, `Message::text(&self) -> String`
  - `enum Event { Token(String), ToolCall { name: String, input: String }, Done, Error(String) }`
  - `enum SkeinError` (thiserror)

- [ ] **Step 1: Write the red test (content/message)**

`crates/skein-core/src/content.rs`:
```rust
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_text_roundtrips() {
        let m = Message::user_text("hello");
        assert_eq!(m.role, Role::User);
        assert_eq!(m.text(), "hello");
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(back.text(), "hello");
    }
}
```

- [ ] **Step 2: Run the test (failure expected)**

Run: `cargo test -p skein-core content`
Expected: FAIL (types not defined).

- [ ] **Step 3: Implement the types**

Add above the `#[cfg(test)]` block in `content.rs`:
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
    /// Concatenates all text parts.
    pub fn text(&self) -> String {
        self.parts.iter().map(|p| match p { Content::Text(t) => t.as_str() }).collect()
    }
}
```

- [ ] **Step 4: Write the errors and events**

`crates/skein-core/src/error.rs`:
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

`crates/skein-core/src/event.rs`:
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

- [ ] **Step 5: Run the tests (green expected)**

Run: `cargo test -p skein-core`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/skein-core/src/content.rs crates/skein-core/src/event.rs crates/skein-core/src/error.rs
git commit -m "feat(core): domain types Content/Message/Role/Event + errors"
```

---

### Task 3: Silo Store (SQLite, namespaced by mode)

**Files:**
- Create: `crates/skein-core/src/silo.rs`

**Interfaces:**
- Consumes: `Message` (Task 2), `SkeinError`/`Result` (Task 2).
- Produces:
  - `struct SiloStore` with `SiloStore::open(path: &Path, namespace: &str) -> Result<SiloStore>`
  - `fn create_session(&self) -> Result<String>` (returns an id)
  - `fn append(&self, session_id: &str, msg: &Message) -> Result<()>`
  - `fn load(&self, session_id: &str) -> Result<Vec<Message>>`
  - `fn list_sessions(&self) -> Result<Vec<String>>`

- [ ] **Step 1: Write the red test (persistence + isolation)**

`crates/skein-core/src/silo.rs`:
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
        store.append(&sid, &Message::user_text("hi")).unwrap();
        let msgs = store.load(&sid).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].text(), "hi");
        assert_eq!(store.list_sessions().unwrap(), vec![sid]);
    }

    #[test]
    fn namespaces_are_isolated() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("skein.db");
        let local = SiloStore::open(&db, "local").unwrap();
        let sid = local.create_session().unwrap();
        local.append(&sid, &Message::user_text("local secret")).unwrap();

        // A different namespace, same DB file, sees nothing.
        let remote = SiloStore::open(&db, "remote").unwrap();
        assert!(remote.list_sessions().unwrap().is_empty());
        assert!(remote.load(&sid).is_err());
    }
}
```

- [ ] **Step 2: Run the test (failure expected)**

Run: `cargo test -p skein-core silo`
Expected: FAIL (SiloStore not defined).

- [ ] **Step 3: Implement `SiloStore`**

Add above the `#[cfg(test)]` block:
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
        // RNG-free deterministic id: counter + namespace (no RNG required in Phase 0)
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

- [ ] **Step 4: Run the tests (green expected)**

Run: `cargo test -p skein-core silo`
Expected: PASS (both tests, including namespace isolation).

- [ ] **Step 5: Commit**

```bash
git add crates/skein-core/src/silo.rs
git commit -m "feat(core): namespaced SQLite SiloStore + cross-silo isolation test"
```

---

### Task 4: Gateway Client (OpenAI-compatible) + LiteLLM config

**Files:**
- Create: `crates/skein-core/src/gateway.rs`, `config/litellm.config.yaml`

**Interfaces:**
- Consumes: `SkeinError`/`Result` (Task 2).
- Produces:
  - `struct GatewayClient { base_url: String, api_key: String, http: reqwest::Client }`
  - `GatewayClient::new(base_url: &str, api_key: &str) -> GatewayClient`
  - `async fn health(&self) -> Result<bool>` (GET `{base_url}/models`)
  - `async fn complete(&self, model: &str, prompt: &str) -> Result<String>` (POST `{base_url}/chat/completions`)

- [ ] **Step 1: Write the red test (against a wiremock stub server)**

`crates/skein-core/src/gateway.rs`:
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

- [ ] **Step 2: Run the test (failure expected)**

Run: `cargo test -p skein-core gateway`
Expected: FAIL (GatewayClient not defined).

- [ ] **Step 3: Implement `GatewayClient`**

Add above the `#[cfg(test)]` block:
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
            .ok_or_else(|| SkeinError::Gateway("response has no content".into()))
    }
}
```

- [ ] **Step 4: Write the LiteLLM config (Gateway → local Ollama)**

`config/litellm.config.yaml`:
```yaml
model_list:
  - model_name: local-model
    litellm_params:
      model: ollama/llama3.1
      api_base: http://localhost:11434
general_settings:
  master_key: sk-skein-local
```

- [ ] **Step 5: Run the tests (green expected)**

Run: `cargo test -p skein-core gateway`
Expected: PASS (both tests, without a real network — wiremock).

- [ ] **Step 6: Commit**

```bash
git add crates/skein-core/src/gateway.rs config/litellm.config.yaml
git commit -m "feat(core): OpenAI-compat GatewayClient + local LiteLLM config"
```

---

### Task 5: Agentic Runtime (Goose per-turn executor adapter)

> ⚠️ **Re-scoped by ADR 0002 (D1).** The batch `Command::output()` subprocess shown below is a **stub for the stub-binary test only**. The real `GooseRuntime` must be a **per-turn executor** (goosed/embedded, per the T000 spike) that (a) returns a **streaming** `EventStream` (not `Vec<Event>` collected after exit), (b) emits per-turn `Event`s so the `LoopController` (Epic 6) and the Ledger see each step, and (c) propagates a `trace_id`. The `AgentRuntime` trait signature should be `fn run(...) -> EventStream` accordingly. Keep the stub-binary unit test (it validates process wiring), but do not ship the batch design as the core runtime.

**Files:**
- Create: `crates/skein-core/src/runtime.rs`

**Interfaces:**
- Consumes: `Event` (Task 2), `SkeinError`/`Result` (Task 2). CLI flags confirmed by the ADR (Task 0).
- Produces:
  - `trait AgentRuntime { async fn run(&self, workdir: &Path, instruction: &str) -> Result<Vec<Event>>; }`
  - `struct GooseRuntime { bin: String, extra_args: Vec<String> }`
  - `GooseRuntime::new(bin: &str) -> GooseRuntime`
  - Impl `AgentRuntime for GooseRuntime`: runs `<bin> run --no-session -t <instruction>` in `workdir`, maps stdout → `Event::Token`, non-zero exit code → `Event::Error`, end → `Event::Done`.

- [ ] **Step 1: Write the red test (with a fake Goose binary)**

`crates/skein-core/src/runtime.rs`:
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

    // Creates a fake "goose" binary that writes to a file then prints a line.
    fn fake_goose(dir: &Path) -> String {
        #[cfg(windows)]
        {
            let p = dir.join("goose.bat");
            let mut f = std::fs::File::create(&p).unwrap();
            // %* = all args; we just prove the binary is invoked and writes a file.
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

- [ ] **Step 2: Run the test (failure expected)**

Run: `cargo test -p skein-core runtime`
Expected: FAIL (GooseRuntime not defined).

- [ ] **Step 3: Implement `GooseRuntime`**

Add above the `#[cfg(test)]` block:
```rust
pub struct GooseRuntime {
    bin: String,
    /// Flags confirmed by ADR 0001 (Task 0). Default: headless run without a session.
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

- [ ] **Step 4: Run the test (green expected)**

Run: `cargo test -p skein-core runtime`
Expected: PASS (the fake binary writes `probe.txt`, tokens are captured, `Done` at the end).

- [ ] **Step 5: Commit**

```bash
git add crates/skein-core/src/runtime.rs
git commit -m "feat(core): AgentRuntime + GooseRuntime (headless CLI adapter, tested via stub)"
```

---

### Task 6: Core Orchestration (`chat`: run + silo persistence)

**Files:**
- Modify: `crates/skein-core/src/lib.rs` (add `pub mod session;`)
- Create: `crates/skein-core/src/session.rs`

**Interfaces:**
- Consumes: `SiloStore` (Task 3), `AgentRuntime` (Task 5), `Message`/`Event` (Task 2).
- Produces:
  - `struct ChatService<R: AgentRuntime> { store: SiloStore, runtime: R }`
  - `ChatService::new(store: SiloStore, runtime: R) -> Self`
  - `async fn chat(&self, workdir: &Path, session_id: Option<String>, prompt: &str) -> Result<(String, Vec<Event>)>` — creates/loads the session, persists the user message, runs the runtime, persists the assistant response (text concatenated from the `Event::Token`s), returns `(session_id, events)`.

- [ ] **Step 1: Write the red test (orchestration + persistence)**

`crates/skein-core/src/session.rs`:
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
            Ok(vec![Event::Token("response ".into()), Event::Token("assistant".into()), Event::Done])
        }
    }

    #[tokio::test]
    async fn chat_persists_user_and_assistant() {
        let dir = tempfile::tempdir().unwrap();
        let store = SiloStore::open(&dir.path().join("skein.db"), "local").unwrap();
        let svc = ChatService::new(store, StubRuntime);

        let (sid, events) = svc.chat(dir.path(), None, "hello").await.unwrap();
        assert!(matches!(events.last().unwrap(), Event::Done));

        // Reloading from a new store proves persistence.
        let store2 = SiloStore::open(&dir.path().join("skein.db"), "local").unwrap();
        let msgs = store2.load(&sid).unwrap();
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[0].text(), "hello");
        assert_eq!(msgs[1].role, Role::Assistant);
        assert_eq!(msgs[1].text(), "response assistant");
    }
}
```

- [ ] **Step 2: Declare the module**

In `crates/skein-core/src/lib.rs`, add after the other `pub mod`s:
```rust
pub mod session;
```

- [ ] **Step 3: Run the test (failure expected)**

Run: `cargo test -p skein-core session`
Expected: FAIL (ChatService not defined).

- [ ] **Step 4: Implement `ChatService`**

Add above the `#[cfg(test)]` block in `session.rs`:
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

- [ ] **Step 5: Run the test (green expected)**

Run: `cargo test -p skein-core`
Expected: PASS (the entire core crate).

- [ ] **Step 6: Commit**

```bash
git add crates/skein-core/src/session.rs crates/skein-core/src/lib.rs
git commit -m "feat(core): ChatService orchestrates run + user/assistant persistence in the silo"
```

---

### Task 7: Reference CLI (`skein chat`, `skein session list|show`)

**Files:**
- Modify: `crates/skein-cli/src/main.rs`

**Interfaces:**
- Consumes: `ChatService` (Task 6), `SiloStore` (Task 3), `GooseRuntime` (Task 5).
- Produces: `skein` binary with the `chat`, `session list`, `session show` subcommands.

- [ ] **Step 1: Write the red test (CLI E2E with fake goose binary)**

`crates/skein-cli/tests/cli.rs`:
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
              "chat", "-t", "hello"])
       .current_dir(dir.path());
    let out = cmd.assert().success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("assistant hello"));
    assert!(stdout.contains("session s000001"));

    // session show
    let mut cmd2 = Command::cargo_bin("skein").unwrap();
    cmd2.args(["--db", db.to_str().unwrap(), "session", "show", "s000001"]);
    cmd2.assert().success()
        .stdout(predicates::str::contains("hello"))
        .stdout(predicates::str::contains("assistant hello"));
}
```

- [ ] **Step 2: Run the test (failure expected)**

Run: `cargo test -p skein-cli`
Expected: FAIL (the CLI does not handle the subcommands yet).

- [ ] **Step 3: Implement the CLI**

`crates/skein-cli/src/main.rs`:
```rust
use clap::{Parser, Subcommand};
use skein_core::runtime::GooseRuntime;
use skein_core::session::ChatService;
use skein_core::silo::SiloStore;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "skein", version)]
struct Cli {
    /// Path to the base file (silo). Default: ./skein.db
    #[arg(long, default_value = "skein.db")]
    db: PathBuf,
    /// Goose binary to invoke. Default: goose
    #[arg(long, default_value = "goose")]
    goose_bin: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Converse with the agent
    Chat {
        #[arg(short = 't', long)]
        text: String,
        /// Reuse an existing session
        #[arg(long)]
        session: Option<String>,
    },
    /// Session management
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
                    skein_core::event::Event::Error(e) => eprintln!("[error] {e}"),
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

- [ ] **Step 4: Run the test (green expected)**

Run: `cargo test -p skein-cli`
Expected: PASS (chat writes the output + `session s000001`, `session show` reloads user+assistant).

- [ ] **Step 5: Verify fmt + clippy**

Run: `cargo fmt --all && cargo clippy --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/skein-cli/src/main.rs crates/skein-cli/tests/cli.rs
git commit -m "feat(cli): chat + session list/show commands (reference client)"
```

---

### Task 8: Execution Ledger (event-sourced) — capture & inspection

**Files:**
- Create: `crates/skein-core/src/ledger.rs`
- Modify: `crates/skein-core/src/lib.rs` (add `pub mod ledger;`), `crates/skein-core/Cargo.toml` (add `sha2`), `crates/skein-cli/src/main.rs` (wire up the append + `ledger` subcommand)

**Interfaces:**
- Consumes: `SkeinError`/`Result` (Task 2), `SiloStore` DB (Task 3, same file), `ChatService::chat` output `(sid, events)` (Task 6).
- Produces:
  - `enum StepKind { LlmRequest, LlmResponse, ToolCall, ToolResult, StateChange }`
  - `struct Step { id: String, parent: Option<String>, seq: i64, kind: StepKind, payload: String }`
  - `struct LedgerStore` with `open(path, namespace)`, `append(session_id, kind, payload) -> Result<String>` (returns the id = chained hash), `log(session_id) -> Result<Vec<Step>>`, `show(id) -> Result<Step>`.

- [ ] **Step 1: Add the hashing dependency**

In `crates/skein-core/Cargo.toml`, section `[dependencies]`, add:
```toml
sha2 = "0.10"
```

- [ ] **Step 2: Write the red test (append-only + hash chaining)**

`crates/skein-core/src/ledger.rs`:
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

        let id1 = led.append("s1", StepKind::LlmRequest, "exact prompt").unwrap();
        let id2 = led.append("s1", StepKind::LlmResponse, "raw response").unwrap();

        let steps = led.log("s1").unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].parent, None);
        assert_eq!(steps[1].parent.as_deref(), Some(id1.as_str()));
        assert_eq!(steps[1].id, id2);

        // show returns the EXACT payload (in/out), not just a result.
        assert_eq!(led.show(&id1).unwrap().payload, "exact prompt");
        assert_eq!(led.show(&id2).unwrap().payload, "raw response");
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

- [ ] **Step 3: Run the test (failure expected)**

Run: `cargo test -p skein-core ledger`
Expected: FAIL (LedgerStore not defined).

- [ ] **Step 4: Implement `LedgerStore`**

Add above the `#[cfg(test)]` block:
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

        // id = hash of (parent + kind + payload) → chained content addressing (like a commit).
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

- [ ] **Step 5: Declare the module**

In `crates/skein-core/src/lib.rs`, add:
```rust
pub mod ledger;
```

- [ ] **Step 6: Run the tests (green expected)**

Run: `cargo test -p skein-core ledger`
Expected: PASS (hash chaining + namespace isolation).

- [ ] **Step 7: Wire the ledger into the CLI (step-level capture + subcommand)**

In `crates/skein-cli/src/main.rs`: (a) after the `chat`, record the prompt (LlmRequest) and the assistant response (LlmResponse); (b) add `ledger log|show`.

Add to the `enum Cmd`:
```rust
    /// Execution ledger (git-style)
    #[command(subcommand)]
    Ledger(LedgerCmd),
```
Add:
```rust
#[derive(clap::Subcommand)]
enum LedgerCmd {
    Log { session: String },
    Show { id: String },
}
```
In the `Cmd::Chat` arm, after obtaining `(sid, events)` and **before** the `println!("session {sid}")`, insert:
```rust
            let ledger = skein_core::ledger::LedgerStore::open(&cli.db, "local")?;
            ledger.append(&sid, skein_core::ledger::StepKind::LlmRequest, &text)?;
            let assistant: String = events.iter().filter_map(|e| match e {
                skein_core::event::Event::Token(t) => Some(t.as_str()),
                _ => None,
            }).collect::<Vec<_>>().join("\n");
            ledger.append(&sid, skein_core::ledger::StepKind::LlmResponse, &assistant)?;
```
Add the command arms:
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

- [ ] **Step 8: Write the ledger CLI E2E test**

Add to `crates/skein-cli/tests/cli.rs`:
```rust
#[test]
fn ledger_captures_prompt_and_response() {
    let dir = tempfile::tempdir().unwrap();
    let bin = fake_goose(dir.path());
    let db = dir.path().join("skein.db");

    let mut cmd = Command::cargo_bin("skein").unwrap();
    cmd.args(["--db", db.to_str().unwrap(), "--goose-bin", &bin, "chat", "-t", "exact question"])
       .current_dir(dir.path());
    cmd.assert().success();

    // The ledger contains BOTH the model input AND output, not just the result.
    let mut cmd2 = Command::cargo_bin("skein").unwrap();
    cmd2.args(["--db", db.to_str().unwrap(), "ledger", "log", "s000001"]);
    cmd2.assert().success()
        .stdout(predicates::str::contains("LlmRequest"))
        .stdout(predicates::str::contains("LlmResponse"));
}
```

- [ ] **Step 9: Run the tests (green expected)**

Run: `cargo test -p skein-core && cargo test -p skein-cli`
Expected: PASS. Then `cargo fmt --all && cargo clippy --all-targets -- -D warnings` with no warnings.

- [ ] **Step 10: Commit**

```bash
git add crates/skein-core/src/ledger.rs crates/skein-core/src/lib.rs crates/skein-core/Cargo.toml crates/skein-cli/src/main.rs crates/skein-cli/tests/cli.rs
git commit -m "feat(ledger): hash-chained event-sourced ledger + prompt/response capture + skein ledger log|show"
```

---

### Task 9: `SecretProvider` Foundation (OS keychain, JIT resolution)

**Files:**
- Create: `crates/skein-core/src/secrets.rs`
- Modify: `crates/skein-core/src/lib.rs` (`pub mod secrets;`), `crates/skein-core/Cargo.toml` (`keyring`, `zeroize`), `crates/skein-cli/src/main.rs` (`secret set`, `gateway health` commands)

**Interfaces:**
- Consumes: `SkeinError`/`Result` (Task 2), `GatewayClient` (Task 4).
- Produces:
  - `struct SecretRef(String)` (form `keychain://service/key`)
  - `struct SecretValue` (`Debug` redacted, zeroized on drop, `expose(&self) -> &str`)
  - `trait SecretProvider { fn resolve(&self, r: &SecretRef) -> Result<SecretValue>; fn requires_network(&self) -> bool; }`
  - `struct OsKeychain` (impl `SecretProvider`, `requires_network()==false`) + `OsKeychain::store(&SecretRef, &str) -> Result<()>`
  - `fn redact(text: &str, secrets: &[&SecretValue]) -> String`

- [ ] **Step 1: Add the dependencies**

In `crates/skein-core/Cargo.toml`, `[dependencies]`:
```toml
keyring = "3"
zeroize = "1"
```

- [ ] **Step 2: Write the red test (redaction, parse, mock provider)**

`crates/skein-core/src/secrets.rs`:
```rust
use crate::error::{Result, SkeinError};
use std::fmt;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MockProvider(HashMap<String, String>);
    impl SecretProvider for MockProvider {
        fn resolve(&self, r: &SecretRef) -> Result<SecretValue> {
            self.0.get(&r.0).cloned().map(SecretValue::new)
                .ok_or_else(|| SkeinError::NotFound(r.0.clone()))
        }
        fn requires_network(&self) -> bool { false }
    }

    #[test]
    fn debug_is_redacted_and_expose_works() {
        let v = SecretValue::new("s3cr3t".into());
        assert_eq!(format!("{v:?}"), "SecretValue(***)");
        assert_eq!(v.expose(), "s3cr3t");
    }

    #[test]
    fn redact_masks_secret_in_text() {
        let v = SecretValue::new("sk-skein-local".into());
        let out = redact("call with key sk-skein-local ok", &[&v]);
        assert_eq!(out, "call with key *** ok");
    }

    #[test]
    fn keychain_ref_parses() {
        let (svc, key) = OsKeychain::parse(&SecretRef("keychain://skein/gateway-key".into())).unwrap();
        assert_eq!(svc, "skein");
        assert_eq!(key, "gateway-key");
    }

    #[test]
    fn mock_provider_resolves_jit() {
        let mut m = HashMap::new();
        m.insert("keychain://skein/gateway-key".to_string(), "sk-skein-local".to_string());
        let p = MockProvider(m);
        let v = p.resolve(&SecretRef("keychain://skein/gateway-key".into())).unwrap();
        assert_eq!(v.expose(), "sk-skein-local");
    }
}
```

- [ ] **Step 3: Run the test (failure expected)**

Run: `cargo test -p skein-core secrets`
Expected: FAIL (types not defined).

- [ ] **Step 4: Implement `secrets.rs`**

Add above the `#[cfg(test)]` block:
```rust
/// Secret reference — never the value. Form: "keychain://service/key".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRef(pub String);

/// Resolved, ephemeral value: `Debug` redacts, memory zeroized on drop.
pub struct SecretValue(String);

impl SecretValue {
    pub fn new(s: String) -> Self { SecretValue(s) }
    pub fn expose(&self) -> &str { &self.0 }
}
impl Clone for SecretValue {
    fn clone(&self) -> Self { SecretValue(self.0.clone()) }
}
impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "SecretValue(***)") }
}
impl Drop for SecretValue {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.0.zeroize();
    }
}

pub trait SecretProvider {
    /// Resolves a reference to a value, just-in-time. The value is never persisted.
    fn resolve(&self, r: &SecretRef) -> Result<SecretValue>;
    /// True if the back-end requires the network (governs the egress policy).
    fn requires_network(&self) -> bool;
}

/// OS keychain back-end (Windows Credential Manager / Keychain / secret-service).
pub struct OsKeychain;

impl OsKeychain {
    pub fn parse(r: &SecretRef) -> Result<(String, String)> {
        let rest = r.0.strip_prefix("keychain://")
            .ok_or_else(|| SkeinError::Runtime(format!("non-keychain ref: {}", r.0)))?;
        let (service, key) = rest.split_once('/')
            .ok_or_else(|| SkeinError::Runtime(format!("invalid ref: {}", r.0)))?;
        Ok((service.to_string(), key.to_string()))
    }

    pub fn store(&self, r: &SecretRef, value: &str) -> Result<()> {
        let (service, key) = Self::parse(r)?;
        let entry = keyring::Entry::new(&service, &key)
            .map_err(|e| SkeinError::Runtime(format!("keyring: {e}")))?;
        entry.set_password(value).map_err(|e| SkeinError::Runtime(format!("keyring set: {e}")))
    }
}

impl SecretProvider for OsKeychain {
    fn resolve(&self, r: &SecretRef) -> Result<SecretValue> {
        let (service, key) = Self::parse(r)?;
        let entry = keyring::Entry::new(&service, &key)
            .map_err(|e| SkeinError::Runtime(format!("keyring: {e}")))?;
        let secret = entry.get_password()
            .map_err(|e| SkeinError::NotFound(format!("secret {}: {e}", r.0)))?;
        Ok(SecretValue::new(secret))
    }
    fn requires_network(&self) -> bool { false }
}

/// Masks any occurrence of a secret in a text before logging (Ledger/logs).
pub fn redact(text: &str, secrets: &[&SecretValue]) -> String {
    let mut out = text.to_string();
    for s in secrets {
        if !s.expose().is_empty() {
            out = out.replace(s.expose(), "***");
        }
    }
    out
}
```

- [ ] **Step 5: Declare the module**

In `crates/skein-core/src/lib.rs`, add:
```rust
pub mod secrets;
```

- [ ] **Step 6: Run the tests (green expected)**

Run: `cargo test -p skein-core secrets`
Expected: PASS (all 4 tests; without touching the real keychain — mock provider).

- [ ] **Step 7: Wire the CLI (`secret set`, `gateway health` with JIT resolution)**

In `crates/skein-cli/src/main.rs`, add to the `enum Cmd`:
```rust
    /// Stores a secret in the OS keychain (ref keychain://service/key)
    SecretSet { reference: String, value: String },
    /// Checks the Gateway by resolving its key JIT from the keychain
    GatewayHealth {
        #[arg(long, default_value = "http://localhost:4000/v1")]
        base_url: String,
        #[arg(long, default_value = "keychain://skein/gateway-key")]
        key_ref: String,
    },
```
Add the corresponding arms:
```rust
        Cmd::SecretSet { reference, value } => {
            use skein_core::secrets::{OsKeychain, SecretRef};
            OsKeychain.store(&SecretRef(reference.clone()), &value)?;
            println!("secret stored: {reference}");
        }
        Cmd::GatewayHealth { base_url, key_ref } => {
            use skein_core::secrets::{OsKeychain, SecretProvider, SecretRef};
            let secret = OsKeychain.resolve(&SecretRef(key_ref))?;   // JIT resolution
            let client = skein_core::gateway::GatewayClient::new(&base_url, secret.expose());
            let ok = client.health().await?;
            println!("gateway health: {}", if ok { "OK" } else { "FAIL" });
            // `secret` is dropped here → zeroized. Never persisted, never logged in cleartext.
        }
```

- [ ] **Step 8: Verify compilation + fmt + clippy**

Run: `cargo build --all && cargo fmt --all && cargo clippy --all-targets -- -D warnings`
Expected: OK, no warnings.

- [ ] **Step 9: Commit**

```bash
git add crates/skein-core/src/secrets.rs crates/skein-core/src/lib.rs crates/skein-core/Cargo.toml crates/skein-cli/src/main.rs
git commit -m "feat(secrets): SecretProvider + OsKeychain + JIT Gateway key resolution + redact"
```

---

### Task 10: Phase 0 Exit Criterion Verification (real smoke test + doc)

**Files:**
- Create: `docs/superpowers/plans/phase0-smoke-test.md`

**Purpose:** Validate the spec's exit criterion (§8 Phase 0) end-to-end with a **real** local model, and document the procedure. (Automated tests cover the components; the real agentic run is non-deterministic → documented manual smoke test.)

- [ ] **Step 1: Document the prerequisites**

Write `docs/superpowers/plans/phase0-smoke-test.md` with:
- Install Ollama + `ollama pull llama3.1`.
- Install LiteLLM (`pip install litellm`) and start: `litellm --config config/litellm.config.yaml` (listens on `:4000`).
- Configure Goose to use an OpenAI-compatible provider `http://localhost:4000/v1` (key `sk-skein-local`), with the developer/filesystem extension enabled (ref. ADR 0001).

- [ ] **Step 2: Store the Gateway secret and verify JIT resolution**

```bash
./target/release/skein secret-set keychain://skein/gateway-key sk-skein-local
./target/release/skein gateway-health
```
Expected: `gateway health: OK`. Confirm that the key **appears in cleartext neither in the logs nor on screen** (only its JIT resolution is used). Record it.

- [ ] **Step 3: Run the end-to-end scenario**

Document and run:
```bash
cargo build --release
./target/release/skein chat -t "Create a file hello.txt containing the word skein"
```
Expected: Goose (via LiteLLM+Ollama) creates `hello.txt`; the CLI prints the output + `session s000001`.

- [ ] **Step 4: Verify persistence and isolation**

```bash
cat hello.txt                                   # contains "skein"
./target/release/skein session list             # lists s000001
./target/release/skein session show s000001      # shows user + assistant
```
Record the results in the doc (output capture).

- [ ] **Step 5: Verify the Ledger (in/out transparency) + token-level capture via Gateway**

```bash
./target/release/skein ledger log s000001    # shows LlmRequest AND LlmResponse
./target/release/skein ledger show <id>       # displays the exact content (in or out)
```
For **token-level** capture of the real model I/O (beyond the step level), enable LiteLLM logging (JSONL file callback) in `config/litellm.config.yaml` and confirm that one request/response pair per call is written. Record the observed format (it will parameterize the Gateway→Ledger ingestion of a later phase). *Assumed Phase 0 limitation: the CLI ledger captures the step level; full token-level ingestion via the Gateway is a later phase.*

- [ ] **Step 6: Verify egress OFF (local only)**

Confirm in `config/litellm.config.yaml` that no cloud provider is listed; the run works offline (cut the network and redo Step 3). Since the `OsKeychain` secret back-end is offline (`requires_network()==false`), JIT resolution also works without a network. Record it.

- [ ] **Step 7: Commit**

```bash
git add docs/superpowers/plans/phase0-smoke-test.md
git commit -m "docs: Phase 0 smoke test procedure and results (exit criterion)"
```

---

## Self-Review (spec coverage for Phase 0)

- **Headless core + event contract** → Tasks 2 (Event), 6 (ChatService), 7 (CLI consumes the core). ✅
- **CLI = reference client** → Task 7. ✅
- **1 provider via LiteLLM** → Task 4 + Task 10 (Ollama). ✅
- **Filesystem connector** → provided by **Goose**'s developer/filesystem extension (Task 0 spike + Task 10); Skein orchestrates. ✅
- **Local silo persistence** → Task 3 + Task 6. ✅
- **Per-silo isolation** → Task 3 (`namespaces_are_isolated` test) + Task 8 (ledger namespace test). ✅
- **Event-sourced ledger from v1** (§4.11: captures model in/out, inspectable) → Task 8 (`LedgerStore` + `skein ledger log|show`) + Task 10 Step 5. ✅
- **Secret management `SecretProvider` (foundation)** (§7.13: reference-not-value, JIT resolution, redaction) → Task 9 (`SecretProvider`/`OsKeychain`/`redact` + `skein secret-set`/`gateway-health`) + Task 10 Step 2. ✅
- **Egress OFF / local-first** → Task 4 (local config) + Task 9 (`requires_network()`) + Task 10 Step 6. ✅
- **Observability from v1** → `init_tracing` (Task 1) + `tracing::info!` (Task 5). ✅
- **Goose decision (upstream dependency)** → Task 0 (ADR). ✅
- **Phase 0 exit criterion** (a conversation that reads/writes a file, persisted & reloaded) → Task 10. ✅

**Out of Phase 0 scope (later phases, intentionally not covered here)**: Tauri UI, Python/RAG sidecar, Server/Remote modes, RBAC/IdP, Atlassian/M365 connectors, BMAD/Spec-Kit skills, multimodal v2+. For **secrets**: only the **foundation** (`SecretProvider` + `OsKeychain` + redaction, Task 9) is in Phase 0; the **SOPS+age / 1Password / OpenBao / Infisical** back-ends (§7.13) arrive with the cloud providers & connectors. Each will have its own plan.

## Risk Notes (Phase 0)

- **Goose CLI flags**: Task 5 assumes `run --no-session -t <text>`; the ADR (Task 0) confirms/corrects `extra_args`. If the API differs, only this argument vector changes.
- **Goose ↔ LiteLLM**: the OpenAI-compatible provider configuration on the Goose side is done via the Goose config (Task 8), not in our code — validated at the smoke test.
