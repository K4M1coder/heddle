# Tasks: an ACP facade over the native loop (v0 slice)

**Spec:** `specs/008-acp-facade/spec.md` · TDD (red→green), product code in `crates/skein-acp`,
branch `008-acp-facade` cut from `dev`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ (this slice is the first real client boundary; still library-only, no bin,
  no UI) · II Local-first ✅ (byte-stream transport, in-process duplex in tests; no network
  egress, no listening socket, no process spawned)
- III Test-First ✅ (T1's smoke test pins the SDK's structural assumptions before any product
  code; T3 red before T4/T5 green) · IV Inverted coupling ✅ (no `skein-core` file changes at
  all; ACP is adapted through decorators over `ToolTransport` and `ModelClient`)
- V Traceability ✅ (updates are computed from `Ledger::log(run_id)` — no second record; run ids
  `{session_id}#{n}` keep one `Exit` per chain and `verify_chain` per-run semantics intact)
- VI Security ✅ (`ToolPolicy::decide` is gate 1 and runs first, structurally: the permission
  decorator sits behind `ToolGateway`'s transport call, so an unlisted tool never becomes a
  permission request; the request carries the tool name only, never the arguments)
- VII Neutrality ✅ (one crate, one protocol version, one transport shape, five ACP methods;
  everything else named as deferred below)
- VIII Loop discipline ✅ **(a)** `LoopController` unchanged and still the sole authority on
  termination — ACP cannot extend a budget. **(b)** `ProgressProbe` untouched; the client cannot
  feed progress. **(c)** per-step capture unchanged. **(d)** advanced, not closed:
  `session/request_permission` is the first real human-escalation surface, while
  `Exit::HumanReject` stays unreached — a declined permission is a tool denial, not a run
  rejection.
- Cross-platform ✅ (pure Rust, no `#[cfg]` in our code; the SDK carries `rustix`/`windows-sys`
  itself. `core.yml`'s `paths:` already covers `crates/**` at 1.97 and the SDK's MSRV is 1.88 —
  confirmed, not edited).

## Tasks
- [ ] **T0** `specs/008-acp-facade/{spec.md,plan.md,tasks.md}`; branch `008-acp-facade` cut from
      `dev`
- [ ] **T1** pin the SDK's API surface against the vendored 2.0.0 source *before* writing product
      code, and prove the two structural assumptions with a throwaway smoke test
- [ ] **T2** control baseline: `cargo test --workspace` on `dev` before any edit
- [ ] **T3** RED — the whole of `crates/skein-acp/tests/acp_session.rs` against the
      not-yet-existing `skein_acp` API; compiler errors recorded below
- [ ] **T4** GREEN part 1 — `AcpPermissionTransport` and `CancellableModel`
- [ ] **T5** GREEN part 2 — `SkeinAgent`/`SkeinSession`, the `Agent.builder()` wiring, and
      `project_updates`
