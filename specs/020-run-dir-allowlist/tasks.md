# Tasks: an operator-configured `--run-dir` allowlist for `proc_run` (v0 slice)

**Spec:** `specs/020-run-dir-allowlist/spec.md` · **Plan:** `specs/020-run-dir-allowlist/plan.md` ·
TDD (red→green), branch `020-run-dir-allowlist` cut from `dev` at `09d61f8`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ no CLI of its own; `skein acp-agent` gains one repeatable flag and stays the
  authoritative client. The rule that decides which executable a `command` may name stays in
  `skein-connectors`, and the ACL grant stays in `skein-sandbox` · II Local-first ✅ NON-NEGOTIABLE
  and untouched: the AppContainer profile still carries zero capability SIDs and the launch still
  passes `CapabilityCount: 0`, so WFP has no permit filter to match on. A run directory is granted
  read and execute; nothing about the network changes, and slice 019's loopback gate with its
  unsandboxed positive control still passes
- III Test-First ✅ every step's red was observed and recorded verbatim under `## Observed red`
  before its green. T2 is deliberately a compile-and-green refactor with a stop condition rather
  than a red, for the reason its entry states
- IV Inverted coupling ✅ `skein-core` gains nothing and depends on nothing new. `skein-sandbox`
  remains a leaf depending on no Skein crate; `skein-connectors` still reaches it through one
  `#[cfg]`-gated module with no type of the dependency in any public signature. `RunDirs` lives in
  `skein-connectors` because it must exist on all three platforms, and `Sandbox` takes it as
  `&[PathBuf]`
- V Traceability ✅ unchanged machinery, unchanged shape: a `proc_run` call still lands `ToolCall` →
  `Approval` → `ToolResult` on the chain. No new `StepKind`, no change to `ToolGateway`,
  `Approval`, `Redactor` or `AcpPermissionTransport`
- VI Security ✅ deny-by-default stays structural: `--run-dir` is a **third** opt-in that clap
  refuses without the second (`--allow-run`), which itself needs the first (`--fs-root`), and the
  allowlist is unrepresentable without run access because it rides inside `RunAccess::Allowed`. The
  grant is narrowed from the fs-root's `GENERIC_ALL` to `GENERIC_READ | GENERIC_EXECUTE`, and that
  narrowing is proven twice — as an ACL read-back and as an effect. Nothing is auto-discovered
- VII Neutrality ✅ one flag, one newtype, no new tool and no new crate. An environment fallback,
  `--allow-cargo` / `--allow-node` convenience flags, a config file, a third `with_run` parameter, a
  `%PATH%` search, `.cmd` shim support and a run-dir listing tool were each considered and rejected
  with a reason in `plan.md`
- VIII Loop discipline ✅ NON-NEGOTIABLE and untouched. A refusal is still an `Err(String)` the
  model is told about and the run survives; a nonzero exit is still an `Ok`
- Cross-platform ⚠️ **This slice is intentionally Windows-only, and it inherits rather than amends
  ADR-0006's scope.** ADR-0006 authorizes shipping `shell` on one OS first; the Constitution's "no
  OS-specific call without `#[cfg]` + an equivalent" is met on the `#[cfg]` and **not** on the
  equivalent, which is deferred to a Linux (Landlock) and a macOS (Seatbelt) slice each. On the
  macOS and Linux CI legs `skein-sandbox` still compiles to a crate whose only reachable behaviour
  is a loud refusal, `proc_run` is still absent from every catalogue, and `RunDirs` compiles there
  because it is plain path validation in `skein-connectors` rather than anything Win32.

## Tasks
- [x] **T0** `specs/020-run-dir-allowlist/{spec.md,plan.md,tasks.md}`; branch
      `020-run-dir-allowlist` cut from `dev` at `09d61f8`
- [x] **T1** control baseline: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace`, each re-measured rather than quoted
- [x] **T2** types and signatures, no new behaviour: `RunDirs`, `RunAccess::Allowed(RunDirs)`,
      `Sandbox::create(root, run_dirs)`, `Sandbox::run_dirs()`, `grant`'s `access` parameter,
      `resolve_exe`'s ignored new argument, and every call site
- [x] **T3** RED→GREEN — the narrower grant, read back off the directory (`tests/profile.rs`)
- [x] **T4** RED→GREEN — a binary in a named run directory executes (`tests/launch.rs`)
- [x] **T5** RED→GREEN — the run directory is not writable from inside (`tests/escape.rs`)
- [x] **T6** RED→GREEN — resolution, ordering and the refusal (`tests/run_server.rs`)
- [x] **T7** RED→GREEN — the advertisement (`tests/connector.rs`)
- [x] **T8** RED→GREEN — the CLI (`cli_acp_agent.rs`, `fs_root.rs`)
- [x] **T9** the live test, gates and close-out
- [ ] **T10** hand-verification against live Ollama — **not part of this run.** See
      `## Live verification (T10)` below for the recorded command and pass condition.

## Control baseline (T1)

On `020-run-dir-allowlist` @ `09d61f8`, working tree clean, Windows 11 Pro 10.0.26200, toolchain
1.97, 2026-09-04, before any edit:

