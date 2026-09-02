# Tasks: native turn loop + ModelClient port (v0 strict-local)

**Spec:** `specs/004-native-loop/spec.md` · TDD (red→green), product code in
`crates/skein-core`, branch `004-native-loop` cut from `dev`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ (library API; no UI, no bin) · II Local-first ✅ (no network, no new deps)
- III Test-First ✅ (T2 red before T3–T7 green) · IV Inverted coupling ✅ (`ModelClient` +
  `ProgressProbe` traits; the core never names a provider) · V Traceability ✅ (every turn
  through the existing `Ledger::append`; `verify_chain` asserted, including on the error path)
- VI Security n/a this slice (no secrets, no external content) · VII Neutrality ✅ (zero new
  dependencies; hand-rolled test doubles over a mocking crate)
- VIII Loop discipline ✅ **(a)** `LoopController` alone terminates; four Exit variants proven
  through real wiring; zero-budget makes zero calls. **(b)** `ProgressProbe::observe()` takes no
  model output, so self-judgment is unrepresentable. **(c)** action/iteration verification via
  per-step Ledger capture; terminal verification via the `Exit` step. **(d)** HITL escalation
  deferred with `Exit::HumanReject` (see spec Assumptions).
- Cross-platform ✅ (pure Rust, no `#[cfg]`; tri-OS CI for `crates/` **added by T9** —
  `spikes.yml` covered only `spikes/**`).

## Done
- [ ] **T0** `specs/004-native-loop/{spec.md,plan.md,tasks.md}` + branch from `dev`
- [ ] **T1** `SkeinError::Model(String)` (FR-006)
- [ ] **T2** RED — `crates/skein-core/tests/native_loop.rs` with `ScriptedModel`/`ScriptedProbe`
      and all 9 tests; compile failure observed and recorded
- [ ] **T3** `model` — `ModelClient`/`TurnRequest`/`TurnResponse` (FR-001)
- [ ] **T4** `native_loop` — `ProgressProbe`/`LoopRun`/`NativeLoop::run` turn algorithm
      (FR-002/003/004/005)
- [ ] **T5** pre-flight budget guard: exhausted budget ⇒ zero model calls (FR-003)
- [ ] **T6** all four `Exit` variants reached through `NativeLoop::run` (SC-002)
- [ ] **T7** provider error ⇒ `Err` with a still-verifiable chain (FR-006)
- [ ] **T8** `fmt --check`, `clippy -D warnings`, `cargo test -p skein-core`;
      no dependency drift (SC-003)
- [ ] **T9** `.github/workflows/core.yml` — tri-OS fmt/clippy/test for the workspace
      (ADR-0004 D1(d), SC-004)
- [ ] **T10** tick spec 003's "Next slice" first bullet; set this spec's Status

## Next slice (not this feature)
- [ ] `rmcp` Tool Gateway (promote Spike 4) + ACP client facade
- [ ] silo-backed durable Ledger (SQLite) + `SecretProvider` (OS keychain)
- [ ] `skein-cli` reference client
