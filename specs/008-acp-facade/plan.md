# Implementation Plan: an ACP facade over the native loop (v0 slice)

**Branch**: `008-acp-facade` | **Date**: 2026-09-03 | **Spec**: `specs/008-acp-facade/spec.md`

## Summary
A new workspace crate `skein-acp` exposes the existing governed loop over the Agent Client
Protocol. It adds no capability to `skein-core` and changes no file in it: ACP reaches the core
through two decorators over ports the core already defines.

- `AcpPermissionTransport<T: ToolTransport>` wraps the operator's real transport, so it is
  constructed *inside* `ToolGateway`. `call_captured` therefore consults `ToolPolicy` first,
  always — an unlisted tool never becomes an ACP permission request. The client's answer can only
  further restrict. A decline returns `SkeinError::ToolDenied`, which `NativeLoop::mediate`
  already turns into `[tool_result … status=denied]` while the run continues.
- `CancellableModel<C: ModelClient>` wraps the injected model client and returns
  `SkeinError::Model` once the session's cancel flag is set. `NativeLoop::run` propagates that
  out immediately, ending the run at a turn boundary with the chain still verifiable — the path
  the pre-existing `provider_error_leaves_the_chain_verifiable` test already covers.

The sync/async boundary is a plain `std::thread`. `skein-core` is deliberately synchronous; ACP
is async, and the SDK documents that `SentRequest::block_task` *"will deadlock if called in
handlers"* because handler callbacks run on the connection's single dispatch task. So the
`session/prompt` handler moves the `Responder` into a thread and returns `Ok(())` immediately;
the thread runs `NativeLoop::run` to completion and only then responds. Inside that thread,
`AcpPermissionTransport::call` uses `SentRequest::on_receiving_result` (the SDK's in-handler-safe
form, which returns immediately) plus `std::sync::mpsc::Receiver::recv`. No executor is involved
and no tokio runtime exists in the library.

ACP session updates are a **projection of the Ledger**, computed by
`project_updates(&Ledger, run_id) -> Vec<SessionUpdate>` after the run returns. Because the
updates are computed *from* the chain, "no second record" is structural rather than a promise.
The cost is that updates are not streamed during the turn; that is stated in the spec's
Assumptions and deferred to the slice that gives `Ledger` an append-observer.

## Technical Context
**Language/Version**: Rust 1.97 (pinned in `rust-toolchain.toml`, unchanged this slice)
**Primary Dependencies**: `agent-client-protocol = "2.0.0"` (schema `v1`), plus the existing
`serde_json`. Dev-only: `tokio` (`rt-multi-thread`, `io-util`, `macros`, `time`) and `tokio-util`
(`compat`), used solely to build the in-process duplex byte stream the tests connect over —
exactly as `skein-mcp`'s live rmcp fixture does. The SDK itself is runtime-agnostic: its own
`tokio` dependency is a dev-dependency.
**Storage**: the existing in-memory `Ledger`
**Testing**: `cargo test`; a real ACP client and a real ACP agent over `tokio::io::duplex`
adapted with `tokio_util::compat` into `ByteStreams`. Model, probe and tool-transport doubles are
new in this crate, modelled on the ones in `skein-core`'s and `skein-mcp`'s test binaries (copied,
not shared — those are private to their test binaries and must not be moved).
**Target Platform**: Windows + macOS + Linux
**Project Type**: library (three workspace members)
**Performance Goals**: N/A
**Constraints**: no file in `crates/skein-core/` or `crates/skein-mcp/` changes; no network
egress and no listening socket; deny-by-default tool policy runs before the client is consulted
**Scale/Scope**: one new crate, five ACP methods (`initialize`, `session/new`, `session/prompt`,
`session/request_permission`, `session/cancel`), one protocol version

## Constitution Check
*GATE: must pass before implementation.*
- **I. Headless core**: ✅ this slice *is* the work that makes Principle I reachable — the first
  real client boundary. Library plus a real ACP surface; still no `[[bin]]`, no UI.
