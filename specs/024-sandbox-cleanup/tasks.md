# Tasks: `skein sandbox list` / `skein sandbox prune` (v0 slice)

**Spec:** `specs/024-sandbox-cleanup/spec.md` · **Plan:** `specs/024-sandbox-cleanup/plan.md` ·
TDD (red→green), branch `024-sandbox-cleanup` reset onto `dev` at `12c14f5`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the capability is two public free functions on `skein-sandbox`
  (`grants`, `prune`); `skein sandbox list|prune` is a call onto them plus a rendering, holding no
  logic of its own. `main.rs`'s own docstring authorises the direct edge · II Local-first ✅
  NON-NEGOTIABLE and untouched: no network, no new capability SID, no change to how a profile is
  created or a process launched. `prune` only ever removes
- III Test-First ✅ every behavioural step's outcome is recorded verbatim under `## Observed red`,
  and a step with no red says so and why rather than dressing one up
- IV Inverted coupling ✅ `skein-sandbox` remains a leaf depending on no Skein crate. `skein-cli`
  gains an edge to it directly rather than through `skein-connectors`, because `prune` is not an
  MCP tool and putting it behind the tool crate would hide a non-tool capability behind the tool
  boundary. Zero new third-party packages on every target
- V Traceability ✅ unchanged machinery: no new `StepKind`, no change to `ToolGateway`, `Approval`,
  `Redactor` or `AcpPermissionTransport`. Profile cleanup is machine-state maintenance, not a step
  in an agent's run, and putting it on a run's chain would attribute an operator's action to a model
- VI Security ✅ this is the slice that makes a destructive-by-nature capability safe by
  construction rather than by care. Ownership is proven in three structural layers — name gate,
  `REVOKE_ACCESS` on one `TRUSTEE_IS_SID`, live DACL read-back — and the record is deliberately
  **not** one of them, so even a tampered record could at worst revoke a `skein-<hash>` SID's own
  ACE. The destructive command requires an explicit selector, so a bare `prune` is a usage error
  rather than a machine-wide delete
- VII Neutrality ✅ one subcommand group, two functions, one plain-text record file. No new crate,
  no new `windows` feature, no serde. A silo-scoped manifest, a `%LOCALAPPDATA%\skein\` state
  directory, encoding the root in the profile's display name, `GetAppContainerFolderPath`,
  `CheckNetIsolation.exe`, `--json` and `--dry-run` were each considered and rejected with a reason
  in `plan.md`
- VIII Loop discipline ✅ NON-NEGOTIABLE and untouched. Nothing here runs inside a loop; `prune` is
  an operator command with a fixed, finite work list
- Cross-platform ⚠️ **Windows-only in substance, and it inherits rather than amends ADR-0006's
  scope.** The Constitution's "no OS-specific call without `#[cfg]` + an equivalent" is met on the
  `#[cfg]` and **not** on the equivalent — deferred to a Linux (Landlock) and a macOS (Seatbelt)
  slice each, exactly as slices 019 and 020 record. What is new here and worth stating: **there is
  nothing to clean up on those platforms**, because no backend there creates a profile or writes an
  ACE. The refusal is not a stub standing in for missing work; it is the honest answer. The CLI
  subcommand is nonetheless **present** on all three legs, for spec point 6's reason.

## Tasks
- [x] **T0** `specs/024-sandbox-cleanup/{spec.md,plan.md,tasks.md}`; branch `024-sandbox-cleanup`
      reset onto `dev` at `12c14f5`
