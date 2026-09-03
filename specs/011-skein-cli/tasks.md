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
- [ ] **T1** pin the `clap` surface against the vendored `clap 4.6.6` source and a compiled probe,
      *before* any product code
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

## Control baseline (T2)

## Pinned `clap` surface (T1)

## Observed red (Constitution III)

## Gate run (T9)

## Control diff (T10)

## Drift (T11)

## Next slice (not this feature)
