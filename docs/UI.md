# The Heddle desktop UI

One window, one screen: **Chat**. It is a Tauri app — a Rust shell (`ui/src-tauri`) around a TypeScript frontend (`ui/src`) — and it is deliberately the thinnest thing that can be called a UI.

**The window has no capability of its own.** It does not talk to a model, does not run the loop, does not own a tool gateway and does not touch a silo. It spawns `heddle acp-agent` as a child process and speaks the Agent Client Protocol to it — the same protocol, over the same stdio transport, that an ACP-speaking editor uses, and that `crates/heddle-cli/tests/cli_acp_agent.rs` already drives against the same binary. Every button below is one ACP call the CLI already serves (Constitution I).

---

## Prerequisites

Everything here is already installed by `scripts/bootstrap.ps1` / `scripts/bootstrap.sh` — see [docs/DEVELOPMENT.md](DEVELOPMENT.md). Nothing in this slice adds a new bootstrap requirement.

| Need | Why |
|---|---|
| **Rust 1.97** (pinned by `rust-toolchain.toml`) | Builds the shell. Tauri v2's MSRV is 1.77.2, comfortably under the pin. |
| **Node LTS** | Builds the frontend. Already a bootstrap dependency (`docs/DEVELOPMENT.md`). |
| **A local model provider** | `--with-ollama` during bootstrap, or anything OpenAI-compatible on loopback. `heddle-gateway` compiles `ureq` with no TLS backend, so **loopback `http://` is the only thing it can reach** (Constitution II). |
| **A webview** | WebView2 on Windows (present on Windows 10/11), WebKitGTK on Linux (`libwebkit2gtk-4.1-dev`), system WebKit on macOS. |

---

## Configure it

v0 has no settings screen and no config file. It reads the environment, and it **refuses to start naming the variable** when a required one is missing — for the same reason `heddle --root` refuses to guess a silo root: guessing would put an agent's journal somewhere the operator did not name.

| Variable | Required | Becomes | Notes |
|---|---|---|---|
| `HEDDLE_ROOT` | **yes** | `--root` | The directory holding the silos. |
| `HEDDLE_UI_MODEL` | **yes** | `--model` | No default, exactly as the CLI has none. |
| `HEDDLE_UI_SILO` | no | `--silo` | Defaults to `ui`. |
| `HEDDLE_MODEL_BASE_URL` | no | `--base-url` | Omitted entirely when unset, so the CLI's own default applies. |
| `HEDDLE_UI_FS_ROOT` | no | `--fs-root` | **Absent means the session has no tools at all** — `crates/heddle-cli/src/wiring.rs`'s "no root, no tools". |
| `HEDDLE_UI_BIN` | no | — | Path to the `heddle` binary. Defaults to the one beside the app's own executable. |

---

## Run it

```bash
cd ui
npm install
npm run tauri dev
```

`npm run tauri dev` starts Vite, builds the shell, and opens the window. A first run compiles the whole Tauri dependency tree and takes a while.

For a production frontend bundle plus a release shell:

```bash
cd ui
npm run build          # tsc --noEmit && vite build  ->  ui/dist
cargo build -p heddle-ui --release
```

> `ui/dist` must exist before `cargo build -p heddle-ui`: `tauri.conf.json`'s `frontendDist` points at it and `tauri-build` reads it at compile time. `npm run build` first, always. `.github/workflows/core.yml` does exactly that, in that order.

The window expects the `heddle` binary beside its own executable. In a `cargo build` layout that is already true (`target/debug/heddle` and `target/debug/heddle-ui` are siblings); otherwise set `HEDDLE_UI_BIN`.

---

## What each action does

Every row is one ACP method the `heddle acp-agent` subcommand already serves. There is no fourth thing the window can do.

| UI action | Exact call it triggers |
|---|---|
| **App launch** | Spawns `heddle acp-agent --root … --silo … --model … [--base-url …] [--fs-root …]`, then sends ACP `initialize`, then `session/new`. Returns the session id the chain records runs under (`heddle-1`, `heddle-1#1` for its first run). |
| **Send** | Tauri command `send_prompt(text)` → ACP `session/prompt` — the request `crates/heddle-acp/src/lib.rs` answers by running the native loop. |
| **transcript update** (passive) | ACP `session/update` notifications, relayed **1:1 and untransformed** as the Tauri event `session-update`. The projection that produced them is `heddle-acp`'s `project_updates`, reading the run's chain. The window is a view of that view and adds nothing to it (Constitution V). |
| **Cancel** | Tauri command `cancel_run()` → ACP `session/cancel` notification. |
| **Closing the window** | Closes the client's end of the pipe. `heddle acp-agent` exits zero when its client disconnects, so nothing is killed and nothing leaks. |

