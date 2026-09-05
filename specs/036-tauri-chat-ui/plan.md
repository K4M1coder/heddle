# Implementation Plan: slice 021 — a Tauri Chat window over `heddle acp-agent`

## Problem

The design's Phase-1 exit criterion names a Tauri UI with a Chat and a Code screen, and the MVP exit test requires a real scenario driven "from UI and CLI and API". Neither is reachable: there is no UI at all.

The risk in building one is not that it is hard, it is that it quietly becomes a second implementation. A chat window that formats its own prompts, keeps its own transcript record, or decides its own tool policy would put agent logic outside the core — the exact failure Constitution I exists to prevent. So the design question for this slice is not "how do we render a chat" but "how little may the window know".

## What was verified before planning

Three premises in the original request did not match the repository, and are corrected here rather than followed.

| Premise | Reality |
|---|---|
| "the next spec number is after 034" | `specs/` tops out at `020-run-dir-allowlist`. **021** is next. |
| "specs 025–028 (streaming-sse, mid-stream-cancel, cancel-tool-call, cancel-permission-wait) already exist" | None of them exists. No SSE or HTTP streaming surface exists anywhere in the tree. The two access surfaces are `heddle chat` (one-shot) and `heddle acp-agent` (ACP over stdio). The real dependencies are **008-acp-facade** and **013-acp-agent**. |
| "the streaming/cancel machinery is already there" | `crates/heddle-acp/src/lib.rs`'s `PromptRequest` handler runs the loop to completion, then emits every update in one batch **before** responding. `crates/heddle-acp/src/cancel.rs`'s own doc comment states "cancellation is not mid-turn". |

Grepping for the cited spec numbers before writing a task line is what turned a false premise into three lines of correction instead of a plan that claimed a feature the repository does not have.

Also checked before anything was locked: **`tauri` 2.11.5 declares `rust-version: 1.77.2`**, under the `rust-toolchain.toml` pin of 1.97. Had it not been, this would have been escalated rather than resolved by bumping a workspace-wide pin from inside a UI slice.

## Approach

### D1 — The window is an ACP *client*, not an embedder

The alternative was linking `heddle-core` (or `heddle-acp`) into the Tauri binary and calling the loop in-process. Rejected: it would make the UI a second host for agent logic, and the only thing keeping it honest would be discipline. Spawning `heddle acp-agent` and speaking the protocol to it means the window is structurally incapable of doing anything the CLI cannot — the same subprocess, the same argv, the same wire an editor uses. `crates/heddle-cli/tests/cli_acp_agent.rs` is the reference for the client code; nothing about ACP framing is reimplemented.

`ui/src-tauri` depends on no crate under `crates/`. The dependency runs the other way at runtime, through argv.

### D2 — `session.rs` names no Tauri type

Updates leave through a caller-supplied closure (`impl Fn(SessionNotification) + Send + Sync`), and the connection's end through a second one. `main.rs` hands the first `AppHandle::emit`; `tests/chat_session.rs` hands it a `Vec`.

This is the reason the acceptance tests can drive the shipped client against the real binary with no window, no webview and no display — which in turn is why they can run on all three CI runners. It is Constitution IV's inverted coupling applied to the UI layer itself.

### D3 — One connection thread, and no request is ever awaited inside it

`Client::connect_with` scopes the connection to one async closure. A long-lived desktop app needs to issue requests from that closure at arbitrary later times, so the closure runs a loop over a `futures::channel::mpsc` of commands.

The loop must never `await` a request to completion. If it did, a `session/cancel` arriving while a prompt was in flight could not be delivered — the loop would be sitting on the prompt's response. So every request is issued with `on_receiving_result`, which registers a callback and returns immediately, and the answer travels back to the caller over a `oneshot`. This is the same reasoning `crates/heddle-acp/src/permission.rs` records for its own non-blocking `send_request`.

### D4 — Shutdown is closing the pipe, not killing a process

`SessionHandle` is `Arc`-backed and cloneable; the session ends when the last handle drops. Dropping ends the command loop, which ends the `connect_with` closure, which closes the connection, which closes the child's stdin — and `heddle acp-agent` exits zero when its client disconnects (a behaviour slice 013 already tests). Nothing is killed, so nothing can be orphaned by a failed kill.

