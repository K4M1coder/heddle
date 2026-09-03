# Tasks: `skein acp-agent` — the ACP facade as a running process (v0 slice)

**Spec:** `specs/013-acp-agent/spec.md` · TDD (red→green), product code in `crates/skein-cli` and
`crates/skein-acp` plus one additive error variant in `crates/skein-core`, branch `013-acp-agent`
cut from `dev` after slice 012 merged.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the ACP facade is a library and `skein acp-agent` is a wiring-only subcommand
  that holds no capability of its own. The one thing the CLI needed that the API lacked — a name for
  "the protocol transport failed" — is closed by *adding to `SkeinError`*, not by reaching around it,
  the same move slice 012 made · II Local-first ✅ NON-NEGOTIABLE, and **reused rather than
  reimplemented**: the same `LocalEndpoint::parse` guard and the same no-TLS-by-construction `ureq`
  build, now with exactly one copy of the resolution logic shared by both commands (`wiring.rs`)
- III Test-First ✅ T2's red observed before its green, T3's before its green, and T6's subprocess
  test is T4's red — recorded as such in `## Observed red` rather than substituted with a weaker
  in-process one · IV Inverted coupling ✅ ACP names stay inside `skein-acp` (`serve_stdio` is why),
  the executor choice stays there too (mirroring `skein-mcp`'s `RmcpToolTransport`), `skein-core`
  gains no dependency, and the one public-API change is stated and justified (plan D3)
- V Traceability ✅ every ACP session's runs land on a real silo chain and verify **from a second
  process**. The known gap is carried forward unchanged: the chain holds the *translated*
  `TurnRequest`/`TurnResponse`, and model I/O is **not** redacted
- VI Security ✅ no credential path in this slice; the deferral keeps its pre-written constraint (a
  provider token MUST arrive as a `SecretRef` through `SecretProvider`)
- VII Neutrality ✅ one subcommand, one method, one error variant, zero new packages. **No new ACP
  method, no streaming, no tools, no auth, no config file, no `--json`.**
- VIII Loop discipline ✅ NON-NEGOTIABLE and unchanged: real provider metering or a failed turn,
  `finish_reason: "length"` is not final, and the `ProgressProbe` returns `false` because a tool-less
  session has no external ground truth
- Cross-platform ✅ no `#[cfg]` in any new file; the stub binds `127.0.0.1:0`; subprocess spawning
  and process-group teardown are the dependency's concern (`AcpAgent::spawn_process` uses
  `process_group(0)` on Unix and `CREATE_NO_WINDOW` on Windows). `core.yml`'s `paths:` already covers
  `crates/**` and `Cargo.toml`, and this slice adds no workspace member — confirmed by reading, not
  edited.

## Tasks
- [x] **T0** `specs/013-acp-agent/{spec.md,plan.md,tasks.md}`; branch `013-acp-agent` cut from `dev`
      with slice 012 merged
- [x] **T1** control baseline: `cargo test --workspace` before any edit — **105 passed, 1 ignored**
- [x] **T2** RED→GREEN — `SkeinError::Protocol` with its own test. Ordered first because
      `serve_stdio` cannot compile without it
- [x] **T3** RED→GREEN — the fallible factory (plan D3), with
      `a_factory_that_fails_makes_session_new_fail_and_leaves_the_connection_usable` as its
      justifying test
- [x] **T4** GREEN — `SkeinAgent::serve_stdio` (plan D1/D2), plus `futures` in
      `[workspace.dependencies]` and in `skein-acp`'s `[dependencies]`. Its red is T6
- [x] **T5** refactor — `crates/skein-cli/src/wiring.rs` (plan D4), with the five `cli_chat.rs`
      tests unchanged as the control
- [x] **T6** RED — `crates/skein-cli/tests/cli_acp_agent.rs`, a real ACP client spawning the real
      binary, against the not-yet-existing `acp-agent` subcommand
- [x] **T7** GREEN — `crates/skein-cli/src/acp.rs`, the `AcpAgent` subcommand, and the `main.rs`
      docstring correction