- [ ] **T6** gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace`
- [ ] **T7** control diff: `git diff dev` empty on `crates/skein-core/`, `crates/skein-mcp/`,
      `spikes/`, `.github/` and `rust-toolchain.toml`; the root `Cargo.toml` diff confined to
      `[workspace.dependencies]`
- [ ] **T8** drift check: dependency count added to `Cargo.lock`, MSRV confirmation, and the
      `serde_json/preserve_order` feature-unification investigation
- [ ] **T9** close out: tick the ACP bullet in `specs/007-tool-allowlist/tasks.md`, set this
      spec's Status, and populate the "Next slice" list

## Control baseline (T2)

`cargo test --workspace` on `dev` / `0b0102e`, working tree clean, 2026-09-03: **40 passing** —
`skein-core/tests/core.rs` 6, `tests/native_loop.rs` 18, `tests/tool_gateway.rs` 9,
`skein-mcp/tests/rmcp_gateway.rs` 7; 0 failed, 0 ignored. This is the number T6 diffs against.

## Pinned SDK surface (T1)

Read from the vendored source of `agent-client-protocol 2.0.0` and
`agent-client-protocol-schema 1.5.0` in the local cargo registry, not from a docs summary. Every
name below is used by the product code exactly as spelled here.

| Item | Pinned spelling |
|---|---|
| `ContentChunk` | `ContentChunk::new(content: ContentBlock)` — no `message_id` in the constructor |
| `PromptResponse` | `PromptResponse::new(stop_reason: StopReason)` |
| `NewSessionResponse` | `NewSessionResponse::new(session_id: impl Into<SessionId>)` |
| `ToolCallUpdate` | `ToolCallUpdate::new(tool_call_id: impl Into<ToolCallId>, fields: ToolCallUpdateFields)`; status lives on the fields: `ToolCallUpdateFields::new().status(impl IntoOption<ToolCallStatus>)` |
| `ToolCall` (session update) | `ToolCall::new(tool_call_id: impl Into<ToolCallId>, title: impl Into<String>)`, then `.kind(ToolKind)` |
| `ToolCallStatus` | `Pending`, `InProgress`, `Completed`, `Failed` (`#[non_exhaustive]`) |
| `PermissionOptionKind` | `AllowOnce`, `AllowAlways`, `RejectOnce`, `RejectAlways` (`#[non_exhaustive]`) |
| `PermissionOptionId` | `PermissionOptionId::new(impl Into<Arc<str>>)`; also `From<&'static str>`, `From<String>`, `From<Arc<str>>` |
| `PermissionOption` | `PermissionOption::new(option_id, name: impl Into<String>, kind)` |
| `AgentCapabilities` | `AgentCapabilities::new()` |
| `RequestPermissionOutcome` | `Selected(SelectedPermissionOutcome)` \| `Cancelled` (`#[non_exhaustive]`) |
| `StopReason` | `EndTurn`, `MaxTokens`, `MaxTurnRequests`, `Refusal`, `Cancelled` (`#[non_exhaustive]`) |
| `SessionUpdate` | `AgentMessageChunk(ContentChunk)`, `ToolCall(ToolCall)`, `ToolCallUpdate(ToolCallUpdate)`, … (`#[non_exhaustive]`) |
| `CancelNotification` | `CancelNotification::new(session_id)`, field `session_id` |
| `SentRequest::on_receiving_result` | `fn(self, task: impl FnOnce(Result<T, Error>) -> F + Send + 'static) -> Result<(), Error>` where `F: Future<Output = Result<(), Error>> + Send + 'static` |
| Handler return type | `Result<T, Error>` where `T: IntoHandled<…>`; `impl<T> IntoHandled<T> for ()`, so a handler that answers later returns `Ok(())` |

**The two structural assumptions were proved, not assumed.** `crates/skein-acp/tests/smoke.rs`
(throwaway, T1 only) ran green on the first attempt, 2026-09-03:

1. a `Responder<PromptResponse>` moved into a `std::thread` responds after the handler has
   already returned, and the client's `block_task()` receives it;
2. `SentRequest::on_receiving_result` plus `std::sync::mpsc::Receiver::recv` round-trips a
   `session/request_permission` from that same non-dispatch thread without deadlocking.

**The plan's `connection.spawn` fallback is therefore not adopted.** The reason the direct form
is safe is the ordering documented on `on_receiving_result`: it registers a callback and returns
immediately, so the dispatch loop stays free to deliver the response the OS thread is blocked on.
Our callback does one `mpsc::Sender::send` and awaits nothing, which is the bounded work that
method's ordering barrier requires.

## Observed red (Constitution III)

- **T3** `cargo build --workspace --all-targets`, 2026-09-03:
  - `error[E0432]: unresolved imports skein_acp::project_updates, skein_acp::CancellableModel,
    skein_acp::SessionParts, skein_acp::SkeinAgent`
    (`crates/skein-acp/tests/acp_session.rs:12:17` — *"no `SkeinAgent` in the root"*,
    *"no `SessionParts` in the root"*, *"no `CancellableModel` in the root"*,
    *"no `project_updates` in the root"*)
  - `error[E0433]: cannot find AcpPermissionTransport in skein_acp`
    (`crates/skein-acp/tests/acp_session.rs:649:52`)
  - `error: could not compile skein-acp (test "acp_session") due to 2 previous errors`
  - As in slice 007, rustc abandons the crate once import resolution fails, so these two
    diagnostics are the whole red: every name the suite needs is unresolved, and no
    type-level error is reached.

## Next slice (not this feature)
