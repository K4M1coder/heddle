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
- [x] **T3** RED→GREEN — `ToolTransport::list` with its defaulted body and the docstring arguing why
      *this* silent default is the safe one
- [x] **T4** RED→GREEN — `ToolGateway::advertise`: list, filter to the allowlist, in allowlist order
- [x] **T5** RED→GREEN — `TurnRequest.tools` and every literal construction site, one atomic commit
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

- **T3** `cargo test -p skein-core --test tool_gateway` with the defaulted-body test written against
  a trait that had only `call` — **1 compile error**:
  - `error[E0599]: no method named list found for struct UnlistedTransport in the current scope` at
    `crates/skein-core/tests/tool_gateway.rs:70:27`
  - `error: could not compile skein-core (test "tool_gateway") due to 1 previous error`
  - Green: **11 passed** where 10 had passed, with the ten unchanged — the proof that a defaulted
    method left all nine pre-existing `impl ToolTransport` sites compiling untouched.

- **T4** `cargo test -p skein-core --test tool_gateway` with the three advertisement tests written
  against a gateway that had no such method — **3 compile errors**, one per new test:
  - `error[E0599]: no method named advertise found for struct ToolGateway<T> in the current scope`,
    at `crates/skein-core/tests/tool_gateway.rs:127`, `:152` and `:171`
  - `error: could not compile skein-core (test "tool_gateway") due to 3 previous errors`
  - Green: **14 passed** where 11 had passed, with the eleven unchanged. `CountingTransport` gained
    an additive `catalogue` field and an `offering` constructor; no existing body moved.

- **T5** `cargo test -p skein-core --test core` with the no-`tools`-key serialization test written
  against a two-field `TurnRequest` — **1 compile error**:
  - `error[E0560]: struct TurnRequest has no field named tools` at
    `crates/skein-core/tests/core.rs:361:9`
  - `error: could not compile skein-core (test "core") due to 1 previous error`
  - Then a **second, behavioural red** once the field existed, because the test's byte-exact literal
    guessed serde's default externally-tagged enum shape for `Content` rather than reading it:
    `left: "…\"parts\":[{\"type\":\"text\",\"text\":\"hello\"}]…"` /
    `right: "…\"parts\":[{\"Text\":{\"text\":\"hello\"}}]…"`. `Content` carries
    `#[serde(tag = "type", rename_all = "snake_case")]`. The expectation was corrected to the tree's
    actual wire shape; the claim under test — that **no `tools` key appears** — was never weakened.
  - Green: **19 passed** in `core` where 18 had passed, and `cargo test --workspace` reached
    **126 passed, 1 ignored**. The three literal construction sites gained `tools: Vec::new()` and
    nothing else; `openai_compat.rs`'s byte-exact no-tools assertion is still green with an
    **unchanged body**, which is what D5's skip-when-empty exists to achieve.
