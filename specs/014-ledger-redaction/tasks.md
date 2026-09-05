# Tasks: redaction on the `LlmRequest`/`LlmResponse` Ledger path (v0 slice)

**Spec:** `specs/014-ledger-redaction/spec.md` · **Plan:** `specs/014-ledger-redaction/plan.md` ·
TDD (red→green), product code in `crates/heddle-core`, `crates/heddle-acp` and `crates/heddle-cli`,
branch `014-ledger-redaction` cut from `dev` after slice 013 merged.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the fix lives in `heddle-core`'s governed loop and gateway; the CLI adds one
  flag and no capability — `RedactArgs::redactor()` is `Redactor::resolve` plus argument parsing ·
  II Local-first ✅ NON-NEGOTIABLE and unchanged: `OsKeychain` reports `requires_network() == false`,
  the loopback guard still runs first in both commands, and no new egress path exists
- III Test-First ✅ every step's red observed and recorded in `## Observed red` before its green,
  including the one step (T3) whose red is a test written in the step after it · IV Inverted
  coupling ✅ `heddle-core` names no credential store: the loop takes a `Redactor`, and the CLI
  resolves references through `SecretProvider`. `NativeLoop` does not reach into `ToolGateway` for a
  redactor — both collaborators stay independently injectable
- V Traceability ✅ **closed, not carried forward.** The chain still holds the translated
  `TurnRequest`/`TurnResponse` and it is now redacted, so `heddle ledger show` cannot print a
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
      `crates/heddle-core/tests/core.rs`. First because nothing else compiles without them
- [x] **T3** RED→GREEN — `NativeLoop`'s fourth constructor argument and the two `redact_json` calls
      in `run`, with all 26 call sites updated in the same atomic commit. Its red is T4's tests
