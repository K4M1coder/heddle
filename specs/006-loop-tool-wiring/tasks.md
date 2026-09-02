# Tasks: wire the Tool Gateway into the native loop (v0 strict-local)

**Spec:** `specs/006-loop-tool-wiring/spec.md` · TDD (red→green), product code in
`crates/skein-core`, branch `006-loop-tool-wiring` cut from `dev`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ (library API; no UI, no bin — and this is the slice that lets the headless
  core actually call a tool mid-conversation) · II Local-first ✅ (no network, no new deps, no
  `Cargo.toml` touched)
- III Test-First ✅ (T2 red before T3–T5 green) · IV Inverted coupling ✅ (the loop is generic
  over the *existing* `T: ToolTransport` seam and never names a transport; a `ToolMediator`
  trait was rejected because it would make the governed step mockable)
- V Traceability ✅ (tool steps through the existing `Ledger::append` with the existing
  `StepKind`s, on the run's one chain; `verify_chain` asserted on the interleaved path and the
  transport-error path)
- VI Security ✅ (the loop feeds back the gateway's **redacted** capture, so a tool's secret
  cannot re-enter the chain through the next turn's `LlmRequest` history; tool output enters as
  `Role::User` data with a marker, never as `System` instruction — the marker is explicitly not
  claimed as an injection boundary) · VII Neutrality ✅ (no new module, crate, `StepKind`,
  dependency, null-object transport or second loop entry point)
- VIII Loop discipline ✅ **(a)** `record_iteration` and `should_exit` still called exactly once
  per boundary in the same fixed order; `loop_ctl.rs` byte-identical; a tool-bearing turn buys
  no extra iteration and an exhausted budget makes zero model *and* zero tool calls.
  **(b)** tools run *before* `probe.observe()` so the ground-truth probe can see the effect of
  the turn's own tool — design §4.14 names tool results as a reflection anchor, and probing
  first would trip `Exit::NoProgress` on tool-driven runs. **(c)** per-step Ledger capture now
  covers the tool attempt, the decision and the result; terminal verification is still the
  `Exit` step. **(d)** HITL escalation still deferred: approval is a configured list.
- Cross-platform ✅ (pure Rust, no `#[cfg]`; `core.yml`'s `paths:` already covers `crates/**`
  and its toolchain pin is already 1.97 — confirmed, not edited).

## Done
- [x] **T0** `specs/006-loop-tool-wiring/{spec.md,plan.md,tasks.md}` + branch from `dev`
- [x] **T1** `ToolGateway::call_captured` — the governed body moved verbatim, `call` reduced to
      a delegate; no behaviour change, all 26 tests still green (FR-004)
- [x] **T2** RED — `RecordingTransport`, `no_tools()`, `reply_with_tools()` and all 8 new tests
      in `crates/skein-core/tests/native_loop.rs`, plus the 9th in
      `crates/skein-mcp/tests/rmcp_gateway.rs`; compile failure observed and recorded
- [x] **T3** `TurnResponse.tool_calls` — serde-defaulted, and the stale `model.rs` doc comment
      corrected (FR-001)
- [x] **T4** `NativeLoop<C, P, T>` carries a `pub gateway: ToolGateway<T>` injected at
      construction (FR-002)
- [x] **T5** mediation inside `run`: gateway per call in declaration order, between the turn's
      `BudgetSpent` append and `probe.observe()`; redacted feedback as `Role::User`; denial
      survives, any other tool error propagates (FR-002/003/004/005/006)
- [x] **T6** `fmt --check`, `clippy --workspace --all-targets -D warnings`,
      `cargo test --workspace` — **35/35** (spec 003's 6 + spec 004's 9 + spec 005's 6 gateway
      + 5 rmcp + this slice's 8 loop + 1 rmcp), 2026-09-03. Windows leg observed; macOS and
      Linux unobserved until the repository has a remote (SC-001)
- [x] **T7** no dependency drift: `git diff dev` on every `Cargo.toml` empty (FR-007, SC-003)
- [x] **T8** CI: `core.yml` already covers `crates/**` at toolchain 1.97 — confirmed, unedited;
      `git diff dev -- spikes/` empty (SC-005, ADR-0004 D2)
- [x] **T9** control diff: `tests/tool_gateway.rs` and `src/loop_ctl.rs` unchanged; spec 004's
      four Exit-variant tests differ only by the added `NativeLoop::new` argument (SC-004)
- [x] **T10** tick the loop-wiring bullet in `specs/005-tool-gateway/tasks.md`; set this spec's
      Status

## Observed red (Constitution III)

- **T2** `cargo build --workspace --all-targets`, 2026-09-03:
  - `error[E0560]: struct TurnResponse has no field named tool_calls` (×3)
  - `error[E0061]: this function takes 2 arguments but 3 arguments were supplied` (×20 —
    every `NativeLoop::new` site)
  - `error[E0609]: no field gateway on type NativeLoop<ScriptedModel, ScriptedProbe>` (×7)
  - `error: could not compile skein-core (test "native_loop") due to 28 previous errors`
  - `error: could not compile skein-mcp (test "rmcp_gateway") due to 2 previous errors`

## Next slice (not this feature)
- [ ] ACP client facade over the native loop + gateway
- [x] `ToolPolicy` allowlist for model-chosen tool names — with the model naming the tool,
      "anything not classified mutating is allowed" is a materially weaker posture than it was
      in slice 005 (spec Assumptions, R3) → `specs/007-tool-allowlist/`
- [ ] a typed `Content::ToolResult` variant and real prompt-injection defense; redacting the
      tool *name* on its way into the Ledger
- [ ] tool advertisement on `TurnRequest`, which needs tool discovery (`tools/list`)
- [ ] cost / wall-clock budgets and `Exit::Error`; the `ts`/`principal`/`silo` fields design
      §4.11 sketches on `Step`
- [ ] silo-backed durable Ledger (SQLite) + `SecretProvider` (OS keychain)
- [ ] `skein-cli` reference client
