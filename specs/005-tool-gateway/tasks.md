# Tasks: governed Tool Gateway + rmcp transport (v0 strict-local)

**Spec:** `specs/005-tool-gateway/spec.md` · TDD (red→green), product code in
`crates/skein-core` and the new `crates/skein-mcp`, branch `005-tool-gateway` cut from `dev`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ (library API; no UI, no bin) · II Local-first ✅ (no network; the rmcp
  tests speak to an in-process duplex, and `skein-core` gains no dependency)
- III Test-First ✅ (T3 red before T4 green; T6 red before T7 green) · IV Inverted coupling ✅
  (`ToolTransport` is the seam; `skein-core`'s `Cargo.toml` cannot name `rmcp`)
- V Traceability ✅ (capture through the existing `Ledger::append` with the existing
  `StepKind`s; `verify_chain` asserted on the denial, transport-error and interleaved paths)
- VI Security ✅ (deny-by-default for mutating tools with the transport untouched on denial;
  redaction of arguments *and* results before every append, asserted by scanning every payload
  of the run) · VII Neutrality ✅ (`rmcp` reused rather than reimplementing MCP; no new
  `StepKind`, no `SecretProvider`, no tool discovery, no async port)
- VIII Loop discipline n/a this slice — the gateway is not wired into `NativeLoop`.
  **(d)** HITL escalation still deferred: approval is a configured list of tool names.
- Cross-platform ✅ (pure Rust, no `#[cfg]`; `core.yml`'s `paths:` already covers `crates/**`,
  so the new crate rides the existing tri-OS matrix — `rust-toolchain.toml` added by T1).

## Done
- [x] **T0** `specs/005-tool-gateway/{spec.md,plan.md,tasks.md}` + branch from `dev`
- [x] **T1** toolchain 1.79 → 1.97 in `rust-toolchain.toml`, `Cargo.toml`,
      `.github/workflows/core.yml` and `docs/DEVELOPMENT.md`, on its own commit before any new
      code; all three gates green on unchanged source (FR-008)
- [x] **T2** `SkeinError::ToolDenied` and `SkeinError::Tool` (FR-002/FR-006)
- [x] **T3** RED — `crates/skein-core/tests/tool_gateway.rs` with `CountingTransport` and all 6
      tests; compile failure observed and recorded
- [x] **T4** `tool` — `ToolCall`/`ToolOutcome`/`ToolTransport`/`ToolPolicy`/`Decision`/
      `Redactor`/`CapturedResult`/`ToolGateway::call`/`replay_tool_calls`
      (FR-001/002/003/004/005)
- [x] **T5** `crates/skein-mcp` skeleton + root `[workspace.dependencies]` entries (FR-007)
- [x] **T6** RED — `crates/skein-mcp/tests/rmcp_gateway.rs` with the live embedded rmcp server
      fixture and all 5 tests; compile failure observed and recorded
- [x] **T7** `RmcpToolTransport` — owns its `tokio` runtime, blocks behind the sync port
      (SC-002)
- [x] **T8** `fmt --check`, `clippy --workspace --all-targets -D warnings`,
      `cargo test --workspace` — **26/26** (spec 003's 6 + spec 004's 9 + this slice's 6
      gateway + 5 rmcp), 2026-09-03; `git diff` on `crates/skein-core/Cargo.toml` empty
      (SC-001/SC-003)
- [x] **T9** CI: `core.yml` `paths:` covers `crates/**` and `rust-toolchain.toml`
- [x] **T10** `spikes/` untouched — clean `git diff`, and no `crates/` reference to
      `mcp_gateway` or a `spikes/` path (SC-004, ADR-0004 D2)
- [x] **T11** `docs/superpowers/spikes/mcp-gateway-evidence.md` — the Spike 4 evidence note
      ADR-0004 D2 requires and that was never written
- [x] **T12** tick the gateway half of the backlog bullet in specs 003 and 004; set this
      spec's Status

## Observed red (Constitution III)

- **T3** `cargo test -p skein-core`:
  `error[E0432]: unresolved imports skein_core::replay_tool_calls, skein_core::Redactor,`
  `skein_core::ToolCall, skein_core::ToolGateway, skein_core::ToolOutcome,`
  `skein_core::ToolPolicy, skein_core::ToolTransport` —
  `error: could not compile skein-core (test "tool_gateway")`.
- **T6** `cargo test -p skein-mcp`:
  `error[E0432]: unresolved import skein_mcp::RmcpToolTransport` —
  `no RmcpToolTransport in the root` —
  `error: could not compile skein-mcp (test "rmcp_gateway")`.

## Next slice (not this feature)
- [ ] ACP client facade over the native loop + gateway
- [ ] wire the gateway into `NativeLoop` (`TurnResponse.tool_calls`, mid-loop tool mediation,
      prompt-injection handling of tool output)
- [ ] silo-backed durable Ledger (SQLite) + `SecretProvider` (OS keychain)
- [ ] `skein-cli` reference client