- [x] **T4** RED (written before T3's green) — the three new tests in
      `crates/heddle-core/tests/native_loop.rs`, plus the additive `ScriptedModel.seen` field
- [x] **T5** RED→GREEN — the tool-name redaction in `ToolGateway::call_captured`
- [x] **T6** GREEN — `HeddleSession::new` clones the injected redactor into both collaborators
- [x] **T7** RED→GREEN — one test in `crates/heddle-acp/tests/acp_session.rs` proving a session's
      chain is redacted and pinning the `project_updates` consequence
- [x] **T8** RED→GREEN — `wiring::RedactArgs`, flattened into `ChatArgs` and `AcpAgent`, resolved in
      `chat.rs` and `acp.rs` after the endpoint guard and before `Silo::open`
- [x] **T9** gates, control diff, dependency drift, close-out

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

- **T2** `cargo test -p heddle-core --test core` before either addition existed — **3 compile
  errors**, and the file did not build:
  - `error[E0599]: no method named redact_json found for struct Redactor in the current scope`,
    twice, each with `help: there is a method redact with a similar name`
  - `error[E0599]: no method named clone found for struct Redactor in the current scope`
  - `error: could not compile heddle-core (test "core") due to 3 previous errors`
  - Green: **17 passed** where 15 had passed, with the fifteen unchanged.

- **T4** `cargo test -p heddle-core --test native_loop` with the three new tests written against a
  signature that did not exist yet — **3 compile errors**, one per new construction site:
  - `error[E0061]: this function takes 3 arguments but 4 arguments were supplied`, at
    `tests/native_loop.rs:864`, `:909` and `:962`, each pointing at
    `crates/heddle-core/src/native_loop.rs:40`
  - `error: could not compile heddle-core (test "native_loop") due to 3 previous errors`
  - Committed red, the way slice 013 committed `cli_acp_agent.rs` before its subcommand existed.

- **T3's green** turned T4's three red tests green on the first run with no change to the test
  file: `cargo test --workspace` went to **115 passed, 1 ignored** (110 baseline + 2 from T2 + 3
  from T4). All twenty pre-existing `native_loop.rs` bodies are unchanged —
  `git diff` on that file shows nine deleted lines, every one of them a single-line
  `NativeLoop::new(model, probe, no_tools());` that rustfmt rewrapped once it grew a fourth
  argument, and no deleted assertion anywhere.

- **T5** `cargo test -p heddle-core --test tool_gateway` before the three recorded copies were
  scrubbed — **1 failed, 9 passed**, and the failure printed the leak verbatim:
  - `no captured payload may contain the secret: ["{\"tool\":\"read_sk-SECRET-abc123\",\"args\":{}}",
    "{\"tool\":\"read_sk-SECRET-abc123\",\"decision\":\"denied\",\"reason\":\"tool is not in the
    allowlist\"}"]` — both the `ToolCall` attempt and the `ApprovalRecord`, exactly the two the
    request's description did not mention and the plan added.
  - Green: **10 passed** in that target, with the nine unchanged.

- **T7 was written and committed before T6**, inverting the plan's numbering. The plan says T6 is a
  green "covered by T7's test", which would have left T6 with no red of its own; Constitution III
  wants the red observed. So T3 handed `HeddleSession::new`'s loop an empty `Redactor` — the
  behaviour of the tree before this slice, not a pretend one — T7's test failed on it, and T6 made
  it pass.
  - `cargo test -p heddle-acp --test acp_session a10` before T6: **1 failed**, printing the whole
    chain — `the redactor the operator injected governs the whole chain: ["1",
    "{\"run_id\":\"heddle-1#1\",…\"my key is sk-SECRET-abc123\"…}",
    "{…\"your key sk-SECRET-abc123 is fine\"…}", "1", "FinalOutput"]`
  - Green after T6: **15 passed** in that target where 14 had passed, the fourteen unchanged.

- **T8** the three new CLI tests, written before the flag existed:
  - `cargo test -p heddle-cli --test cli_chat` — **2 failed, 6 passed**, both on
    `error: unexpected argument '--redact' found`, clap's exit **2** where the tests want 0 and 1.
  - `cargo test -p heddle-cli --test cli_acp_agent` — **1 failed, 3 passed**:
    `assertion left == right failed / left: Some(2) / right: Some(1)`, the same clap refusal.
  - Green: `cli_chat` **6 → 8**, `cli_acp_agent` **3 → 4**, `cli_ledger` 8 and `cli_secret` 2
    unchanged. `heddle chat --help` and `heddle acp-agent --help` both list
    `--redact <REFERENCE>`.
  - `Redactor::new(…)` no longer appears at either wiring site. The one remaining construction in
    `crates/heddle-cli/src` is inside `RedactArgs::redactor()` itself — the empty redactor for the
    no-flag case, which is the whole point of that branch: with no `--redact`, the credential store
    is never opened.

## Gate run (T9)

2026-09-03, Windows leg observed locally; macOS and Linux legs unobserved until the repository has a
remote (the standing caveat of specs 004–013).

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, no warning on any of the six
  crates.
- `cargo test --workspace` — **120 passed, 0 failed, 1 ignored**: the 110 baseline plus ten, and
  every one of the 110 with an unchanged body. `core` 15→**17**, `native_loop` 18→**21**,
  `tool_gateway` 9→**10**, `acp_session` 14→**15**, `cli_chat` 6→**8**, `cli_acp_agent` 3→**4**.
  Unchanged: `cli_ledger` 8, `cli_secret` 2, `governed_run` 2, `openai_compat` 14 (+1 ignored),
  `rmcp_gateway` 7, `silo_ledger` 7, `silo_secret` 5.
  - **Ten new tests, not the eight the plan predicted.** The plan's Validation section numbers eight
    and its T2 describes a single `core.rs` test carrying two unrelated claims — that `redact_json`
    keeps a payload's shape, and that a cloned `Redactor` scrubs what the original does. Those are
    two claims with two failure modes, so they are two tests. 110 + 8 + 2 = 120.
- `cargo build --workspace` — clean.
- `heddle chat --help` and `heddle acp-agent --help` both list `--redact <REFERENCE>` with the help
  text `keychain://<service>/<account>. Repeatable`.

## Control diff (T9)

`git diff dev --stat -- crates/heddle-silo/ spikes/ .github/ rust-toolchain.toml` is **one line**:
`crates/heddle-silo/tests/silo_ledger.rs | 1 +`, the mechanical fourth `NativeLoop::new` argument the
plan named in advance. Stated as the exception rather than claimed as an empty diff. `spikes/` is
untouched (ADR-0004 D2), and so are `.github/` and `rust-toolchain.toml`.

`git diff dev --stat -- crates/heddle-gateway/src crates/heddle-mcp/src` is **empty**: neither crate's
product code moved, only their test binaries' constructor calls.

Over the whole branch, **1395 insertions and 32 deletions** across 19 files. The 32 deletions are
accounted for in four places and nowhere else:

- `crates/heddle-core/tests/native_loop.rs` — nine single-line `NativeLoop::new(model, probe,
  no_tools());` calls that rustfmt rewrapped once they grew a fourth argument. No assertion moved.
- `crates/heddle-mcp/tests/rmcp_gateway.rs` — the same, once.
- `crates/heddle-core/src/{native_loop.rs,tool.rs}` — the two `ledger.append` arguments and the three
  `call.tool.clone()` copies this slice exists to change, plus their comments.
- `crates/heddle-cli/src/{chat.rs,acp.rs,main.rs,wiring.rs}` — the two `Redactor::new(vec![])`
  wiring sites, the `serve` signature, and one import line per file.

## Drift (T9)

**Zero new packages and zero new edges — by construction, not by measurement.**
`git diff dev -- Cargo.toml crates/*/Cargo.toml` is **empty**: no manifest in the workspace changed,
so the graph `cargo` resolves is the same graph it resolved on `dev`. `heddle-cli` already declared
`heddle-silo` (for `Silo` and `OsKeychain`) and `heddle-core`; `RedactArgs` needed nothing new. `Arc`
was rejected in the plan's D3 and `std` needs no declaration.

No toolchain change and no new build prerequisite: no crate entered the graph, so nothing can have
raised the MSRV, and `rust-toolchain.toml`, `workspace.package.rust-version` and
`.github/workflows/core.yml` are untouched.

## Deviations from the plan

Two, both recorded rather than absorbed:

1. **T7 was implemented before T6**, inverting the plan's numbering — see `## Observed red`. The
   plan makes T6 a green whose only cover is T7's test, which would have meant a green with no
   observed red. The behaviour of the tree between T3 and T6 is the behaviour it had on `dev`, so
   nothing was faked to produce the red.
2. **Ten new tests rather than eight**, for the reason recorded in the gate run above.

## Out of scope

Deliberately not done, so no one helpfully does it:

- **Raw wire-byte capture** — the HTTP request and response bodies `heddle-gateway` exchanges. A
  separate, already-named next-slice item, and the thing design §4.5's "exact model I/O" actually
  asks for.
- **Provider authentication**, a provider token as a `SecretRef`, sampling parameters, a config file.
- **Automatic secret detection.** `Redactor` is an exact-value scrubber and stays one: a credential
  the operator never registered and never named with `--redact` still lands in cleartext.
- **Redacting `HeddleError`, stderr, or `heddle chat`'s stdout.** The invariant is about Ledger
  payloads; `chat`'s stdout carrying the raw answer is the contract test 6 pins.
- **Changing `ToolGateway::new`'s signature**, adding `Arc` anywhere, making `SecretValue: Clone`,
  or putting the `Redactor` on `Ledger` (plan D1 says why the last one loses).
- **`heddle-silo`'s and `heddle-gateway`'s product code**, `spikes/`, `.github/`,
  `rust-toolchain.toml`.

## Next slice (not this feature)
- [ ] **raw-wire-byte capture** — a `StepKind` for the provider's literal request and response
      bytes. Redaction on that path is the same `redact_json`/`redact` pair, but the payload is not
      a `TurnRequest`, so the capture has to exist first.
- [ ] **tool advertisement** — a `tools` field on `TurnRequest`, which needs tool discovery from the
      Tool Gateway. Still the largest untested-in-production path in the tree.
- [ ] **streaming (SSE)** with incremental ACP `AgentMessageChunk` notifications. Today a client
      still sees one chunk per turn, after the turn — and now a redacted one.
- [ ] **provider authentication**: an `Authorization: Bearer` whose value arrives as a `SecretRef`
      resolved through `SecretProvider`, which is the first thing that would want `--redact`'s
      reference list to be shared with the model client.
- [ ] a config file holding the base URL, the model, a default silo root **and the run's redaction
      references**, so `--redact` is not the only way to name them.
- [ ] the egress-policy layer and ADR-0002 D4's process-level socket-deny boundary.
