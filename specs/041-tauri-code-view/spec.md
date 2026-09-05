# Feature Specification: a Code view and a Settings/connectors screen for the Tauri window

**Slice**: 041
**Status**: implemented
**Depends on**: [036-tauri-chat-ui](../036-tauri-chat-ui), [039-atlassian-connector](../039-atlassian-connector), [040-m365-connector](../040-m365-connector)

Slice 036 gave the product a window with one screen, Chat. Design §8's Phase-1 exit criterion for the UI axis is "Tauri (**Chat + Code**)", so Chat alone does not meet it, and an operator running the window has no way to see the files their session's agent can reach or which connectors that session actually has — both are `--fs-root`/`--allow-run` flags read off a terminal.

This slice adds two screens to the same window, the same session and the same child process. It adds **no capability**: the Code view reads through `heddle_connectors::FsRoot`, the containment primitive the agent's own `fs_read`/`fs_list` tools already resolve every path through, and the Settings screen reports the launch the child was really spawned from.

---

## What this slice changes for a user

Before: one screen. To see what the agent could reach you read your own `HEDDLE_UI_FS_ROOT` back out of your shell, and to know whether it had git tools you worked out whether that directory was a repository.

After: a **Code** tab lists that directory's real contents and shows a selected text file's real content, refusing anything outside the root in the same words the agent is refused. A **Settings** tab lists all five connectors the product ships with each one's real status for the running session — including the two that are neither on nor off.

---

## Five things a reader must know up front

1. **Every new UI action is a call the CLI already serves.** `list_directory` is `FsRoot::read_dir`, which is what `fs_list` calls; `read_file` is `FsRoot::open_file` with `fs_read`'s own 64 KiB cap and UTF-8 rule; `session_settings` is a projection of the `ResolvedLaunch` the child was spawned from. The traceability table in FR-002 is the whole of the new API surface (Constitution I).

2. **Atlassian and Microsoft 365 have no enablement flag at all, and the screen says so.** Both connectors are compiled in (specs 039 and 040), and no `heddle` subcommand accepts a flag that wires either to a session — specs/039's own "Out of scope" defers that enablement policy deliberately, and there is no `atlassian` or `m365` reference anywhere in `crates/heddle-cli`. So they are reported as **not wired to a session**, a third state, rather than as a disabled switch an operator could believe they might flip. Building that switch would be the window inventing a capability the CLI does not serve.

3. **The Code view is read-only, and that is a decision rather than a first pass.** `fs_write` exists and `heddle acp-agent` approves the agent for it. A *second* writer racing the model's own writes on the same root raises questions this slice does not answer — what a conflicting concurrent write means, and what confirming or undoing one looks like (Constitution VI). Named in "Out of scope".

4. **`ui/src-tauri` now depends on one crate under `crates/`.** Slice 036 recorded that it depended on none. The Code view has to decide whether a path is inside the session's root, and that decision already exists exactly once in the product; a second copy in the UI would be an untested reimplementation of the one containment rule a string cannot express. `heddle-connectors` carries no loop, no chain, no silo and no model, so it cannot become a place for agent logic to hide — which is the property Constitution I and IV were protecting. Recorded as a departure in `tasks.md`'s Constitution Check, not glossed.

5. **There is no second session and no second child process.** The Code view does not open one, and it could not read a file over ACP if it wanted to: `agent_client_protocol` has no client-initiated file read, and going through `session/prompt` would mean spending a non-deterministic model turn to list a directory.

---

## Functional requirements

### FR-001 — Three screens, one session

A tab strip switches between Chat, Code and Settings. Switching a tab repaints; it never starts, stops or re-launches an agent. There is one `AppState`, one `SessionHandle` and one child process, exactly as before.

### FR-002 — Every new action maps 1:1 to an existing capability

