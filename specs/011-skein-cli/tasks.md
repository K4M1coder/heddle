# Tasks: `skein-cli` — the reference CLI client (v0 slice)

**Spec:** `specs/011-skein-cli/spec.md` · TDD (red→green), product code in `crates/skein-cli` plus
one additive method in `crates/skein-core`, branch `011-skein-cli` cut from `dev` after slice 010
merged.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ **this is the slice that makes Principle I true of the product** — the core
  was already headless, it had no client. Every command is a thin call onto an existing public
  API; the one place the CLI needed knowledge the API lacked (enumerating runs) is closed by
  *adding it to the API*, not by reaching around it · II Local-first ✅ (`Silo::open`'s own id
  validation is what the CLI calls, so containment stays a property of the id; no network)
- III Test-First ✅ (T1 pins the `clap` surface against the vendored source **and** a compiled
  probe before any product code; T3's red observed before T4, T5's before T6, T7's before T8) ·
  IV Inverted coupling ✅ (`skein-cli` has **no `lib` target**, so nothing can depend on the
  outermost layer; it depends on neither protocol crate)
- V Traceability ✅ (`ledger log`'s `{kind}` column is the step kind's **serde** name via
  `serde_json::to_value` — the same string the hash is fed, mirroring `ledger_store.rs`'s own
  stated rule, so there is no second name mapping that can drift from the hashed bytes)
- VI Security ✅ (no `--value` flag — machine-asserted, clap exits 2 on it; a terminal stdin is
  refused rather than prompted, so the value never reaches terminal scrollback; an empty secret is
  refused; no code path formats the value into any stream; an unknown silo is a loud error, never
  an empty answer)
- VII Neutrality ✅ (two command groups, five subcommands, one additive core method. **No chat, no
  `acp-agent`, no placeholder model**, no `--json`, no colour, no config file, no completions, no
  `--silo` on `secret`. The absent `ModelClient` is stated in the spec, not papered over)
- VIII Loop discipline ✅ (`LoopController`, `ProgressProbe` and `NativeLoop` untouched; v0 runs
  no loop)
- Cross-platform ✅ (no `#[cfg]` in the new crate; `#[command(bin_name = "skein")]` makes usage
  text identical on all three OSes. `core.yml`'s `paths:` already covers `crates/**` and
  `Cargo.toml`, and `members = ["crates/*"]` already covers a new crate — confirmed by reading,
  not edited).

## Tasks
- [x] **T0** `specs/011-skein-cli/{spec.md,plan.md,tasks.md}`; branch `011-skein-cli` cut from
      `dev` with slice 010 merged
- [x] **T1** pinned the `clap` surface against the vendored `clap 4.6.6` source and a compiled probe,
      *before* any product code; measured, not copied — see below
- [x] **T2** control baseline: `cargo test --workspace` before any edit — **71**
- [x] **T3** RED — `ledger_runs_lists_run_ids_in_first_append_order` in
      `crates/skein-core/tests/core.rs` against the not-yet-existing `Ledger::runs()`
- [x] **T4** GREEN — `Ledger::runs()` in `crates/skein-core/src/ledger.rs`
- [x] **T5** RED — `crates/skein-cli/tests/cli_ledger.rs` (e1..e7) against a `fn main() {}` stub
- [x] **T6** GREEN — `src/main.rs` + `src/ledger.rs`
- [x] **T7** RED — `crates/skein-cli/tests/cli_secret.rs` (e8..e9)
- [x] **T8** GREEN — `src/secret.rs`
- [x] **T9** gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace`, `cargo build --workspace` + run `skein --help`
- [ ] **T10** control diff: `git diff dev` empty on `crates/skein-mcp/`, `crates/skein-acp/`,
      `crates/skein-silo/`, `spikes/`, `.github/` and `rust-toolchain.toml`
- [ ] **T11** dependency drift recorded per target
- [ ] **T12** close out: correct `docs/DEVELOPMENT.md`'s two stale lines, tick the two stale
      bullets in `specs/003-skein-core-foundation/tasks.md` and the first of spec 010's "Next
      slice", set this spec's Status

## Pinned `clap` surface (T1)

Re-measured on 2026-09-03 on this Windows host at the pinned 1.97, from the **vendored** source in
the registry cache and from a throwaway probe crate outside this repository (built, run, deleted;
`git status` was clean before and after). The advisory plan's §1 numbers were **not** copied — they
were re-derived, and they hold, with one correction noted below.

| Item | Pinned spelling / measured value |
|---|---|
| Declared dependency | `clap = { version = "4.6", default-features = false, features = ["std", "derive", "help", "usage", "error-context"] }` |
| Resolved versions | `clap 4.6.6`, `clap_builder 4.6.6`, `clap_derive 4.6.4`, `clap_lex 1.1.0` |
| License | `MIT OR Apache-2.0` (`clap-4.6.6/Cargo.toml`) |
| MSRV | **1.85** for `clap`, `clap_builder`, `clap_derive` and `clap_lex` (`rust-version` in each vendored `Cargo.toml`), under the 1.97 pin — **no toolchain change** |
| `clap`'s own default feature set | `["std", "color", "help", "usage", "error-context", "suggestions"]` (`clap-4.6.6/Cargo.toml` `[features] default`) — ours is that set minus `color` and `suggestions` |
| `cargo tree -e normal` with our features | **10 packages**: `clap`, `clap_builder`, `clap_lex 1.1.0`, `anstyle 1.0.14`, `clap_derive`, `heck 0.5.0`, `proc-macro2 1.0.107`, `quote 1.0.47`, `unicode-ident 1.0.24`, `syn 3.0.4` |
| Same probe with default features + `derive` | **21 packages** — the extra 11 are `anstream 1.0.0`, `anstyle-parse`, `anstyle-query`, `anstyle-wincon`, `colorchoice`, `is_terminal_polyfill`, `once_cell_polyfill`, `utf8parse`, `windows-sys 0.61.2`, `windows-link` (all from `color`) and `strsim 0.11.1` (from `suggestions`) |

**Correction to the advisory number.** The advisory plan said the default-feature graph is 22
crates; the measured figure on this host is **21**. The claim it supports — that the minimal
feature set roughly halves the graph — is unchanged; the number is corrected here rather than
carried forward wrong.

**Five facts were measured in the probe, not assumed:**

- **`env!("CARGO_BIN_EXE_skein")` resolves to the built binary** from a `tests/*.rs` of the same
  package, and `std::process::Command` + `Stdio::piped()` drives it and pipes its stdin.
  `std::io::IsTerminal` and `std::process::ExitCode::from(1)` compile and behave at 1.97. **No
  CLI-testing dependency (`assert_cmd`, `predicates`, `trycmd`) is needed**, which is also why
  `docs/DEVELOPMENT.md`'s claim that `assert_cmd` is "pulled by Cargo on first build" is corrected
  at closeout: no crate in this workspace has ever referenced it.
- **The exact derive shape used in T6/T8 was compiled and run.** `skein secret set keychain://a/b
  --value hunter2` exits **2** with `error: unexpected argument '--value' found`. The absence of a
  `--value` flag is therefore machine-assertable, which is what makes FR-007 a test rather than a
  promise.
- `#[command(flatten)]`ing a `SiloArgs` struct into three sibling subcommands compiles and each
  subcommand accepts `--root`/`--silo`.
- **`#[command(bin_name = "skein")]` is required.** Without it, `skein ledger` renders
  `Usage: skein.exe ledger <COMMAND>` on Windows and `Usage: skein ledger …` elsewhere — measured
  both ways in the probe by toggling the attribute. With it, the output is identical on all three
  OSes, so a stderr assertion is not OS-dependent.
- **A `[[bin]]` target contributes a `running 0 tests` line** to `cargo test` output
  (`Running unittests src\main.rs … 0 passed`), so it does not inflate the suite count.

## Control baseline (T2)

`cargo test --workspace` on `011-skein-cli` @ `76450ed` (identical to `dev`), working tree clean,
2026-09-03, before any edit: **71 passing**, 0 failed, 0 ignored — `skein-acp/tests/acp_session.rs`
13, `skein-core/tests/core.rs` 12, `tests/native_loop.rs` 18, `tests/tool_gateway.rs` 9,
`skein-mcp/tests/rmcp_gateway.rs` 7, `skein-silo/tests/silo_ledger.rs` 7, `tests/silo_secret.rs` 5.
The four `src/lib.rs` unit-test targets and the four doc-test targets each contribute `0 passed`.
This is the number T9 diffs against.

## Observed red (Constitution III)

- **T3** `cargo test -p skein-core --test core`, 2026-09-03:
  - `error[E0599]: no method named runs found for struct Ledger in the current scope`
    (`crates\skein-core	ests\core.rs:285:13`)
  - `error: could not compile skein-core (test "core") due to 1 previous error`
- **T5** `cargo test -p skein-cli --test cli_ledger` against a `fn main() {}` stub, 2026-09-03:
  **8 failed, 0 passed.** The stub parses no arguments and prints nothing, so every test failed on
  its output or its exit code rather than on a compile error — unlike slices 007–010, whose red was
  an unresolved import. Two representative failures:
  - `e1_ledger_log_prints_every_step_in_the_silo` — `assertion left == right failed; left: ""`
    against the three expected four-column lines.
  - `e6_ledger_verify_fails_on_a_forged_row` — `left: Some(0), right: Some(1)`: the stub exits 0,
    so a forged chain looked verified. That is the failure mode the test exists for.
- **T7** `cargo test -p skein-cli --test cli_secret`, 2026-09-03: **1 failed, 1 passed.**
  - `e8_secret_set_then_delete_round_trips_without_printing_the_value` — the real red:
    `error: secret: keychain://skein-cli-test-<pid>-0/cli: not implemented`, from the placeholder
    `src/secret.rs` T6 left behind.
  - `e9_secret_set_has_no_value_flag_and_refuses_an_empty_secret` **passed before its product code
    existed**, and this is recorded rather than glossed. Both its halves were already satisfied:
    clap rejects `--value` with exit 2 purely from the argument tree T6 landed (the property T1
    pinned), and the placeholder's `SkeinError::Secret` happened to produce the `secret:` prefix
    and exit 1 the empty-stdin half asserts. A test that cannot fail proves nothing, so e9 was
    re-run after T8 replaced the placeholder — where it now passes against `read_value`'s own
    emptiness refusal, and the `--value` half remains a genuine guard against the flag being added
    later.

## Gate run (T9)

2026-09-03, Windows leg observed locally; macOS and Linux legs unobserved until the repository has
a remote (SC-001).

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, no lint raised on the new crate.
- `cargo test --workspace` — **82 passing**, 0 failed, 0 ignored: 71 pre-existing + 1 core seam
  test + 10 CLI process tests. Per binary: `acp_session` 13, `cli_ledger` 8, `cli_secret` 2,
  `core` 13, `native_loop` 18, `tool_gateway` 9, `rmcp_gateway` 7, `silo_ledger` 7,
  `silo_secret` 5. The new `[[bin]]` target contributes a `Running unittests src\main.rs …
  0 passed` line, as T1 measured, so it does not inflate the count.
- **82, not the advisory plan's 81.** The extra test is
  `e10_the_silo_root_falls_back_to_skein_root_and_is_required`, which covers FR-005: `--root`
  wins, `$SKEIN_ROOT` is the fallback, and neither is a loud exit 1 rather than a silent default.
  The advisory plan enumerated nine CLI tests and did not include one, but it also specified the
  `--root` → `$SKEIN_ROOT` → error precedence as product behaviour in its step 7. Shipping a
  documented resolution rule with no test — where a process test costs four lines and the failure
  mode (silently defaulting somewhere) is invisible — was the worse of the two deviations. It is a
  process test of a real contract, not one of the three shapes the plan's "no padding" clause
  excludes (no test of clap itself, no test of `--help` text, no unit test of an inner formatter).
- `cargo build --workspace` — clean; `target/debug/skein.exe` exists (7.3 MB) and runs.
  `skein --help` prints `Usage: skein <COMMAND>` with the `ledger` and `secret` groups, and
  `skein --version` prints `skein 0.0.0` (SC-002). **`Usage: skein`, not `skein.exe`** — the
  `bin_name` attribute T1 pinned, observed in the shipped binary and not only in the probe.
- The two `cli_secret` tests ran against the **real** Windows Credential Manager under service
  names unique per process and per test (`skein-cli-test-<pid>-<n>`), each removed by a `Drop`
  guard. `cmdkey /list` afterwards matches nothing containing `skein`, so the suite leaves the
  developer's credential store as it found it.

**The one path no automated test covers**, as foreseen: the terminal-stdin refusal, because the
harness has no PTY. It was exercised by hand instead — `Start-Process skein.exe secret set
keychain://manual/probe` with a console stdin and only the output streams redirected:

```
EXITCODE=1
error: secret: refusing to read a secret from a terminal: pipe it instead, e.g. `printf %s "$TOKEN" | skein secret set <REFERENCE>`
```

stdout was empty and nothing was stored. `is_terminal()` on stdin has no `#[cfg]` in this code, so
the macOS and Linux behaviour follows from `std`, not from a per-OS branch of ours.

## Control diff (T10)

## Drift (T11)

## Next slice (not this feature)
