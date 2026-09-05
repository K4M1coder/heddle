# Feature Specification: prove the ACP permission gate end to end with an answering client (v0 slice)

**Feature Branch:** `018-acp-permission-gate` · **Created:** 2026-09-03 · **Status:** Implemented (v0
slice) **Input:** `specs/016-fs-connector/tasks.md` and `specs/017-git-connector/tasks.md`, both
`## Next slice` — *"the ACP permission gate, exercised … `cli_acp_agent.rs`'s client registers no
permission handler, and building one is a slice of its own. Until then the gate is wired and
unproven end to end."* · Constitution III (**test-first**), V (**traceability**), VI
(**deny-by-default**, the principle at stake), VII (**no capability without a real need**) · design
§4.3.

Slice 013 wired `AcpPermissionTransport` so a mutating tool call made through `heddle acp-agent` asks
the connected ACP client before executing. Slices 016 (`fs_write`) and 017 (git, read-only only)
left the gate **reachable** but incompletely proven, and both named the same residual without
closing it. This slice closes it.

**This slice changes no product code.** It is tests plus these Spec-Kit documents. Everything it
needs — the transport, the policy, the connector, the silo-backed chain, and the client-side
answering API — already exists on `dev`; the gap was a test, and it sits in one file.

## What this slice changes for a user

**Nothing observable.** No new flag, no new tool, no changed output, no changed dependency. What
changes is what the project can *claim*: that Constitution VI's deny-by-default gate has been driven
by a real editor-shaped client against a tool with a real effect on disk, in both directions, over
the real protocol, against the real binary — rather than being wired and taken on trust.

## Five things a reader must know up front

1. **A real ACP client answering a real permission request was already tested — at unit level.**
   `crates/heddle-acp/tests/acp_session.rs`'s `ask_permission` helper builds a real `Client` with
   `.on_receive_request(async move |request: RequestPermissionRequest, responder, _cx| …)`, connects
   it to a real `Agent` over a real `ByteStreams`, and drives `AcpPermissionTransport::call`
   directly. `p1`/`p2`/`p3` cover Allow, Reject and Cancelled. **The residual was never "no client
   ever answers"** — the two prior slices' wording is imprecise and this spec corrects it rather
   than inheriting it. See point 2 for what was genuinely missing.
2. **What was genuinely unproven is the composed layer, and it is four things at once.** (a) *No
   real mutating tool*: `p1`/`p2` use a `CountingTransport` double, so a Deny proves "a counter did
   not move", never "nothing happened on disk". (b) *No Ledger*: `ask_permission` bypasses
   `ToolGateway::call_captured` entirely, so no `ToolCall`, `Approval` or `ToolResult` step exists on
   any of those paths and `verify_chain` is never called — Constitution V is unproven here. (c) *No
   real process*: `p1`–`p3` run in-process over a `tokio::io::duplex`. (d) **No test anywhere makes a
   tool call through `heddle acp-agent` at all** — every one of the eight tests in
   `crates/heddle-cli/tests/cli_acp_agent.rs` either drives a text-only turn, or asserts the
   *advertised* tool list off the wire and stops there, or refuses before the handshake.