| UI action | Exact call |
|---|---|
| Code tab, clicking a folder, or clicking a step of the path | `list_directory(path)` → `FsRoot::read_dir` on the session's `--fs-root` — the call `fs_list` makes |
| Clicking a file | `read_file(path)` → `FsRoot::open_file`, with `fs_read`'s `READ_BYTE_CAP` and UTF-8 rule — the call `fs_read` makes |
| Settings tab | `session_settings()` → a projection of the `ResolvedLaunch` `start_session` spawned the child from; `git`'s row is `heddle_connectors::is_git_repository`, the call `wiring::ToolArgs::git_tools` makes |

No new Tauri permission is granted. `capabilities/default.json` remains `core:default` + `core:event:default`: the webview still has no filesystem, shell, HTTP or process permission of its own, and asks the Rust shell instead.

### FR-003 — The Code view reaches exactly what the session reaches, and refuses in the same words

Containment is `FsRoot`'s and is not re-derived. A path that escapes the root, an absolute path, and a path that does not exist are refused with `FsRoot`'s own messages. The root handle is opened **once**, in `start_session`, from the same resolution the child was launched with, so the view can never browse a directory the running agent was not given.

### FR-004 — No `--fs-root` is stated, not rendered as an empty tree

Absent a root the session has no tools at all (`wiring.rs`'s "no root, no tools"). The Code view says so. An empty listing would be the window reporting on a directory it never looked at.

### FR-005 — A file that cannot be shown says why

Over the read cap, or not valid UTF-8: a stated refusal, never a lossy decode, a silent truncation or a panic. The caps are `fs_read`'s, so a file the agent is refused is refused here for the same reason.

### FR-006 — The Settings screen reports real status, derived once

`fs` follows `--fs-root`; `shell` follows `--allow-run`; `git` follows `is_git_repository` against that root. Nothing reads `std::env` a second time: the snapshot is taken when the child is spawned, so what the screen reports cannot drift from what is running. Every row carries a reason, because a row that is merely off tells an operator nothing they can act on.

### FR-007 — Connectors with no CLI surface report a third state

`atlassian` and `m365` report `NotWiredToSession` in every configuration, with a one-line reason naming their specs. Not `Disabled`.

### FR-008 — A new configuration variable, on the same terms as every other

`HEDDLE_UI_ALLOW_RUN` becomes `--allow-run`, without which the Settings screen could not report `shell` truthfully — today's `config.rs` never passes the flag at all. Unset and blank are off; `1`/`true`/`yes` are on; anything unrecognised is **refused by name** rather than read as "no", because guessing is the one thing `config.rs` exists not to do. Ignored without a root, since `proc_run` is offered over the root or not at all.

### FR-009 — The frontend's new logic is two more pure, tested modules

`codeState.ts` and `settingsState.ts` are pure functions over the wire shapes, unit-tested in `node` with no DOM and no Tauri import, exactly as `chatState.ts` is. `main.ts` stays glue. The Rust structs are `#[serde(rename_all = "camelCase")]`, and a test pins the wire shape so a rename breaks a test rather than the running window.

### FR-010 — A slow answer for an abandoned click does not repaint the screen

`codeState.ts` records what is being fetched, so a listing or a file that arrives after the operator has clicked elsewhere is dropped instead of overwriting what they are looking at.

---

## Success criteria

| # | Criterion | Proven by |
|---|---|---|
| SC-001 | The Code view lists a real directory's real contents, ordered | `the_root_listing_is_the_real_directory_with_directories_first_and_then_names` |
| SC-002 | A subdirectory lists its own real children | `a_subdirectory_lists_its_own_real_children` |
| SC-003 | A selected file shows its real content | `a_selected_file_shows_its_real_content` |
| SC-004 | A path leaving the root is refused, and says so | `a_path_that_escapes_the_root_is_refused_and_says_so` |
| SC-005 | An absolute path is refused before it is joined onto the root | `an_absolute_path_is_refused_before_it_is_joined_onto_the_root` |
| SC-006 | No fs-root is reported as such, not as an empty directory | `no_fs_root_is_reported_as_such_and_not_as_an_empty_directory` |
| SC-007 | Non-UTF-8 content is refused with a reason | `a_file_that_is_not_utf_8_text_is_refused_with_a_reason_rather_than_decoded_as_garbage` |
| SC-008 | An oversize file is refused on `fs_read`'s own terms | `a_file_over_the_read_cap_is_refused_on_the_same_terms_fs_read_refuses_it` |
| SC-009 | All five connectors are named, so none reads as forgotten | `every_connector_the_product_ships_is_named_so_none_reads_as_forgotten` |
| SC-010 | With no root, fs/git/shell are off, each with a reason | `with_no_fs_root_the_session_has_no_tools_and_the_screen_says_so` |
| SC-011 | git is on only for a really `git init`ed root | `git_is_on_only_when_the_configured_root_is_really_a_repository` |
| SC-012 | shell follows `--allow-run` and is off by default | `shell_follows_allow_run_and_is_off_by_default` |
| SC-013 | Atlassian and M365 always report "not wired to a session" | `atlassian_and_m365_report_that_no_flag_wires_them_to_a_session_yet` |
| SC-014 | The wire shape the frontend types claim is the one Rust emits | `the_screen_serialises_to_the_camel_case_wire_shape_settings_state_ts_reads` |
| SC-015 | An unrecognised `HEDDLE_UI_ALLOW_RUN` is refused by name | `an_unreadable_allow_run_value_is_refused_by_name_rather_than_read_as_no` |
| SC-016 | The resolved launch reports the same root it put on the argv | `the_resolved_launch_reports_the_same_root_it_put_on_the_argv` |
| SC-017 | A stale answer for an abandoned click is dropped | `drops a listing that is not for the directory being opened`, `drops an answer for a file that is no longer selected` |
| SC-018 | A failed read shows the reason, never another file's text | `shows the refusal instead of any content at all` |
| SC-019 | `docs/UI.md` names both screens and the call each action triggers | read-through |

The Rust acceptance tests run against real directories in a `TempDir` — including a really `git init`ed one — with no window, no `AppHandle` and no running model.

---

## Assumptions and residuals

- **`FsRoot` pins its directory for the window's lifetime.** That is the type's documented second property, and the CLI's own sessions already pay it. It means the operator's `--fs-root` cannot be renamed or deleted while the window is open. Stated in `docs/UI.md` rather than discovered.
- **`HEDDLE_UI_ALLOW_RUN` is the one variable in `config.rs` with no prior sibling to copy exactly.** It follows `FS_ROOT`'s optional/blank-is-unset shape, and adds a refusal for an unrecognised value that the string-valued variables have no equivalent of.
- **Passing `--allow-run` off Windows makes the child refuse to start.** That is `RunArgs::resolve`'s existing, deliberate behaviour for the CLI, unchanged here: an unsupported flag is a loud refusal, not a silently missing tool.
- **The Code view navigates one directory at a time rather than expanding an in-place tree.** One click is one `read_dir`, with a breadcrumb path back up. Same information, no view state that can disagree with the disk.
- **Tauri v2 does not gate app-registered commands on a capability entry.** Capabilities gate plugin permissions (`core:*`); the three commands slice 036 shipped were already invokable with none. So `capabilities/default.json` gains no entry — only a description that stays true.

---

## Out of scope

- **Editing a file from the Code view.** Read-only. A second writer racing the model's own `fs_write` on the same root, and the confirm/undo story Constitution VI would require for it, are the next slice.
- **Enabling or configuring any connector from the Settings screen.** It reports; it changes nothing. For Atlassian and Microsoft 365 there is nothing to offer at all: no CLI flag wires either to a session, and building a toggle for a flag that does not exist would be new business logic in the UI.
- **A provider or model picker.** `--provider` is flattened into `heddle chat` and not into `heddle acp-agent`, so there is no CLI surface to expose. Design §8's Phase-1 exit criterion does not name one either.
- **A tool-permission dialog.** `docs/UI.md` previously said it would ship "with the settings work"; the settings work that shipped is a read-only report, and an interactive approval flow is a different thing with a different risk. Still a follow-up.
- **Multi-session / multi-tab chat, packaging, signing, installers.** Unchanged from slice 036.