The frontend is granted nothing else: `ui/src-tauri/capabilities/default.json` gives it `core:default` and `core:event:default` and no shell, filesystem, HTTP or process permission at all. It could not spawn an agent or reach a provider even if its code tried to.

---

## Manual smoke test

1. Export `HEDDLE_ROOT` and `HEDDLE_UI_MODEL` (e.g. `llama3.1`), with a local provider running.
2. `cd ui && npm run tauri dev`. The window opens and the status line goes from **Starting the agent…** to **Ready.**
3. Type `what is 2 + 2?` and press Enter. The status line shows **Working…**, then the answer appears and the status returns to **Ready.**
4. Send a second message. It runs on the same session — the chain records `heddle-1#1` and `heddle-1#2` under the same silo. Verify with:
   ```bash
   heddle ledger log --root "$HEDDLE_ROOT" --silo ui --run heddle-1#2
   ```
5. Send a message that will take several turns, then press **Cancel**. The status line reads **Cancelled. The step that was already running finished.** — and the next turn did not run.
6. Close the window, then confirm no `heddle` process is left behind.

---

## Known limitations

These are facts about the backend this slice sits on, not omissions in the window. They are stated here rather than left for a user to discover.

### There is no token-level streaming

`crates/heddle-acp/src/lib.rs` runs the whole turn, computes every `SessionUpdate` from the run's chain in one pass, and sends them all **before** answering `session/prompt`. So the screen shows a pending state for the run's duration and then the full transcript — assistant text and every tool call — appears at once. Nothing is being buffered by the UI; there is simply no incremental stream to subscribe to yet.

Adding one would mean changing `heddle-acp`'s `PromptRequest` handler to emit during the loop instead of after it. That is a backend change and belongs in its own spec. When it lands, `ui/src/chatState.ts`'s reducer is the boundary that gains test cases — it already folds one update at a time rather than one batch at a time.

### Cancel is a next-turn-boundary stop, not a mid-turn one

`session/cancel` sets a flag that `CancellableModel::turn` checks **before it starts the next model call** (`crates/heddle-acp/src/cancel.rs`). A model call already in flight always completes. Pressing Cancel therefore stops the run before its next step; it does not stop generation mid-token. The button's tooltip says so: *"Stops before the next step; a step already running finishes."*

### Tool permission requests are declined

`heddle acp-agent` asks its client before every tool call (`crates/heddle-acp/src/permission.rs`). This slice has no permission dialog, so the shell answers every `session/request_permission` by selecting the offered **reject** option. With no `HEDDLE_UI_FS_ROOT` set the question never arises — the policy allows nothing, and a tool the policy refuses never becomes a permission request. With one set, tool calls will be refused and shown as failed cards.

Declining is the only one of the three possible behaviours a client may choose unilaterally: allowing would grant what the operator never approved, and not answering would hang the child's loop thread. A client may narrow what runs, never widen it (Constitution VI). The permission dialog ships with the settings work below.

### One window, one session

`HeddleAgent` supports many sessions; this window drives exactly one. No tabs, no session switcher.

---

## Deliberately not in this slice

Named here so they read as scope, not as gaps:

- **The Code view.** A follow-up slice.
- **Settings and connector-management screens** — including the permission dialog above, and any UI for `--model` / `--fs-root` / `--base-url`. Same follow-up.
- **Packaging, signing and installers.** `bundle.active` is `false` in `tauri.conf.json`. Per-OS code signing is a release concern (Constitution's stack constraints), not part of "build and launch locally".

---

## Where the code is

| Path | What it owns |
|---|---|
| `ui/src-tauri/src/session.rs` | The ACP client: one child process, one session, three messages. Names no Tauri type, which is why `tests/chat_session.rs` can drive it with no window. |
| `ui/src-tauri/src/config.rs` | The environment → argv mapping in the table above, and its refusals. |
| `ui/src-tauri/src/main.rs` | Tauri wiring only: three commands, two events, one shutdown boundary. |
| `ui/src/chatState.ts` | The whole of the frontend's logic — a pure `SessionUpdate` → view-state reducer, unit-tested without a browser. |
| `ui/src/main.ts` | DOM glue. Holds no branching logic beyond painting what the reducer produced. |

Spec: [`specs/036-tauri-chat-ui/`](../specs/036-tauri-chat-ui/).