- `cargo fmt --all --check` — clean, no output, exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, `Finished dev profile`, exit 0.
- `cargo test --workspace` — **217 passed, 0 failed, 4 ignored**: `acp_session` 16,
  `cli_acp_agent` 13, `cli_chat` 12, `cli_ledger` 8, `cli_secret` 2, `connector` 8, `fs_root` 10,
  `fs_server` 7, `git_root` 5, `git_server` 13, `governed_fs_run` 4 (+1 ignored), `governed_git_run`
  4 (+1 ignored), `governed_proc_run` 0 (+1 ignored), `run_server` 7, `core` 19, `native_loop` 25,
  `tool_gateway` 14, `governed_run` 2, `openai_compat` 15 (+1 ignored), `rmcp_gateway` 9,
  `skein-sandbox` `src/lib.rs` unit target 4 (`argv`), `escape` 3, `launch` 3, `profile` 2,
  `silo_ledger` 7, `silo_secret` 5. Every other `src/lib.rs` and `src/main.rs` unit target reports 0.

Slice 019's close records 193 at `b82f37a`, before its own tests landed; the delta of +24 is slice
019's own additions, which is why the baseline is re-measured rather than quoted.

## Observed red

**T3** — `cargo test -p skein-sandbox --test profile`, the one new test:

```
thread 'a_run_dir_is_granted_read_and_execute_and_the_root_is_not' panicked at
crates\skein-sandbox\tests\profile.rs:185:5:
the run directory's DACL must name the AppContainer SID at all, got
[("S-1-5-21-1203453866-3760803099-1050353712-1008", 1245631),
 ("S-1-5-21-1411561155-2461164688-2535433281-4238526846", 1245631),
 ("S-1-5-18", 2032127), ("S-1-5-32-544", 2032127),
 ("S-1-5-21-4080930094-269924791-1978800073-2222", 2032127)]
test result: FAILED. 2 passed; 1 failed
```

The expected red and not an ACL or a Win32 one: T2 threaded the list through `Sandbox::create` and
deliberately granted nothing with it, so the run directory carries its five ordinary ACEs and no
AppContainer one. The mask dump in the failure message is the helper doing its job — those five are
the two users, SYSTEM, Administrators and the owner, at `0x1301FF` and `0x1F01FF`, all normalised.

One **stated deviation** from the plan's control diff, made to get this red at all: the test needs
`FILE_GENERIC_READ`, `FILE_READ_DATA`, `WRITE_DAC` and their neighbours, which live behind the
`windows` crate's `Win32_Storage_FileSystem` feature. That feature was added to `skein-sandbox`'s
existing `[target.'cfg(windows)'.dev-dependencies]` `windows` entry, beside the `Win32_UI_Shell` the
argv oracle already needs. **No product dependency and no product feature changed** — the plan's
"`skein-sandbox`'s dependency set is untouched" holds for `[dependencies]`. The rejected alternative
was hand-copied hex constants in a security test, which is exactly the kind of second source of
truth this codebase refuses.

## Live verification (T10)

**Not performed in this run** — it was explicitly excluded from the implementation run's scope. The
command and its pass condition are recorded here so the hand-verification is repeatable rather than
re-derived.

Target: `D:\Users\cthedrez\.rustup\toolchains\1.97-x86_64-pc-windows-msvc\bin`, command
`cargo --version`. Chosen on measurement: that directory is user-owned with `FullControl` so the
grant succeeds **without elevation**; it holds the real `cargo.exe` and `rustc.exe` plus the
`rustc_driver-*.dll` and `std-*.dll` they load, all in the one directory one inheritable ACE covers;
and `cargo --version` was measured to exit 0 under the sandbox's exact five-variable environment
block. `node --version` is the documented fallback and needs an **elevated** skein, because
`C:\Program Files\nodejs` is owned by SYSTEM and carries no AppContainer ACE. `~\.cargo\bin` is
deliberately not the target: it holds the rustup **shim**, which re-executes the real cargo under a
toolchain `bin`.

```powershell
$env:SKEIN_LIVE_MODEL     = "gemma4:latest"   # or whatever `ollama list` actually offers
$env:SKEIN_LIVE_RUN_DIR   = "D:\Users\cthedrez\.rustup\toolchains\1.97-x86_64-pc-windows-msvc\bin"
$env:SKEIN_LIVE_SILO_ROOT = "$env:TEMP\skein-live-020"
cargo test -p skein-connectors --test governed_proc_run -- --ignored --nocapture
skein ledger log  --root $env:SKEIN_LIVE_SILO_ROOT --silo live020
skein ledger show --root $env:SKEIN_LIVE_SILO_ROOT --silo live020 <the ToolResult step id>
```

Pass condition: the model chose `proc_run` with `{"command":"cargo","args":["--version"]}` (or
`cargo.exe`), the chain carries `ToolCall` → `Approval {decision: allowed}` → `ToolResult`, and that
`ToolResult` payload contains a real `cargo 1.97.…` line produced by a real `CreateProcessW` inside
the AppContainer. The `skein ledger show` output for that step is the evidence to paste here.

If the model declines to call the tool, that is a model-selection finding and not a defect — slice
019's live section says so and this one inherits the wording.