- [x] **T1** control baseline: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace`, plus the `%LOCALAPPDATA%\Packages\skein-*` count
      before and after a suite run, each re-measured rather than quoted
- [x] **T2** types and signatures, no behaviour: `GrantKind`, `GrantState`, `GrantedDir`, `Grant`,
      `Pruned`, `grants()`, `prune()`, the new `record` module, and `skein-sandbox` on
      `skein-cli`'s dependency list
- [x] **T3** RED→GREEN — the record is written, and a second create unions rather than replaces
      (`tests/record.rs`)
- [x] **T4** RED→GREEN — `grants()` reports the live DACL (`tests/record.rs`)
- [x] **T5** RED→GREEN — `prune()` removes its own ACE and only its own (`tests/prune.rs`), after
      extracting the DACL read-back helpers into `tests/dacl/mod.rs`
- [x] **T6** RED→GREEN — the non-Windows refusal (`tests/absent.rs`)
- [x] **T7** RED→GREEN — the CLI (`main.rs`, `src/sandbox.rs`, `tests/cli_sandbox.rs`)
- [x] **T8** the test suite stops leaking profiles (severable)
- [x] **T9** gates, dependency drift, control diff, close-out
- [ ] **T10** hand-verification against a real session — **not part of this run.** See
      `## Live verification (T10)` below for the recorded commands and pass condition.

## Control baseline (T1)

On `024-sandbox-cleanup` @ `12c14f5` (reset onto `dev`), working tree clean, Windows 11 Pro
10.0.26200, toolchain 1.97, 2026-09-04, before any edit:

- `cargo fmt --all --check` — clean, no output, exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, `Finished dev profile`, exit 0.
- `cargo test --workspace` — **250 passed, 0 failed, 6 ignored**: `acp_session` 16,
  `cli_acp_agent` 16, `cli_chat` 12, `cli_ledger` 8, `cli_secret` 2, `connector` 9, `fs_root` 15,
  `fs_server` 8, `git_root` 5, `git_server` 13, `governed_fs_run` 6 (+2 ignored), `governed_git_run`
  4 (+1 ignored), `governed_proc_run` 0 (+2 ignored), `run_server` 10, `core` 19, `native_loop` 28,
  `tool_gateway` 14, `governed_run` 2, `openai_compat` 17 (+1 ignored), `rmcp_gateway` 9,
  `skein-sandbox` `src/lib.rs` unit target 4 (`argv`), `escape` 4, `launch` 4, `profile` 3,
  `silo_ledger` 7, `silo_secret` 5. Every other `src/lib.rs` and `src/main.rs` unit target reports 0.

Slice 020's close records 217 at `09d61f8`; the delta of +33 is slices 021 and 022, which is why the
baseline is re-measured rather than quoted.

### The leak, measured (T1)

```
$ ls -d "$LOCALAPPDATA/Packages"/skein-* | wc -l
1009                       # at this run's start
$ cargo test --workspace   # one full run
$ ls -d "$LOCALAPPDATA/Packages"/skein-* | wc -l
1036                       # +27
```

Measured twice with the same result. **27 AppContainer profiles per `cargo test --workspace`**, one
per distinct `TempDir` root reaching `Sandbox::create` across
`skein-sandbox/tests/{profile,launch,escape}.rs` and
`skein-connectors/tests/{connector,run_server}.rs`. This is T8's control: it must become 0.

The plan's 955 was the same quantity read in an earlier session. Nothing contradicts it; the number
grows by 27 per suite run and this run measured 1009.

### The record's location, re-measured (T1)

`icacls` on a real profile folder and its `AC` subfolder, this machine, output in French:

| Object | Trustees |
|---|---|
| `…\Packages\skein-000986f7bfe73a3d` | `AUTORITE NT\Système (I)(OI)(CI)(F)`, `BUILTIN\Administrateurs (I)(OI)(CI)(F)`, `WINE\cthedrez (I)(OI)(CI)(F)` — **no AppContainer SID** |
| `…\Packages\skein-000986f7bfe73a3d\AC` | the unresolvable AppContainer SID at `(OI)(CI)(CR)(F)`, plus those three, plus `Niveau obligatoire faible:(OI)(CI)(NW)` |

D1.3 and D3 rest on that difference, and it reproduces exactly as the plan recorded it.

## Observed red

## Deviations from the plan

## Live verification (T10)
