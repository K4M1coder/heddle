# Tasks: heddle-core foundation (v0 strict-local)

**Spec:** `specs/003-heddle-core-foundation/spec.md` · TDD, product code under `crates/` (first non-quarantine code; authorized for this slice by owner delegation 2026-07-16, ADR-0004 D3).

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ (library, no UI) · II Local-first ✅ (no network) · III Test-First ✅ (6 tests) · IV Inverted coupling ✅ (types/traits only) · V Traceability ✅ (Ledger §4.11) · VI Security n/a this slice (no secrets yet) · VII Neutrality ✅ · VIII Loop discipline ✅ (LoopController §4.14). Cross-platform ✅ (pure Rust; tri-OS CI mechanized in `.github/workflows/`).

## Done
- [x] **T1** Root Cargo workspace (`crates/*`, excl. `spikes`) + `rust-toolchain.toml` (1.79)
- [x] **T2** `content` — `Content`/`Message`/`Role` + serde round-trip test
- [x] **T3** `error` — `HeddleError`/`Result`
- [x] **T4** `ledger` — append-only, SHA-256 hash-chained, per-run isolation, `verify_chain` + tamper-detection test (FR-002/003)
- [x] **T5** `loop_ctl` — `LoopBudget`/`Exit`/`LoopController` with iteration/token/no-progress budgets + ground-truth progress (FR-004)
- [x] **T6** `cargo test -p heddle-core` (6/6), `clippy -D warnings` clean, `fmt --check` clean

## Next slice (not this feature)
- [x] `ModelClient` trait + native loop (promote Spike 1 Option A) writing each turn to the Ledger via LoopController — `specs/004-native-loop/`
- [x] `rmcp` Tool Gateway (promote Spike 4) — `specs/005-tool-gateway/`
- [x] ACP client facade over the native loop + gateway — `specs/008-acp-facade/`, `crates/heddle-acp`
- [x] silo-backed durable Ledger (SQLite) — `specs/009-silo-ledger/`, `crates/heddle-silo`
- [x] `SecretProvider` (OS keychain) + JIT `Redactor` — `specs/010-secret-provider/`
- [x] `heddle-cli` reference client — `specs/011-heddle-cli/`, `crates/heddle-cli` (`heddle ledger log|show|verify`, `heddle secret set|delete`)
