# Implementation Plan: slice 041 — a Code view and a Settings/connectors screen

## Problem

The window has one screen. Design §8's Phase-1 UI axis is "Tauri (Chat + Code)", and an operator has no way to see the files their session can reach or which connectors it has without reading CLI flags back out of a terminal.

## What was verified before planning

- `heddle_connectors::FsRoot` (`crates/heddle-connectors/src/fs.rs:52-169`) exposes `read_dir`, `open_file` and `path`, resolves every argument handle-relative to a directory opened once, and refuses absolute paths and escapes with pinned messages. It is what `fs_read`/`fs_list` already call.
- `heddle_connectors::is_git_repository` is what `wiring::ToolArgs::git_tools` (`crates/heddle-cli/src/wiring.rs:363-377`) calls to decide whether the CLI offers `git_status`/`git_log`. There is no `--git` flag.
- `config::launch_from_env` already computes `fs_root` and discards it after building argv, and never passes `--allow-run` at all.
- **`grep` finds no `atlassian` or `m365` reference anywhere in `crates/heddle-cli`.** Neither connector has a session-enablement flag. specs/039's own "Out of scope" says so deliberately.
- `agent_client_protocol` has no client-initiated file read. There is no ACP route for a Code view that does not spend a model turn.
- Tauri v2 gates plugin permissions, not app-registered commands: slice 036's three commands are invokable with `core:default` alone.

## Approach

### D1 — The Code view reads `FsRoot` directly, in-process, and opens no second session

The task's constraint is "reuse the existing acp-agent session rather than opening a second one", and this honours it by spawning no second child. Reading over ACP is not an alternative that exists — there is no such method — and `session/prompt` would mean a non-deterministic, transcript-polluting model turn to list a directory. Calling `FsRoot` is the closest fit to Constitution I: it is the same connector code the agent's own tools call, reached without duplicating its sandboxing.

### D2 — `heddle-connectors` becomes `ui/src-tauri`'s one dependency under `crates/`

Slice 036 recorded that it had none. The alternative is a second, untested implementation of the one containment rule a string cannot express (Constitution VII forbids that trade). `heddle-connectors` carries no loop, chain, silo or model, so it cannot become a hiding place for agent logic — the property Constitution I and IV were protecting. Recorded as a departure in `tasks.md`, not glossed.

### D3 — One resolution, taken once, at spawn time

`launch_from_env` now returns `ResolvedLaunch { launch, fs_root, allow_run }`, and `start_session` opens the `FsRoot` and computes the settings snapshot from it before spawning the child. Neither `code.rs` nor `settings.rs` reads `std::env`. Re-reading later would let the Code view browse a directory the running agent was never given — the same drift `FsRoot` pins a handle to prevent.

### D4 — An unopenable `--fs-root` is a loud refusal at `start_session`

`ToolArgs::verify_root`'s own ordering: an operator who mistyped the variable hears about it before a model does, not on their first click.

### D5 — `HEDDLE_UI_ALLOW_RUN`, with a refusal for an unrecognised value

The settings screen cannot report `shell` truthfully without it, because `config.rs` never passed `--allow-run`. Unset/blank/`0`/`false`/`no` are off, `1`/`true`/`yes` are on, anything else is refused **by name** — a silent "no" would be the window guessing. Gated on `--fs-root`, because `proc_run` is offered over the root or not at all.

### D6 — A third connector state, not a disabled switch

`ConnectorState::{Enabled, Disabled, NotWiredToSession}`. Painting Atlassian/M365 as `Disabled` would tell an operator there is something to flip. There is not.

### D7 — Refusals are relayed verbatim, never restated

`code.rs` returns `FsRoot`'s messages and `settings.rs` owns the connector reasons; the TypeScript passes both through untouched. A second copy of "this path left the root" in the frontend could drift from the one the product enforces.

### D8 — A read is tagged with what it is for

`codeState.ts` records `dir:<path>` / `file:<path>` and drops any answer that no longer matches. Two quick clicks otherwise race, and the slower answer repaints a screen the operator has left.

### D9 — A failed read shows the reason and no content at all

Not the previous file's text under the new file's name. A pane that shows one file's bytes labelled as another is the window lying about the disk, which is the single thing this screen must not do.

### D10 — Read-only, and the absence of a save button is the record of that

Recorded in `docs/UI.md`'s "Deliberately not in this slice" with the reason (a second writer racing the model's `fs_write`, and Constitution VI's confirm/undo requirement), so it reads as scope.

## Steps

1. `ui/src-tauri/Cargo.toml` — `heddle-connectors`, `serde`, and `git2` as a dev-dependency for a real repository fixture.
2. `config.rs` — `ResolvedLaunch`, `HEDDLE_UI_ALLOW_RUN`, `flag()`; existing tests updated mechanically, four added.
3. `tests/code_commands.rs` red, then `src/code.rs` green.
4. `tests/settings_command.rs` red, then `src/settings.rs` green.
5. `main.rs` — `AppState` gains the root handle and the settings snapshot; three thin commands registered; the handle dropped on window close.
6. `lib.rs` exports; `capabilities/default.json` description kept true.
7. `codeState.test.ts` red, then `codeState.ts` green.
8. `settingsState.test.ts` red, then `settingsState.ts` green.
9. `index.html`, `style.css`, `main.ts` — a tab strip and two panels, vanilla DOM, `textContent` throughout.
10. `docs/UI.md`, `README.md`.

## Validation

### Project gates (unchanged; all must pass)

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --workspace`
- `cd ui && npm run build && npm test`

### New tests

- `ui/src-tauri/tests/code_commands.rs` — 11 tests over real `TempDir` trees.
- `ui/src-tauri/tests/settings_command.rs` — 7 tests over env fixtures and a real `git init`ed directory.
- `ui/src-tauri/src/config.rs` — 4 added to the existing 9.
- `ui/src/codeState.test.ts` — 15 tests.
- `ui/src/settingsState.test.ts` — 8 tests.

## Risks and rollback

| Risk | Mitigation |
|---|---|
| The Settings screen implies Atlassian/M365 are toggleable | A third state with its own wording and colour, pinned by a test asserting the reason string |
| A symlink or relative-path edge case reads outside the root | `FsRoot` is reused verbatim, with its own pinned tests in `heddle-connectors`; nothing is re-derived |
| The new dependency lets agent logic drift into the UI | `heddle-connectors` carries no loop, chain, silo or model; `code.rs`/`settings.rs` name no Tauri type and are driven by tests with no window |
| `FsRoot` pinning the root surprises an operator who tries to rename it | Stated in `docs/UI.md`'s "Known limitations" |
| Scope creep into editing or connector enablement | Both are explicit "Out of scope" items with a stated reason each, durably recorded in `docs/UI.md` |

Rollback is removing the two tabs and their four modules; the Chat screen's session, ACP client and three commands are untouched by this slice.

## Out of scope

File editing; connector enablement or credential entry; a provider/model picker; the tool-permission dialog; multi-session; packaging. Each with its reason in `spec.md`.
