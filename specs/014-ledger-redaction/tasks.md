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
- [x] **T2** RED→GREEN — `Redactor::redact_json` and `impl Clone for Redactor`, with their tests in
      `crates/skein-core/tests/core.rs`. First because nothing else compiles without them
- [x] **T3** RED→GREEN — `NativeLoop`'s fourth constructor argument and the two `redact_json` calls
      in `run`, with all 26 call sites updated in the same atomic commit. Its red is T4's tests
- [x] **T4** RED (written before T3's green) — the three new tests in
      `crates/skein-core/tests/native_loop.rs`, plus the additive `ScriptedModel.seen` field
- [x] **T5** RED→GREEN — the tool-name redaction in `ToolGateway::call_captured`
- [x] **T6** GREEN — `SkeinSession::new` clones the injected redactor into both collaborators
- [x] **T7** RED→GREEN — one test in `crates/skein-acp/tests/acp_session.rs` proving a session's
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

All on 2026-09-03.

- **T2** `cargo test -p skein-core --test core` before either addition existed — **3 compile
  errors**, and the file did not build:
  - `error[E0599]: no method named redact_json found for struct Redactor in the current scope`,
    twice, each with `help: there is a method redact with a similar name`
  - `error[E0599]: no method named clone found for struct Redactor in the current scope`
  - `error: could not compile skein-core (test "core") due to 3 previous errors`
  - Green: **17 passed** where 15 had passed, with the fifteen unchanged.

- **T4** `cargo test -p skein-core --test native_loop` with the three new tests written against a
  signature that did not exist yet — **3 compile errors**, one per new construction site:
  - `error[E0061]: this function takes 3 arguments but 4 arguments were supplied`, at
    `tests/native_loop.rs:864`, `:909` and `:962`, each pointing at
    `crates/skein-core/src/native_loop.rs:40`
  - `error: could not compile skein-core (test "native_loop") due to 3 previous errors`
  - Committed red, the way slice 013 committed `cli_acp_agent.rs` before its subcommand existed.

- **T3's green** turned T4's three red tests green on the first run with no change to the test
  file: `cargo test --workspace` went to **115 passed, 1 ignored** (110 baseline + 2 from T2 + 3
  from T4). All twenty pre-existing `native_loop.rs` bodies are unchanged —
  `git diff` on that file shows nine deleted lines, every one of them a single-line
  `NativeLoop::new(model, probe, no_tools());` that rustfmt rewrapped once it grew a fourth
  argument, and no deleted assertion anywhere.

- **T5** `cargo test -p skein-core --test tool_gateway` before the three recorded copies were
  scrubbed — **1 failed, 9 passed**, and the failure printed the leak verbatim:
  - `no captured payload may contain the secret: ["{\"tool\":\"read_sk-SECRET-abc123\",\"args\":{}}",
    "{\"tool\":\"read_sk-SECRET-abc123\",\"decision\":\"denied\",\"reason\":\"tool is not in the
    allowlist\"}"]` — both the `ToolCall` attempt and the `ApprovalRecord`, exactly the two the
    request's description did not mention and the plan added.
  - Green: **10 passed** in that target, with the nine unchanged.

- **T7 was written and committed before T6**, inverting the plan's numbering. The plan says T6 is a
  green "covered by T7's test", which would have left T6 with no red of its own; Constitution III
  wants the red observed. So T3 handed `SkeinSession::new`'s loop an empty `Redactor` — the
  behaviour of the tree before this slice, not a pretend one — T7's test failed on it, and T6 made
  it pass.
  - `cargo test -p skein-acp --test acp_session a10` before T6: **1 failed**, printing the whole
    chain — `the redactor the operator injected governs the whole chain: ["1",
    "{\"run_id\":\"skein-1#1\",…\"my key is sk-SECRET-abc123\"…}",
    "{…\"your key sk-SECRET-abc123 is fine\"…}", "1", "FinalOutput"]`
  - Green after T6: **15 passed** in that target where 14 had passed, the fourteen unchanged.

## Gate run (T9)

_Recorded at T9._
