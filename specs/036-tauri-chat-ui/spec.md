# Feature Specification: a Tauri Chat window over `skein acp-agent` (first UI slice)

**Slice**: 021
**Status**: implemented
**Depends on**: [008-acp-facade](../008-acp-facade), [013-acp-agent](../013-acp-agent)

Skein has had no graphical entry point at all. `crates/skein-cli` and `crates/skein-acp` are the only access surfaces, so the design's Phase-1 exit criterion "UI: Tauri (Chat + Code)" and the MVP exit test "from UI **and** CLI **and** API, a real scenario" cannot be met.

This slice adds `ui/` at the repository root: a Tauri desktop app with one screen. It adds **no capability**. It spawns `skein acp-agent` as a child process and drives it as an ACP client over stdio — the same role, the same protocol and the same binary that `crates/skein-cli/tests/cli_acp_agent.rs` already exercises.

---

## What this slice changes for a user

Before: to talk to Skein you write a full CLI invocation, wait, and read one printed line. No tool-call visibility, no cancel, one prompt per process.

After: you open a window, type, and watch the assistant's answer and any tool calls it made appear. You can send a second message on the same session, and you can cancel a run. The chain records everything exactly as it would have from the CLI, in the silo you named.

---

## Six things a reader must know up front

1. **Every UI action is an ACP call the CLI already serves.** App launch is `initialize` + `session/new`, Send is `session/prompt`, Cancel is `session/cancel`. There is no fourth thing the window can do, and the traceability table in FR-002 is the whole of its API surface. That is Constitution I made structural rather than promised.

2. **The frontend cannot reach anything.** `ui/src-tauri/capabilities/default.json` grants it `core:default` and `core:event:default` — no shell, no filesystem, no HTTP, no process permission. The child process is spawned by the Rust shell from configuration the *operator* set, never by anything the webview asked for.

3. **There is no token-level streaming, because there is none to relay.** `crates/skein-acp/src/lib.rs` runs the turn to completion, projects every `SessionUpdate` from the run's chain in one pass, and sends them all *before* answering `session/prompt`. The screen is designed around "pending, then the whole transcript at once". This is stated, not silently worked around.

4. **Cancel is a next-turn-boundary stop.** `crates/skein-acp/src/cancel.rs`: a model call already in flight always completes. The button stops the run before its *next* step. The tooltip and `docs/UI.md` say so in the user's words.

5. **A tool-permission dialog is not in this slice, so permission requests are declined.** `skein acp-agent` asks its client before every tool call. Declining is the only answer a client may choose unilaterally — allowing would grant what the operator never approved, and not answering would hang the child's loop thread. A client may narrow what runs, never widen it (Constitution VI).

6. **The Code view and the settings/connector screens are deliberately absent.** They are named in "Out of scope" so they read as scope rather than as gaps.

---

## Functional requirements

### FR-001 — The window drives the real binary as a real ACP client

The Rust shell spawns `skein acp-agent` with argv built from operator configuration, connects an `agent-client-protocol` `Client` to the child's stdio, and performs `initialize` then `session/new` before the window accepts input. It does not reimplement JSON-RPC framing, does not embed `skein-core`, and does not depend on any crate under `crates/`.

### FR-002 — Every UI action maps 1:1 to an ACP method

| UI action | Exact call |
|---|---|
| App launch | spawn `skein acp-agent --root … --silo … --model … [--base-url …] [--fs-root …]`; ACP `initialize`; ACP `session/new` |
| Send | ACP `session/prompt` |
| transcript update (passive) | ACP `session/update` notification, relayed 1:1 and untransformed |
| Cancel | ACP `session/cancel` notification |
| Window closed | the client's end of the pipe closes; the child exits zero on its own |

No transformation happens on the way out: the shell emits `notification.update` verbatim. The projection that built it is `skein-acp`'s `project_updates`, reading the chain. The window is a view of that view (Constitution V).

### FR-003 — Configuration is the CLI's own, and refusals name the variable

There is no settings screen and no config file in v0. Required values (`SKEIN_ROOT`, `SKEIN_UI_MODEL`) have no defaults and their absence is a refusal that names the variable, for the reason `SiloArgs::root` refuses to guess a silo root. Optional values (`SKEIN_UI_SILO`, `SKEIN_MODEL_BASE_URL`, `SKEIN_UI_FS_ROOT`, `SKEIN_UI_BIN`) become flags only when set; an unset optional value produces **no flag**, not an empty one, because the child inherits this process's environment and the two are not the same thing.

### FR-004 — Every value is a flag `skein acp-agent` already parses

The shell invents no configuration surface of its own. Each variable in FR-003 becomes a documented CLI flag. In particular, no `SKEIN_UI_FS_ROOT` means no `--fs-root`, which means the session has no tools at all — `crates/skein-cli/src/wiring.rs`'s "no root, no tools", unchanged and unsoftened.