### D5 — Permission requests are declined, by protocol kind

There are exactly three possible behaviours for a client with no permission dialog: allow, decline, or don't answer. Not answering hangs the child's loop thread forever. Allowing makes the UI grant what the operator never approved. Declining is the only one a client is *permitted* to choose unilaterally, because a client may narrow what runs and never widen it (Constitution VI).

The rejection is selected out of the options the agent offered, matching `PermissionOptionKind::RejectOnce | RejectAlways`, falling back to `RequestPermissionOutcome::Cancelled`. Hardcoding `heddle.reject-once` would couple the UI to an implementation detail of `heddle-acp` rather than to the protocol.

With no `--fs-root` the question never arises: the policy allows nothing, and a tool the policy refuses never becomes a permission request.

### D6 — All frontend logic is one pure reducer over the wire shapes

`ui/src/chatState.ts` holds every decision; `ui/src/main.ts` paints. The reducer's types are the JSON that actually crosses the IPC boundary — `SessionUpdate` is `#[serde(tag = "sessionUpdate", rename_all = "snake_case")]` and its payloads are `camelCase` — so a protocol change surfaces as a failing unit test, not as a blank transcript.

`ContentBlock` is modelled as one open shape (`{ type: string; text?: string }`) rather than a discriminated union, because the Rust enum is `#[non_exhaustive]`: a closed union would be a claim the protocol does not make, and an unknown discriminator must not throw in an app whose only recovery is a restart.

### D7 — `applyUpdate` never touches `pending`

Because a run's updates all arrive before its response (the corrected premise above), no update is evidence that the turn is over. Only the `session/prompt` promise resolving clears `pending`. A reducer that cleared it on the first update would re-enable Send mid-turn. There is a test for exactly that batch.

The reducer folds one update at a time rather than one batch at a time, so if real streaming ever lands it gains test cases rather than a rewrite.

### D8 — Configuration is environment→argv, and it refuses rather than guesses

No settings screen (out of scope) and no config file (v0 has none anywhere). `config.rs` maps the environment onto flags `heddle acp-agent` already parses, and refuses on a missing required value naming it — the reasoning `SiloArgs::root` already records. An unset *optional* value produces no flag at all, not an empty one: the child inherits this process's environment, and "absent here" and "absent there" are different facts.

The `heddle` binary is resolved beside the app's own executable, never from a hardcoded path.

### D9 — `ui/src-tauri` is an explicit workspace member, and CI pays for it visibly

`members` is `["crates/*"]`, a glob, so the UI needs naming. The alternative — leaving it out of the workspace — would exempt it from `fmt`, `clippy -D warnings` and `cargo test --workspace`, which is the wrong trade for a slice whose whole claim is that it adds no logic.

The consequence is real and is not hidden: every workspace build now compiles Tauri, and on Linux that needs a system webview. `.github/workflows/core.yml` therefore gains an apt step and a frontend-build step (`tauri-build` reads `ui/dist` at compile time), in that order, before the cargo steps. Without them the Linux leg of the matrix fails.

## Steps

| # | Step | Files |
|---|---|---|
| T1 | The reducer's tests, then the reducer (red → green) | `ui/src/chatState.test.ts`, `ui/src/chatState.ts` |
| T2 | Frontend toolchain and screen | `ui/package.json`, `ui/vite.config.ts`, `ui/tsconfig.json`, `ui/src/index.html`, `ui/src/main.ts`, `ui/src/style.css` |
| T3 | Workspace member + Tauri crate scaffold | `Cargo.toml`, `ui/src-tauri/{Cargo.toml,build.rs,tauri.conf.json,capabilities/default.json,icons/icon.ico}` |
| T4 | The ACP client's acceptance tests, then the client (red → green) | `ui/src-tauri/tests/chat_session.rs`, `ui/src-tauri/src/session.rs`, `ui/src-tauri/src/lib.rs` |
| T5 | Configuration, with its unit tests | `ui/src-tauri/src/config.rs` |
| T6 | Tauri wiring: three commands, two events, one shutdown boundary | `ui/src-tauri/src/main.rs` |
| T7 | Documentation | `docs/UI.md`, `README.md` |
| T8 | Gates | `.github/workflows/core.yml`, `.gitignore` |
| T9 | Full local validation | — |

