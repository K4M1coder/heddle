# Tasks: tool advertisement on the `TurnRequest` path (v0 slice)

**Spec:** `specs/015-tool-advertisement/spec.md` · **Plan:** `specs/015-tool-advertisement/plan.md` ·
TDD (red→green), product code in `crates/skein-core`, `crates/skein-gateway` and `crates/skein-acp`,
branch `015-tool-advertisement` cut from `dev` after slice 014 merged.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the whole mechanism lives in `skein-core` — a type, a defaulted trait method,
  one gateway method and one loop call. `skein-gateway` gains only the translation of that type onto
  its own wire format, and `skein-acp` only a one-line forward. No CLI change and no new capability ·
  II Local-first ✅ NON-NEGOTIABLE and unchanged: no new egress path, no new dependency, the loopback
  guard and the no-TLS build property are untouched. Advertisement adds bytes to a request that was
  already going to the same local endpoint
- III Test-First ✅ every step's red observed and recorded in `## Observed red` before its green ·
  IV Inverted coupling ✅ `skein-core` still names no protocol: `advertise` asks a `ToolTransport`,
  which is the port, and `ToolSpec.parameters` is an opaque `serde_json::Value` because the schema is
  the server's document and the core never interprets it. `skein-gateway` remains the only crate
  naming the OpenAI wire format, `skein-mcp` the only one naming MCP
- V Traceability ✅ no new `StepKind` and none needed: the advertisement travels inside `TurnRequest`,
  which `run` already captures as `LlmRequest` through `Redactor::redact_json`, so tool descriptions
  and schemas are scrubbed by `redact_value`'s existing recursion. A run's captured request now shows
  exactly what the model was told it could do
- VI Security ✅ deny-by-default, and the one silent default in the slice is argued rather than
  assumed: an un-overridden `ToolTransport::list` advertises **nothing**. The policy filter lives
  inside `ToolGateway`, so "you cannot advertise what the policy forbids" is structural. An
  unapproved `Mutating` tool is still advertised **on purpose** — denying it at advertisement would
  disconnect `AcpPermissionTransport`, which is the only path to a human
- VII Neutrality ✅ one new type, one defaulted trait method, one gateway method, two serde
  attributes. Zero new packages, zero new dependency edges, no new crate, no config, no flag. No
  `ToolCatalog` trait and no hand-written schema anywhere
- VIII Loop discipline ✅ NON-NEGOTIABLE and untouched: `advertise` runs **after** the pre-flight
  `should_exit(false)` check, so a zero-budget run makes no round trip and the budget is still
  enforced before it is spent. The exits, the probe and the controller are unchanged
- Cross-platform ✅ no `#[cfg]` in any new code, no filesystem and no process work. No workspace
  member is added, so `core.yml`'s `paths: crates/**` needs no change

## Tasks
- [x] **T0** `specs/015-tool-advertisement/{spec.md,plan.md,tasks.md}`; branch
      `015-tool-advertisement` cut from `dev` with slice 014 merged
- [x] **T1** control baseline: `cargo test --workspace` before any edit — **120 passed, 1 ignored**
- [x] **T2** RED→GREEN — `ToolSpec` in `crates/skein-core/src/tool.rs`, re-exported from `lib.rs`,
      with its round-trip test in `crates/skein-core/tests/core.rs`
- [ ] **T3** RED→GREEN — `ToolTransport::list` with its defaulted body and the docstring arguing why
      *this* silent default is the safe one
- [ ] **T4** RED→GREEN — `ToolGateway::advertise`: list, filter to the allowlist, in allowlist order
- [ ] **T5** RED→GREEN — `TurnRequest.tools` and every literal construction site, one atomic commit
- [ ] **T6** RED→GREEN — `NativeLoop::run` advertises once per run and stamps the specs into every
      `TurnRequest`; a `list` failure is fatal
- [ ] **T7** RED→GREEN — `AcpPermissionTransport::list` forwards to its inner transport (the slice's
      highest-risk line, its own test)
- [ ] **T8** RED→GREEN — `ChatRequest.tools` in `crates/skein-gateway/src/lib.rs`, byte-exact
- [ ] **T9** gates, control diff, dependency drift, close-out

## Control baseline (T1)

`cargo test --workspace` on `015-tool-advertisement` @ `8860a01` (identical to `dev`), working tree
clean apart from this slice's three spec files, 2026-09-03, before any code edit: **120 passed, 0
failed, 1 ignored** — `acp_session` 15, `cli_acp_agent` 4, `cli_chat` 8, `cli_ledger` 8, `cli_secret`
2, `core` 17, `native_loop` 21, `tool_gateway` 10, `governed_run` 2, `openai_compat` 14 (+1 ignored,
the optional live-Ollama test), `rmcp_gateway` 7, `silo_ledger` 7, `silo_secret` 5. The five
`src/lib.rs`/`src/main.rs` unit-test targets and the five doc-test targets each contribute
`0 passed`. This matches slice 014's recorded gate figure exactly, and it is the number T9 diffs
against.

## Observed red (Constitution III)

All on 2026-09-03.

- **T2** `cargo test -p skein-core --test core` with the round-trip test written against a type that
  did not exist — **1 compile error**, and the file did not build:
  - `error[E0432]: unresolved import skein_core::ToolSpec` at `crates/skein-core/tests/core.rs:5:79`
    — `no ToolSpec in the root`
  - `error: could not compile skein-core (test "core") due to 1 previous error`
  - Green: **18 passed** where 17 had passed, with the seventeen unchanged.