- **II. Local-first / silo isolation**: ✅ the transport is a byte stream; tests use an
  in-process duplex. No network egress, no listening socket. `async-process` arrives as a
  transitive dependency (for the SDK's subprocess transport) but this slice spawns no process.
- **III. Test-First**: ✅ T1's smoke test pins the SDK's two structural assumptions before any
  product code exists; T3's red is observed and recorded before T4/T5.
- **IV. Inverted coupling**: ✅ **no `skein-core` trait signature changes and no `skein-core`
  file changes at all.** ACP is adapted through decorators over `ToolTransport` and
  `ModelClient`. `skein-core` never names ACP. The `Send + 'static` bounds the facade needs are
  bounds on *its own* generics, not on the core's traits.
- **V. Traceability**: ✅ ACP updates are computed from `Ledger::log(run_id)`; there is no second
  record, and T7's test asserts the correspondence. Run ids are `{session_id}#{n}`, keeping one
  `Exit` per chain and leaving `verify_chain` semantics untouched.
- **VI. Security / deny-by-default**: ✅ `ToolPolicy::decide` is gate 1 and runs first,
  structurally: the permission decorator sits *behind* `ToolGateway`'s transport call, so an
  unlisted tool never reaches the client as a permission request. The permission request carries
  the tool name only, so no argument egress to an out-of-process client whose transcript the
  `Redactor` does not govern.
- **VII. Neutrality / YAGNI**: ✅ one new crate, one protocol version (`schema::v1`), one
  transport shape (byte streams), five ACP methods. Multi-modal content, plan updates, thought
  chunks, session persistence/resume, `allow_always`, MCP passthrough and the `_meta` extension
  channel are all deferred and named in the spec and in `tasks.md`.
- **VIII. Loop discipline (NON-NEGOTIABLE)**: ✅ **(a)** `LoopController` is unchanged and still
  the sole authority on termination; ACP cannot extend a budget. **(b)** `ProgressProbe` is
  untouched; the ACP client cannot feed progress. **(c)** per-step capture is unchanged.
  **(d)** this slice *advances* VIII(d): `session/request_permission` is the first real
  human-escalation surface the product has had. `Exit::HumanReject` stays unreached — a declined
  permission is a tool denial, not a run rejection — and that is named as deferred.
- **Cross-platform**: ✅ pure Rust, no `#[cfg]` in our code; the SDK carries `rustix` (unix) and
  `windows-sys` (windows) itself. `core.yml`'s `paths:` already covers `crates/**` at toolchain
  1.97, so no CI edit is needed. The SDK's MSRV is 1.88, below the repo's pin.

## Project Structure

### Documentation (this feature)
```text
specs/008-acp-facade/
├── spec.md      # this feature's requirements
├── plan.md      # this file
└── tasks.md     # executable breakdown
```

### Source Code (repository root)
```text
Cargo.toml                     # +agent-client-protocol, +tokio-util in [workspace.dependencies]
crates/skein-acp/
  Cargo.toml                   # new member (picked up by `members = ["crates/*"]`)
  src/lib.rs                   # SkeinAgent, SkeinSession, SessionParts, serve, project_updates
  src/permission.rs            # AcpPermissionTransport — gate 2
  src/cancel.rs                # CancellableModel
  tests/acp_session.rs         # the real-client/real-agent suite
```
**Structure Decision**: nothing outside `crates/skein-acp/`, `specs/008-acp-facade/` and two
lines of the root `Cargo.toml` changes. `crates/skein-core/` and `crates/skein-mcp/` are
byte-identical to `dev`, so specs 003–007 all remain independent controls.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| **A third workspace crate, and the largest dependency graph the repo has taken on** (Principle VII) | ACP is the boundary ADR-0003 decided on, and the SDK is the only real implementation of it. Putting `async-io`, `async-process`, `blocking`, `uuid`, `rustc-hash`, `shell-words` and a pinned schema crate behind `skein-core`'s four-dependency list would permanently widen the trust and build surface of the one crate that must stay boring. `skein-mcp` set this precedent for rmcp; the isolation argument is stronger here. | Adding the dependency to `skein-core`: violates Principle IV outright — the core would name a protocol. Hand-rolling a JSON-RPC subset: a stand-in for the protocol, which SC-002 exists to forbid, and it would buy none of the interop ADR-0003 wanted. |
| **`std::thread` + `std::sync::mpsc` as the sync/async boundary, rather than the ecosystem-standard `spawn_blocking`** | The SDK is runtime-agnostic; adding tokio to the *library* would impose a runtime choice on every future embedder for no gain. `std::thread` has no executor semantics to reason about and its deadlock analysis fits in one sentence: the prompt handler returns immediately, so the dispatch loop stays free to deliver the permission response the loop thread is blocked on. | `tokio::task::spawn_blocking` + a runtime in the library: imposes tokio on embedders (FR-010). `blocking::unblock` + `futures::executor::block_on`: two more direct dependencies to reach the same place. Awaiting the loop inside the prompt handler: deadlocks by construction — the handler occupies the dispatch task the permission response must arrive on. Making `skein-core`'s traits async: rewrites four merged slices and violates FR-002. |
| **Session updates are emitted after the turn, not streamed during it** (a visible functional gap) | Streaming needs an append-observer seam on `Ledger`, and `ledger.rs` may not change this slice; the only alternative is a parallel event channel out of the loop, which is precisely the second record Principle V prohibits. Emitting a projection of the finished chain keeps "a view, not a record" structural. | A `Vec<SessionUpdate>` accumulated inside the loop and drained afterwards: identical latency, but a second in-memory record that could drift from the chain. An observer callback on `NativeLoop`: a `skein-core` signature change (FR-002). |
