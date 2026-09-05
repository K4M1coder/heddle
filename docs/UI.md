# The Heddle desktop UI

One window, three screens: **Chat**, **Code** and **Settings**. It is a Tauri app — a Rust shell (`ui/src-tauri`) around a TypeScript frontend (`ui/src`) — and it is deliberately the thinnest thing that can be called a UI.

**The window has no capability of its own.** It does not talk to a model, does not run the loop, does not own a tool gateway and does not touch a silo. It spawns `heddle acp-agent` as a child process and speaks the Agent Client Protocol to it — the same protocol, over the same stdio transport, that an ACP-speaking editor uses, and that `crates/heddle-cli/tests/cli_acp_agent.rs` already drives against the same binary. Every button below is one call the CLI already serves (Constitution I).

The Code and Settings screens do not widen that. The Code view reads through `heddle_connectors::FsRoot` — the *same* containment primitive the agent's own `fs_read` and `fs_list` tools resolve every path through — so it can reach exactly what the running session can reach and refuses everything else in the same words. The Settings screen reports the flags that session was launched with and changes none of them: there is nothing on it to click.

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

There is no config file, and the Settings screen reports this configuration rather than editing it — changing a value means changing the environment and restarting, exactly as it does for the CLI. The window reads the environment, and it **refuses to start naming the variable** when a required one is missing — for the same reason `heddle --root` refuses to guess a silo root: guessing would put an agent's journal somewhere the operator did not name.

| Variable | Required | Becomes | Notes |
|---|---|---|---|
| `HEDDLE_ROOT` | **yes** | `--root` | The directory holding the silos. |
| `HEDDLE_UI_MODEL` | **yes** | `--model` | No default, exactly as the CLI has none. |
| `HEDDLE_UI_SILO` | no | `--silo` | Defaults to `ui`. |
| `HEDDLE_MODEL_BASE_URL` | no | `--base-url` | Omitted entirely when unset, so the CLI's own default applies. |
| `HEDDLE_UI_FS_ROOT` | no | `--fs-root` | **Absent means the session has no tools at all** — `crates/heddle-cli/src/wiring.rs`'s "no root, no tools". |
| `HEDDLE_UI_ALLOW_RUN` | no | `--allow-run` | `1`/`true`/`yes` turns on the sandboxed `proc_run` tool over the root; anything unrecognised is refused by name rather than read as "no". Ignored without `HEDDLE_UI_FS_ROOT`, because the tool is offered over the root or not at all. **Windows-only in v0** — elsewhere `heddle acp-agent` refuses the flag rather than ignoring it. |
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

Every row is one call the CLI already serves. There is no seventh thing the window can do.

| UI action | Exact call it triggers |
|---|---|
| **App launch** | Spawns `heddle acp-agent --root … --silo … --model … [--base-url …] [--fs-root …]`, then sends ACP `initialize`, then `session/new`. Returns the session id the chain records runs under (`heddle-1`, `heddle-1#1` for its first run). |
| **Send** | Tauri command `send_prompt(text)` → ACP `session/prompt` — the request `crates/heddle-acp/src/lib.rs` answers by running the native loop. |
| **transcript update** (passive) | ACP `session/update` notifications, relayed **1:1 and untransformed** as the Tauri event `session-update`. The projection that produced them is `heddle-acp`'s `project_updates`, reading the run's chain. The window is a view of that view and adds nothing to it (Constitution V). |
| **Cancel** | Tauri command `cancel_run()` → ACP `session/cancel` notification. |
| **Closing the window** | Closes the client's end of the pipe. `heddle acp-agent` exits zero when its client disconnects, so nothing is killed and nothing leaks. The `FsRoot` handle the Code view held is dropped with it, so the window stops pinning the operator's directory. |
| **Code tab, or clicking a folder or the path** | Tauri command `list_directory(path)` → `heddle_connectors::FsRoot::read_dir` on the session's `--fs-root` — the same call the agent's `fs_list` tool makes, with the same containment and the same refusal for a path that leaves the root. Sorted directories-first then by name, so a redraw is not a shuffle. |
| **Clicking a file** | Tauri command `read_file(path)` → `FsRoot::open_file` — the same call the agent's `fs_read` tool makes, with the same 64 KiB cap and the same refusal for anything that is not UTF-8 text. Read-only: there is no save. |
| **Settings tab** | Tauri command `session_settings()` → a projection of the `ResolvedLaunch` this window spawned the child from. `fs` and `shell` follow `--fs-root` and `--allow-run`; `git` follows `heddle_connectors::is_git_repository` against that root, which is the same call `wiring::ToolArgs::git_tools` makes to decide whether the CLI offers the git tools. Reads no environment variable of its own, so it cannot report a capability the running child does not have. |

The frontend is granted nothing else: `ui/src-tauri/capabilities/default.json` gives it `core:default` and `core:event:default` and no shell, filesystem, HTTP or process permission at all. It could not spawn an agent or reach a provider even if its code tried to. **The Code view is not an exception**: the webview never touches a file, it asks the Rust shell, and the Rust shell answers only inside the one directory the operator named.

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

### The Code view

