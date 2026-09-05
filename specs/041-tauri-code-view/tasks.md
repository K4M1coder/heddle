# Tasks: a Code view and a Settings/connectors screen (slice 041)

## Constitution Check

| Principle | How this slice satisfies it |
|---|---|
| **I — Headless core, CLI as reference, UI as a thin layer** | Every new action is a call the product already serves: `list_directory` is `FsRoot::read_dir` (what `fs_list` calls), `read_file` is `FsRoot::open_file` with `fs_read`'s own cap and UTF-8 rule, `session_settings` is a projection of the `ResolvedLaunch` the child was spawned from. The table is in `spec.md` FR-002 and in `docs/UI.md`. Where the CLI has no surface — Atlassian and M365 enablement, a provider picker — the window builds none and says so. |
| **II — Local-first, silo isolation** | Nothing new reaches the network. The two new reads touch one operator-named directory and refuse every path outside it. The webview gains no Tauri permission: `capabilities/default.json` is still `core:default` + `core:event:default`. |
| **III — Test-First** | Every one of the four new modules was preceded by its tests, each observed red: `code_commands.rs` before `code.rs`, `settings_command.rs` before `settings.rs`, `codeState.test.ts` before `codeState.ts`, `settingsState.test.ts` before `settingsState.ts`. The fixtures are real: real directories, real files, a real `git init`. |
| **IV — Inverted coupling & explicit boundaries** | `code.rs` and `settings.rs` name no Tauri type and take their inputs as arguments, so the tests drive them with no `AppHandle` and no window. `codeState.ts`/`settingsState.ts` import neither DOM nor Tauri. |
| **V — Traceability & reversibility** | The window records nothing and changes nothing. Both screens are read-only projections: the Code view of the disk, the Settings screen of one already-made launch decision. Rollback is deleting two tabs and four modules; Chat is untouched. |
| **VI — Security & secrets by reference** | Containment is `FsRoot`'s and is not re-derived. No file is written — the read-only decision is recorded with its reason rather than left as an unfinished edge. `--allow-run` stays a second opt-in on top of a root and is off by default. Every file name and file body is painted with `textContent`, never `innerHTML`: content off the operator's disk is data, never markup. |
| **VII — Neutrality & reuse (YAGNI)** | `FsRoot` and `is_git_repository` are reused rather than reimplemented — that reuse is the whole reason for the one new dependency. Still no view framework: three screens, `hidden`, and `replaceChildren`. |
| **VIII — Loop discipline** | Neither new screen touches the loop. Both are reads outside any run. |
| **Cross-platform** | No OS-specific code. Paths are `/`-separated relative paths, which `FsRoot` resolves on every platform. `HEDDLE_UI_ALLOW_RUN` produces a flag the child refuses off Windows, loudly, exactly as it does for the CLI. |
| **English-only content** | All UI copy, comments, docs, specs and tests are English. |

**Complexity Tracking — one departure, named rather than absorbed.** Slice 036 recorded that `ui/src-tauri` depends on no crate under `crates/`, and this slice adds one: `heddle-connectors`. The alternative was a second implementation of path containment inside the UI, which is the one thing Constitution VII most clearly forbids and the one place a bug would be a security bug. `heddle-connectors` carries no loop, no chain, no silo and no model, so the property Constitution I and IV were protecting — the UI being structurally incapable of holding agent logic — survives. The cost is real and is stated: the UI build now compiles `cap-std`, `git2`, `rmcp` and `tokio`.

---

## Tasks

| # | Task | Status |
|---|---|---|
| T1 | `ui/src-tauri/Cargo.toml` — `heddle-connectors`, `serde`, `git2` (dev) | done |
| T2 | `ui/src-tauri/src/config.rs` — `ResolvedLaunch`, `HEDDLE_UI_ALLOW_RUN`, `flag()`, 4 new tests | done |
| T3 | `ui/src-tauri/tests/code_commands.rs` observed red, then `src/code.rs` green | done |
| T4 | `ui/src-tauri/tests/settings_command.rs` observed red, then `src/settings.rs` green | done |
| T5 | `ui/src-tauri/src/main.rs` — the root handle and settings snapshot in `AppState`, three thin commands, the handle released on window close | done |
| T6 | `ui/src-tauri/src/lib.rs` exports; `capabilities/default.json` description kept true | done |
| T7 | `ui/src/codeState.test.ts` observed red, then `codeState.ts` green | done |
| T8 | `ui/src/settingsState.test.ts` observed red, then `settingsState.ts` green | done |
| T9 | `ui/src/index.html`, `style.css`, `main.ts` — tab strip and two panels | done |
| T10 | `docs/UI.md` — new action rows, new variable, new smoke-test steps, scope limits, "Where the code is" | done |
| T11 | `README.md` "Current status" — both screens and the call each triggers | done |
| T12 | Full local validation (fmt, clippy, `cargo test --workspace`, `npm test`, `npm run build`) | done |

---

## Observed red

- `tests/code_commands.rs` before `src/code.rs`: `unresolved import heddle_ui::code`.
- `tests/settings_command.rs` before `src/settings.rs`: `no SessionSettings in settings`.
- `codeState.test.ts` before `codeState.ts`: `Failed to load url ./codeState`.
- `settingsState.test.ts` before `settingsState.ts`: `Failed to load url ./settingsState`.

---

## Deviations from the plan, and why

1. **`capabilities/default.json` gained no command entries.** The plan expected to add the three new commands to an allowed set. Tauri v2 has no such set for app-registered commands — capabilities gate plugin permissions, and slice 036's three commands were already invokable with `core:default` alone. Only the file's description changed, so that it stays true.
2. **The Code view navigates one directory at a time with a breadcrumb path, rather than expanding an in-place tree.** Same information, one `read_dir` per click, and no expansion state that can disagree with the disk.
3. **A failed file read clears the content pane instead of leaving the previous file's text up.** The plan's edge-case list said "prior content untouched"; that would show one file's bytes under another file's name, which is the window lying about the disk. The pane shows the refusal instead.
