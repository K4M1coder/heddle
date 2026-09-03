# Tasks: redaction on the `LlmRequest`/`LlmResponse` Ledger path (v0 slice)

**Spec:** `specs/014-ledger-redaction/spec.md` · **Plan:** `specs/014-ledger-redaction/plan.md` ·
TDD (red→green), product code in `crates/skein-core`, `crates/skein-acp` and `crates/skein-cli`,
branch `014-ledger-redaction` cut from `dev` after slice 013 merged.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the fix lives in `skein-core`'s governed loop and gateway; the CLI adds one
  flag and no capability — `RedactArgs::redactor()` is `Redactor::resolve` plus argument parsing ·
  II Local-first ✅ NON-NEGOTIABLE and unchanged: `OsKeychain` reports `requires_network() == false`,
  the loopback guard still runs first in both commands, and no new egress path exists
- III Test-First ✅ every step's red observed and recorded in `## Observed red` before its green,
  including the one step (T3) whose red is a test written in the step after it · IV Inverted
  coupling ✅ `skein-core` names no credential store: the loop takes a `Redactor`, and the CLI
  resolves references through `SecretProvider`. `NativeLoop` does not reach into `ToolGateway` for a
  redactor — both collaborators stay independently injectable
- V Traceability ✅ **closed, not carried forward.** The chain still holds the translated
  `TurnRequest`/`TurnResponse` and it is now redacted, so `skein ledger show` cannot print a
  configured conversation secret. Redaction happens **before** `Ledger::append`, so the hash chain
  stays a pure function of its payloads and a run replays identically
- VI Security ✅ deny-by-default: the redactor is a **required** fourth constructor argument, so "no
  redaction" cannot be a silent default. Secrets stay by reference — `--redact` takes
  `keychain://…`, there is no `--redact-value`, and one unresolvable reference fails the whole run
  before a chain is opened. The tool *name* is redacted too, because it is model-chosen text
- VII Neutrality ✅ one public method (`Redactor::redact_json`), one `Clone` impl, one repeatable
  flag, zero new packages, zero new dependency edges. No `Arc`, no `ToolGateway::new` signature
  change, no config file
- VIII Loop discipline ✅ NON-NEGOTIABLE and untouched: the budget, the probe and the exits are the
  same. The model still receives the **raw** request and the caller still gets the **raw** final
  message, so redaction cannot change what the loop decides
- Cross-platform ✅ no `#[cfg]` in any new code. The two keychain-touching CLI tests follow
  `cli_secret.rs`'s already-green tri-OS `Drop`-guarded pattern, and `RedactArgs::redactor()` never
  opens the credential store when `--redact` is absent, so the nine pre-existing CLI tests keep
  running headless. `core.yml`'s `paths:` already covers `crates/**`; no workspace member is added

## Tasks
- [x] **T0** `specs/014-ledger-redaction/{spec.md,plan.md,tasks.md}`; branch `014-ledger-redaction`
      cut from `dev` with slice 013 merged
- [x] **T1** control baseline: `cargo test --workspace` before any edit — **110 passed, 1 ignored**
- [ ] **T2** RED→GREEN — `Redactor::redact_json` and `impl Clone for Redactor`, with their tests in
      `crates/skein-core/tests/core.rs`. First because nothing else compiles without them
- [ ] **T3** RED→GREEN — `NativeLoop`'s fourth constructor argument and the two `redact_json` calls
      in `run`, with all 26 call sites updated in the same atomic commit. Its red is T4's tests
- [ ] **T4** RED (written before T3's green) — the three new tests in
      `crates/skein-core/tests/native_loop.rs`, plus the additive `ScriptedModel.seen` field
- [ ] **T5** RED→GREEN — the tool-name redaction in `ToolGateway::call_captured`
- [ ] **T6** GREEN — `SkeinSession::new` clones the injected redactor into both collaborators
- [ ] **T7** RED→GREEN — one test in `crates/skein-acp/tests/acp_session.rs` proving a session's
      chain is redacted and pinning the `project_updates` consequence
- [ ] **T8** RED→GREEN — `wiring::RedactArgs`, flattened into `ChatArgs` and `AcpAgent`, resolved in
      `chat.rs` and `acp.rs` after the endpoint guard and before `Silo::open`
- [ ] **T9** gates, control diff, dependency drift, close-out

## Control baseline (T1)

`cargo test --workspace` on `014-ledger-redaction` @ `03e1c22` (identical to `dev`), working tree
clean apart from this slice's three spec files, 2026-09-03, before any code edit: **110 passed, 0
failed, 1 ignored** — `acp_session` 14, `cli_acp_agent` 3, `cli_chat` 6, `cli_ledger` 8, `cli_secret`
2, `core` 15, `native_loop` 18, `tool_gateway` 9, `governed_run` 2, `openai_compat` 14 (+1 ignored,
the optional live-Ollama test), `rmcp_gateway` 7, `silo_ledger` 7, `silo_secret` 5. The five
`src/lib.rs`/`src/main.rs` unit-test targets and the five doc-test targets each contribute
`0 passed`. This matches slice 013's recorded gate figure exactly, and it is the number T9 diffs
against.

## Observed red (Constitution III)

_Recorded per step._

## Gate run (T9)

_Recorded at T9._