### FR-005 — The frontend's logic is one pure, tested reducer

All event→view-state mapping lives in `ui/src/chatState.ts` as pure functions over immutable state, unit-tested in `node` with no DOM. `ui/src/main.ts` is DOM glue with no branching logic beyond painting. The reducer's input types are the **wire** shapes (`sessionUpdate` discriminator, `camelCase` fields), so a protocol change breaks a test rather than the running app.

### FR-006 — A turn's whole batch does not end the turn

Because a run's updates all arrive before its response, the reducer must not treat any update as evidence that the turn is over. `pending` is cleared only when `session/prompt` itself resolves.

### FR-007 — The window says so when the agent dies

If the child process ends, the window reports it and stops accepting input, rather than sitting in a pending state on a pipe with nobody at the other end.

### FR-008 — Nothing is left running

Closing the window closes the client's end of the pipe; `skein acp-agent` exits zero when its client disconnects. The session ends when its last handle drops, so there is no process to kill and none to leak.

### FR-009 — The slice is under the same gates as everything else

`ui/src-tauri` is an explicit entry in the root `Cargo.toml`'s `members`, so `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` cover it. `.github/workflows/core.yml` gains `ui/**` in its path triggers, plus the two steps that make those cargo steps possible: the Linux webview toolchain, and the frontend build that produces the `ui/dist` `tauri-build` reads.

### FR-010 — The permission answer is chosen by protocol kind, not by a known id

The shell picks the rejection out of the options the agent offered, matching on `PermissionOptionKind`, and falls back to cancelling the request when none is offered. It never names an option id that `skein-acp` happens to use today.

---

## Success criteria

| # | Criterion | Proven by |
|---|---|---|
| SC-001 | A started session names itself as the chain will | `starting_a_session_spawns_the_real_agent_and_names_the_session` |
| SC-002 | A prompt is answered and its transcript relayed before the answer | `a_prompt_is_answered_and_its_transcript_is_relayed_before_the_answer` |
| SC-003 | Two prompts run on one session, both transcripts arrive | `two_prompts_run_on_one_session_and_both_transcripts_arrive` |
| SC-004 | A cancel is reported as `Cancelled`, and the next turn did not run | `a_cancel_stops_the_run_at_the_next_turn_boundary_and_says_so` |
| SC-005 | A cancel with nothing in flight is a no-op, not an error | `cancelling_with_nothing_in_flight_is_not_an_error` |
| SC-006 | Ending the session shuts the child down and reports it once | `dropping_the_handle_shuts_the_agent_down_and_reports_it_once` |
| SC-007 | A prompt on a closed session fails instead of hanging | `a_prompt_after_the_agent_is_gone_fails_instead_of_hanging` |
| SC-008 | Missing required configuration refuses, naming the variable | `config.rs` unit tests |
| SC-009 | Unset optional configuration produces no flag | `optional_flags_are_absent_rather_than_empty_when_unset` |
| SC-010 | Each `SessionUpdate` variant maps to the right view state | `ui/src/chatState.test.ts` |
| SC-011 | A whole turn's batch does not clear `pending` | `keeps pending set until the prompt itself resolves` |
| SC-012 | `docs/UI.md` states both limitations in user-facing language | read-through |

The Rust acceptance tests run against the **real** `skein` binary with a `std::net::TcpListener` standing in for the model provider, so none of them needs a running Ollama and none opens a window.

---

## Assumptions and residuals

- **Tauri v2's MSRV is 1.77.2**, under the pinned 1.97. Checked before a version was locked; had it not been, bumping `rust-toolchain.toml` would have been a workspace-wide decision beyond this slice.
- **Adding `ui/src-tauri` to `members` taxes every workspace build.** `cargo test --workspace` now compiles Tauri, and on Linux that needs a system webview. This is the reason `.github/workflows/core.yml` grew an apt step — a consequence of FR-009 that is paid once, visibly, rather than by excluding the UI from the gates.
- **No view framework is chosen.** The Constitution's stack line names "Tauri/TS UI" and nothing more. One screen with one input, one transcript and one button does not need a framework decision forced by this slice; `chatState.ts` is framework-agnostic and the Code-view slice is free to introduce one if it earns its keep then.
- **`ui/package-lock.json` is committed** although the root `Cargo.lock` is not. An application pins its dependency tree; a library workspace does not.

---

## Out of scope

- **The Code view.** A follow-up slice.
- **Settings / connector-management screens**, including the tool-permission dialog and any UI for `--model` / `--fs-root` / `--base-url`. Same follow-up.
- **Token-level streaming.** Does not exist in `skein-acp`; adding it means changing that crate's `PromptRequest` handler, which is a backend change and belongs in its own spec.
- **Multi-session / multi-tab chat.** `SkeinAgent` supports many sessions; this window drives one.
- **Packaging, signing, installers.** `bundle.active` is `false`. Per-OS code signing is a release concern.
