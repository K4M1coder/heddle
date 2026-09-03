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
- [ ] **T0** `specs/011-skein-cli/{spec.md,plan.md,tasks.md}`; branch `011-skein-cli` cut from
      `dev` with slice 010 merged
- [x] **T1** pinned the `clap` surface against the vendored `clap 4.6.6` source and a compiled probe,
      *before* any product code; measured, not copied — see below
- [ ] **T2** control baseline: `cargo test --workspace` before any edit — **71**
- [ ] **T3** RED — `ledger_runs_lists_run_ids_in_first_append_order` in
      `crates/skein-core/tests/core.rs` against the not-yet-existing `Ledger::runs()`
- [ ] **T4** GREEN — `Ledger::runs()` in `crates/skein-core/src/ledger.rs`
- [ ] **T5** RED — `crates/skein-cli/tests/cli_ledger.rs` (e1..e7) against a `fn main() {}` stub
- [ ] **T6** GREEN — `src/main.rs` + `src/ledger.rs`
- [ ] **T7** RED — `crates/skein-cli/tests/cli_secret.rs` (e8..e9)
- [ ] **T8** GREEN — `src/secret.rs`
- [ ] **T9** gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
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

## Observed red (Constitution III)

## Gate run (T9)

## Control diff (T10)

## Drift (T11)

## Next slice (not this feature)