3. **(d) is also why the residual could sit open through two slices without anyone hitting a
   failure.** Because no ACP test ever provoked a tool call, that file's client having no permission
   handler never mattered. The handler is the slice's actual new machinery, and adding a tool call
   without it is **measured** (T3's red): the client does *not* auto-reply "method not found", the
   request is simply never answered, and `AcpPermissionTransport::ask`'s untimed
   `std::sync::mpsc::Receiver::recv()` blocks the child's loop thread forever. Not an internal error
   — a hang. The file's pre-existing `run_with_timeout` is what makes that a 60-second failure
   instead of a stuck CI job on three operating systems.
4. **The disk is the proof, not the tool result.** The Deny assertion is
   `!fs_root.join("planted.txt").exists()`, on the **same fixture** where the Allow test proves the
   very same call *does* create that file. This is `governed_fs_run.rs`'s own recorded reasoning —
   *"its **absence on disk** is the ground truth that nothing downstream of the policy ran. Not a
   counter in the server: an effect the server would have had"* — applied at a new refusing layer.
   The `status=denied` text is asserted too, as corroboration, never as the proof.
5. **The refusal is recorded on the chain in the shape the existing deny path already uses, and no
   new `StepKind` is invented.** `ToolGateway::call_captured` appends `ToolCall` → `Approval` →
   *then* calls the transport → `ToolResult` only on success. `governed_fs_run.rs`'s
   `an_unlisted_write_never_reaches_the_server` pins `[ToolCall, Approval]` for a *policy* denial;
   an ACP denial lands in exactly the same shape at a different refusing layer. The client's answer
   is on the chain twice over anyway: as the **absence** of `ToolResult`, and verbatim inside the
   next `LlmRequest` payload, because `NativeLoop::mediate` feeds
   `[tool_result tool=fs_write status=denied]\nacp client declined permission (heddle.reject-once)`
   back into the conversation the following request records.

## Functional requirements

- **FR-001** No file under `crates/*/src/` changes. No manifest changes, no new dependency, no new
  flag. If green requires a `src/` edit, the slice's premise is wrong and that is a **stop
  condition**, not a thing to patch.
- **FR-002** Two acceptance tests are appended to `crates/heddle-cli/tests/cli_acp_agent.rs` under a
  `// ---- the ACP permission gate (spec 018) ----` section. That file is the only one that changes
  in `crates/`, and its diff against `dev` is **append-only apart from the `use` block**.
- **FR-003** The tests share one parameterised harness — `run_answering(…, answer:
  PermissionOptionKind)` — differing **only** in which offered option the client selects. Each test
  gets its own `TempDir` fs root, its own silo and its own child process, so each assertion's ground
  truth is its own.
- **FR-004** The client's permission handler **records** every `RequestPermissionRequest` it receives
  (the type derives `Clone`) and answers by finding the offered option whose `kind` matches the
  requested answer — `request.options.iter().find(|o| o.kind == answer)`, never a hand-built literal.
  That is what a real editor does, and it is `acp_session.rs`'s existing pattern.
- **FR-005** The handler only records and responds. It never calls `block_task()`, so it cannot
  deadlock the dispatch loop that must stay free to deliver the answer the agent's loop thread is
  blocked on. The file's existing `run_with_timeout` bounds a regression at 60s rather than hanging
  CI on three operating systems.
- **FR-006** `tool_call_reply` and `last_message` are **copied verbatim** from `cli_chat.rs`, for the
  reason `cli_acp_agent.rs`'s own header already records: `heddle-cli` has no `lib` target,
  integration-test binaries share nothing, and copying keeps the other file's tests as this slice's
  controls.
- **FR-007** No pre-existing assertion anywhere in the workspace is changed or removed.

## Success criteria

- **SC-001** An ACP client answering `AllowOnce` over the real protocol to the real `heddle acp-agent`
  binary lets `fs_write` execute; the file exists on disk with the model's exact content.
- **SC-002** An ACP client answering `RejectOnce` under the identical fixture leaves **no file on
  disk**.
- **SC-003** Both runs' chains verify through `heddle ledger verify` in a second process, at 12 and 11
  steps respectively; the deny chain differs from the allow chain by the absence of `tool_result`
  and nothing else.
- **SC-004** The permission request observed **by the client** carries the session id, the tool name
  as its `tool_call_id` and title, and exactly the two documented option ids and kinds
  (`heddle.allow-once`/`AllowOnce`, `heddle.reject-once`/`RejectOnce`) in that order. These two string
  constants are what `AcpPermissionTransport::call` matches on and they are asserted nowhere on
  `dev` — a typo in either would silently turn every Allow into a denial.
- **SC-005** The model is told `status=ok` with the byte count on Allow and `status=denied` with the
  selected option id on Deny; the run reaches `StopReason::EndTurn` in both. A governed refusal is
  history the run survives, not an error.
- **SC-006** No file under `crates/*/src/` changes. `cargo test --workspace` is the T1 baseline
  **+2**, with `cli_acp_agent` 8 → 10 and every other target's count unchanged.

## Assumptions and residuals

- **`fs_write` is still the only `Mutating` tool**, and `ToolArgs::agent_policy` is still the only
  place it is `approved`. Re-verified this slice; if a second mutating tool ever lands, this slice's
  two tests do not automatically cover it.
- **`heddle-1` is the session id** because each test spawns its own `heddle` child process and the
  facade mints session ids from an `AtomicU64` starting at 1 per process — the reasoning the
  file's existing headline test already records and relies on.
- **A permission request cannot be correlated to its tool call by a client.**
  `AcpPermissionTransport::ask` uses `ToolCallId::new(tool)` — the tool *name* — while
  `heddle_acp::project_updates` uses `step.id`, the chain hash, as the `ToolCallId` for the
  `SessionUpdate::ToolCall`. The two ids never match, so an editor cannot join the prompt it showed
  to the tool call it later sees. Discovered while verifying; fixing it needs the chain step id
  inside the transport, which the transport does not have. That is a design change, not this slice's.
- **An ACP-denied call is projected as `Pending` forever.** `project_updates` maps
  `Approval.decision == "allowed"` → `ToolCallStatus::Pending` and only a `ToolResult` step →
  `Completed`. On the ACP-deny path the `Approval` says `allowed` (the *policy* allowed it) and no
  `ToolResult` is written, so the client's last word on that tool call is `Pending`. Observable and
  mildly wrong. The deny test therefore asserts only the **absence** of a `Completed` status —
  asserting `Pending` positively would freeze behaviour this slice does not endorse.
- **`AcpPermissionTransport::ask` blocks on an untimed `std::sync::mpsc::Receiver::recv()`.** A
  client that receives the request and never answers hangs the child — **measured** in T3's red, not
  inferred. Unchanged by this slice and recorded; a timeout belongs to a slice that has timeout
  machinery.
- **The Windows leg is observed locally; the macOS and Linux legs are unobserved** until this
  repository has a remote — the standing caveat of slices 004–017, unamended. No `#[cfg]` anywhere:
  the fixture is `TempDir` plus `Path::join`, and `FsRoot` canonicalizes both sides of its
  containment check already.

## Out of scope

Deliberately not done, so nobody helpfully does it:

- **Any change under `crates/*/src/`** — including the two residuals above.
- **A new `StepKind`, or a second `Approval` step** recording the client's answer explicitly. The
  verified deny shape is `ToolCall` + `Approval` + no `ToolResult`, and this slice matches it rather
  than inventing one. `AcpPermissionTransport` holds no `Ledger` and is constructed *inside*
  `ToolGateway`; giving it one inverts the gateway/transport relationship Principle IV's decorator
  design rests on. And a new `StepKind` changes `hash()`'s input space and every chain-shape
  assertion in the tree — an enormous blast radius to record something already derivable twice over.
- **The `Cancelled` outcome.** `p3_a_cancelled_answer_denies_without_reaching_the_transport` already
  covers it at unit level, and this slice is scoped to the two outcomes an editor's user actually
  picks.
- **`AllowAlways` / `RejectAlways`, or any "remember my answer" persistence.** The transport offers
  neither kind; adding one is a feature, not a proof.
- **New mutating tools, new `ToolAccess` classification, any UI.**
- **A live-model test.** A live model cannot be made to answer Deny and cannot be relied on to call
  `fs_write` at all; determinism in both branches is the whole point of the `StubProvider`. Slice
  017's `#[ignore]`d T11/T13 pattern does not transfer.
- **A `shell` connector.** Still deferred; ADR-0006 scopes it Windows-first and it is not this slice.
- **The other named residuals**, all carried forward untouched from slice 017: the
  `canonicalize`-to-open TOCTOU fix, `role: "tool"` / `tool_call_id` conversation replay, raw
  wire-byte capture, streaming (SSE), provider authentication, a config file, `--json` output, and
  the slices-008-vs-014 `serde_json/preserve_order` reconciliation.
- **`crates/heddle-silo/`, `spikes/`** (ADR-0004 D2), **`.github/`, `rust-toolchain.toml`,
  `Cargo.toml`, `Cargo.lock`** — all asserted empty in the control diff.