T1 before T2 and T4 before T6 are the TDD ordering (Constitution III). T3 before T4 only because the test file needs a crate to live in.

## Validation

### Project gates (unchanged; all three must pass)

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`cargo test --workspace` is the canonical command for this slice rather than `cargo test -p heddle-ui`: `CARGO_BIN_EXE_*` only covers the current package's binaries, and `tests/chat_session.rs` drives `heddle`, which belongs to `heddle-cli`. The test derives the path from its own location and, if the binary is absent, fails with the exact remedy rather than with a confusing spawn error.

### New gates

```bash
cd ui && npm ci && npm test && npm run build
```

`npm run build` is `tsc --noEmit && vite build`, so it is the type-check gate as well as the bundler, and it must run **before** any cargo step: `tauri-build` reads `ui/dist`.

### New tests

| Test | File | What it proves |
|---|---|---|
| `starting_a_session_spawns_the_real_agent_and_names_the_session` | `chat_session.rs` | The shell spawns the real binary and completes `initialize` + `session/new`; the id is the one the chain will use. |
| `a_prompt_is_answered_and_its_transcript_is_relayed_before_the_answer` | `chat_session.rs` | `session/prompt` end to end, and the ordering guarantee the reducer depends on. |
| `two_prompts_run_on_one_session_and_both_transcripts_arrive` | `chat_session.rs` | One session, many runs — the shape `HeddleSession` records as `heddle-1#1`, `heddle-1#2`. |
| `a_cancel_stops_the_run_at_the_next_turn_boundary_and_says_so` | `chat_session.rs` | `StopReason::Cancelled`, and that the turn after the cancel did not run. |
| `cancelling_with_nothing_in_flight_is_not_an_error` | `chat_session.rs` | A no-op, not an error. |
| `dropping_the_handle_shuts_the_agent_down_and_reports_it_once` | `chat_session.rs` | D4: the pipe closes, the child ends, the window is told once. |
| `a_prompt_after_the_agent_is_gone_fails_instead_of_hanging` | `chat_session.rs` | A dead session fails fast rather than hanging a window. |
| 8 configuration tests | `config.rs` | D8: refusals name the variable, blanks count as unset, optional flags are absent rather than empty, the binary defaults beside the app. |
| 20 reducer tests | `chatState.test.ts` | D6/D7: each variant's mapping, the batch case, stop reasons, disconnection, the send/cancel guards, immutability. |

**The stub provider is gated.** It reads a turn's request, reports it, and then *waits* until the test lets it answer. That is what makes the cancellation test a statement about ordering rather than about how fast a runner is: the loop thread is provably parked mid-turn while the cancel is sent. The cancel is then followed by an `initialize` round trip — JSON-RPC dispatch is ordered, so an answer to it proves the child already processed the cancel. Only then is the turn released.

A `TcpListener` in the test process stands in for the model, so no test needs Ollama; no test opens a window, so none needs a display.

## Risks and rollback

| Risk | Mitigation |
|---|---|
| Tauri's MSRV exceeds the toolchain pin | Checked before locking a version: 1.77.2 vs. 1.97. Resolved, not deferred. |
| A reviewer expects token-level streaming | `spec.md`'s point 3, this plan's premise table, and `docs/UI.md`'s "Known limitations" all state up front that it does not exist. |
| An orphaned `heddle acp-agent` if the app crashes | D4: shutdown is the pipe closing, and the child exits zero on client disconnect on its own. A hard crash of the app closes its handles too. |
| The Linux CI leg fails on a missing webview | The apt step in `core.yml`, added in the same change as the `members` entry that causes the need. |
| Workspace build time grows for everyone | Accepted and recorded in `spec.md`'s residuals. The alternative — exempting the UI from the gates — is worse for a slice whose claim is that it adds no logic. |

Rollback is removing `ui/` and the `ui/src-tauri` entry from `members`; nothing under `crates/` changed.

## Out of scope

The Code view; settings and connector screens (including the permission dialog); token-level streaming; multi-session chat; packaging, signing and installers. Each is named in `spec.md` and in `docs/UI.md` so it reads as scope rather than as a gap.