7. Restart with `HEDDLE_UI_FS_ROOT` set to a directory with a few files and a subdirectory in it. Click **Code**. The tree lists that directory's real contents, directories first.
8. Click the subdirectory. It lists its real children, and the path above the tree gains a step. Click **root** in that path to come back; `../` does the same thing.
9. Click a text file. Its real content appears in the right-hand pane and the status line names the file. There is no save button, and its absence is deliberate — see "Deliberately not in this slice".
10. Click a file that is not text (any `.png` will do). The pane states that it is not UTF-8 text instead of showing decoded garbage.
11. Restart with `HEDDLE_UI_FS_ROOT` **unset** and open **Code** again. It states that this session has no fs-root and that the agent has no tools either — not an empty tree, which would be the window reporting on a directory it never looked at.

### The Settings screen

12. Click **Settings**. Five rows: Filesystem, Git, Shell, Atlassian, Microsoft 365.
13. With no `HEDDLE_UI_FS_ROOT`, Filesystem/Git/Shell all read **Disabled**, each saying why.
14. With `HEDDLE_UI_FS_ROOT` set to a git repository, Filesystem reads **Enabled** and names `fs_read, fs_list, fs_write`, and Git reads **Enabled** and names `git_status, git_log`. Point it at a directory that is not a repository and Git goes back to **Disabled** with the reason — there is no flag for this; the CLI offers the git tools exactly when the root it was given is one.
15. Atlassian and Microsoft 365 always read **Not wired to a session**, in every configuration. That is the true status and not a bug: both connectors are compiled in (specs 039 and 040) and no flag on any `heddle` subcommand turns either on for a session yet. A toggle here would be the window offering something the CLI cannot do.

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

Declining is the only one of the three possible behaviours a client may choose unilaterally: allowing would grant what the operator never approved, and not answering would hang the child's loop thread. A client may narrow what runs, never widen it (Constitution VI).

The permission dialog was previously described here as shipping "with the settings work". It did not: the Settings screen that shipped is a read-only report of connector status, and an interactive approval flow is a different thing with a different risk. It remains a follow-up.

### One window, one session

`HeddleAgent` supports many sessions; this window drives exactly one. The three tabs are three views of it, not three sessions: switching a tab repaints, and never starts, stops or re-launches an agent. No session switcher.

### The Code view is read-only, and pins its directory while the window is open

There is no save. `FsRoot` also holds a handle to the root for as long as the session lives — the same pinning the CLI's own sessions do, and the reason the operator's directory cannot be renamed or deleted out from under a running window. Closing the window releases it.

---

## Deliberately not in this slice

Named here so they read as scope, not as gaps:

- **Editing a file from the Code view.** Read-only. `fs_write` exists and the agent is approved for it, but a *second* writer racing the model's own writes on the same root raises questions this slice does not answer: what a conflicting concurrent write means, and what confirming or undoing one looks like (Constitution VI). The next slice.
- **Enabling or configuring a connector from the Settings screen.** It reports; it does not change anything. For `fs`/`git`/`shell` that means editing the environment and restarting, as it does for the CLI. For Atlassian and Microsoft 365 there is nothing to offer at all yet: no `heddle` subcommand accepts a flag that wires either to a session, so a switch here would be the window inventing a capability the CLI does not serve. Wiring them up is CLI work and its own spec.
- **A tool-permission dialog.** Above.
- **A provider or model picker.** `--provider` is flattened into `heddle chat` and not into `heddle acp-agent`, so there is no CLI surface for the window to expose yet.
- **Packaging, signing and installers.** `bundle.active` is `false` in `tauri.conf.json`. Per-OS code signing is a release concern (Constitution's stack constraints), not part of "build and launch locally".

---

## Where the code is

| Path | What it owns |
|---|---|
| `ui/src-tauri/src/session.rs` | The ACP client: one child process, one session, three messages. Names no Tauri type, which is why `tests/chat_session.rs` can drive it with no window. |
| `ui/src-tauri/src/config.rs` | The environment → argv mapping in the table above, and its refusals. |
| `ui/src-tauri/src/main.rs` | Tauri wiring only: six commands, two events, one shutdown boundary. |
| `ui/src-tauri/src/code.rs` | The Code view's two reads, over `heddle_connectors::FsRoot`. Names no Tauri type, so `tests/code_commands.rs` drives it against a real `TempDir` with no window. |
| `ui/src-tauri/src/settings.rs` | The connector status, derived from the one `ResolvedLaunch` the child was spawned with. Reads no environment of its own, so it cannot drift from what is running. |
| `ui/src/chatState.ts` | A pure `SessionUpdate` → view-state reducer, unit-tested without a browser. |
| `ui/src/codeState.ts` | The Code screen's tree/selection state, including dropping a slow answer for a click the operator has moved on from. Pure, no DOM. |
| `ui/src/settingsState.ts` | The connector rows' wording. Every fact stays the Rust side's; only the label and the status phrase are added here. |
| `ui/src/main.ts` | DOM glue for all three screens. Holds no branching logic beyond painting what the state modules produced. |

Specs: [`specs/036-tauri-chat-ui/`](../specs/036-tauri-chat-ui/) (Chat), [`specs/041-tauri-code-view/`](../specs/041-tauri-code-view/) (Code and Settings).