- [x] **T8** gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace`, `cargo build --workspace`, + `skein acp-agent --help`
- [x] **T9** control diff, dependency drift, close-out

## Control baseline (T1)

`cargo test --workspace` on `013-acp-agent` @ `2e41492` (identical to `dev`), working tree clean,
2026-09-03, before any edit: **105 passed, 0 failed, 1 ignored** — `acp_session` 13, `cli_chat` 6,
`cli_ledger` 8, `cli_secret` 2, `core` 14, `native_loop` 18, `tool_gateway` 9, `governed_run` 2,
`openai_compat` 14 (+1 ignored, the optional live-Ollama test), `rmcp_gateway` 7, `silo_ledger` 7,
`silo_secret` 5. The five `src/lib.rs`/`src/main.rs` unit-test targets and the five doc-test targets
each contribute `0 passed`. This is the number T8 diffs against, and it matches slice 012's recorded
gate figure exactly.

## Observed red (Constitution III)

_Recorded as each step lands._

## Gate run (T8)

_Recorded at T8._

## Control diff (T9)

_Recorded at T9._

## Drift (T9)

_Recorded at T9._

## Out of scope

Deliberately not done, so no one helpfully does it:

- **Tool advertisement, tool discovery, and any MCP wiring in the CLI.** `TurnRequest` has no
  `tools` field and `skein-cli` does not depend on `skein-mcp`. Consequence, stated in the spec:
  `AcpPermissionTransport`'s permission flow is **not reachable** through `skein acp-agent` in this
  slice; a model-invented tool call is refused by the empty `ToolPolicy`, and that refusal is what
  the ACP client sees.
- **Streaming (SSE) and incremental `AgentMessageChunk`s.** `project_updates` emits one chunk per
  `LlmResponse` step *after* the run; `"stream": false` is still sent explicitly. Real streaming
  changes the Ledger capture shape and belongs with the gateway.
- **New ACP methods or capabilities** — `session/load`, `session/set_mode`, `fs/*`, MCP-over-ACP,
  extra `AgentCapabilities` fields, terminals, plan updates. Slice 008's surface, unchanged.
- **Redaction on the `LlmRequest`/`LlmResponse` path**, raw-wire-byte capture, provider
  authentication, sampling parameters, a config file. Separate, already-named items on slice 012's
  `## Next slice` list.
- **A silo-per-session or per-client silo mapping**, session persistence across process restarts, or
  ACP `session/load`. One process, one silo; sessions live only as long as the connection.
- **Changing `skein chat`'s behaviour.** T5 moves code; the five `cli_chat.rs` tests are the proof
  nothing else moved.
- **A tokio runtime in `skein-cli` or `skein-acp` product code.** Slice 012's next-slice note
  predicted one; it is not needed, and the spec says why.
- **`spikes/`** — untouched (ADR-0004 D2).

## Next slice (not this feature)
- [ ] **redaction on the `LlmRequest`/`LlmResponse` path**, so `skein ledger show` cannot print a
      conversation secret. Carried from slices 011 and 012, and now reachable from two commands
      rather than one: `ToolCall`/`ToolResult` payloads pass through the `Redactor` in
      `ToolGateway::call_captured`, but `NativeLoop::run` appends model I/O **raw**. The fix belongs
      to the governed loop.
- [ ] **tool advertisement** — a `tools` field on `TurnRequest`, which needs tool *discovery* from
      the Tool Gateway first. It is what would make `AcpPermissionTransport` reachable from a real
      editor, which is the largest untested-in-production path this slice leaves behind.
- [ ] **raw-wire-byte capture** — a `StepKind` for the provider's literal request and response
      bytes, which is what design §4.5's "exact model I/O" and Spike 1's criterion C1 actually ask
      for.
- [ ] **streaming (SSE)**, together with incremental ACP `AgentMessageChunk` notifications. Today a
      client sees one chunk per turn, after the turn.
- [ ] **provider authentication**, when a local gateway needs one: an `Authorization: Bearer` whose
      value arrives as a `SecretRef` resolved through `SecretProvider` (Principle VI).
- [ ] **sampling parameters** — temperature, top-p, seed. `TurnRequest` cannot express them.
- [ ] a config file holding the base URL, the model and a default silo root, so `--base-url`/
      `--model`/`--root` are not the only way to name them.
- [ ] the egress-policy layer and ADR-0002 D4's **process-level socket-deny boundary**, which is
      what would close `LocalEndpoint`'s `localhost` re-resolution residual.
