# Feature Specification: tool advertisement — the model learns a tool exists (v0 slice)

**Feature Branch:** `015-tool-advertisement` · **Created:** 2026-09-03 · **Status:** Implemented (v0
slice) **Input:** `specs/014-ledger-redaction/tasks.md` "Next slice" — *"**tool advertisement** — a
`tools` field on `TurnRequest`, which needs tool discovery from the Tool Gateway. Still the largest
untested-in-production path in the tree"* · ADR-0004 D3's sixth v0 item (*"MCP tools
(fs/git/shell)"*) · Constitution III (**test-first**), IV (**inverted coupling**), V
(**traceability**), VI (**deny-by-default, secrets redacted from logs**), VII (**no capability
without a real need**) · design §4.2, §4.3, §4.5.

Since slice 005 Skein has had a governed Tool Gateway; since slice 006 the loop mediates every call
through it; since slice 007 an allowlist decides; since slice 014 the tool payloads it records are
scrubbed. The *response* path is complete: `OpenAiCompatClient` already translates a provider's
`tool_calls` into `ToolCall`s, and `NativeLoop::mediate` already governs them.

The **request** path does not exist. `TurnRequest` carries `{run_id, messages}` and nothing else, so
no model is ever told a tool exists, so no model ever asks for one.
`crates/skein-gateway/tests/openai_compat.rs` says so in a comment on its own tool-call test: *"This
client advertises no tools, so a provider should not send these."*

This slice builds the request path and nothing else. It ships **no connector**: the `fs` server that
gives the advertisement something to advertise is slice 016. This is the shape slice 005 shipped the
whole `ToolGateway` in — mechanism first, proven by tests, wired to a real server in its own slice —
and it is what makes each half independently revertible.

## What this slice changes for a user

**Nothing, observably.** No new flag, no new subcommand, no new dependency. Both loop-running
commands still wire `NoTools` behind an empty allowlist, so:

- the bytes `skein chat` puts on the wire are **byte-identical** to slice 014's;
- every Ledger payload of every run has the **same shape** it had before;
- old chains still deserialize, and chains written after this slice still deserialize if it is
  reverted.

That is a deliberate property, not an accident of a small change: `TurnRequest.tools` and the
gateway's `ChatRequest.tools` are both skipped when empty, so a run that advertises nothing
serializes no `tools` key at all. Slice 014 had to announce a payload-shape change to its readers;
this slice must not need to, and SC-007 pins it.

## Four things a reader must know up front

1. **Advertisement is discovered, never hand-written.** `ToolGateway::advertise` asks the transport
   for its catalogue and filters it by the policy. The alternative — typing `ToolSpec`s next to the
   allowlist — would put each tool's JSON Schema in the tree twice, once derived from the server's
   real parameter type and once by hand, and the two would drift. The model sends arguments matching
   the advertised schema and the server validates against the real one, so drift there is a runtime
   failure the compiler cannot see. Discovery makes it unrepresentable.
2. **The filter lives inside `ToolGateway`, which owns both the policy and the transport.** "You
   cannot advertise what the policy forbids" is therefore structural rather than reviewed — the same
   move slice 005 made for `call`. The CLI is not trusted to intersect two lists correctly.
3. **`ToolTransport::list` is defaulted, and that is the *opposite* of `NativeLoop::new`'s required
   `Redactor` for a reason that must not be lost.** There the silent default was the **unsafe** one:
   a loop with no redactor records cleartext. Here it is the **safe** one: a transport that does not
   override `list` advertises nothing, which is deny-by-default. The distinction is written into the
   trait's docstring, because "the project distrusts silent defaults" is the right instinct and this
   is the exception that has to justify itself.
4. **A `Mutating` tool with no approval is still advertised.** It is visible to the model and refused
   at call time with a reason the model is told. Denying at advertisement would make `skein-acp`'s
   `AcpPermissionTransport` permanently unreachable: its entire design is that the policy allows and
   the *client* decides, and `ToolGateway::call_captured` consults the policy **before** the
   transport. FR-004 and SC-004 pin this deliberately, so nobody later "tightens" it and silently
   disconnects the ACP permission gate.

## Functional requirements

- **FR-001** `skein_core::ToolSpec` — `{ name: String, description: String, parameters:
  serde_json::Value }` — lives in `tool.rs` beside `ToolCall` and is re-exported from `lib.rs`.
  `parameters` is an opaque `Value`: the schema is the server's, and a typed mirror of JSON Schema in
  `skein-core` would be a second source of truth for a document the core never interprets.
- **FR-002** `ToolTransport::list(&mut self) -> Result<Vec<ToolSpec>>` has a defaulted body returning
  `Ok(Vec::new())`, so all nine existing `impl ToolTransport` sites compile untouched and the
  120-test baseline stays a live control.
- **FR-003** `ToolGateway::advertise(&mut self) -> Result<Vec<ToolSpec>>` returns the transport's
  catalogue filtered to the policy's allowlist, **in allowlist order** — the operator's order, not
  the server's. A tool the server does not offer is absent rather than fabricated.
- **FR-004** `advertise` includes an allowlisted `Mutating` tool whose name is not in `approved`.
- **FR-005** `TurnRequest.tools: Vec<ToolSpec>` carries `#[serde(default, skip_serializing_if =
  "Vec::is_empty")]`.
- **FR-006** `NativeLoop::run` calls `advertise` exactly once per run, **after** the pre-flight
  `ctl.should_exit(false)` check — so a zero-budget run makes no round trip — and **before** the
  message vector is built. The resulting specs are stamped into every `TurnRequest` of the run.
- **FR-007** A `list` failure is **fatal to the run**, matching how `mediate` treats any
  non-`ToolDenied` transport error: an inventory that could not be read leaves the run's capabilities
  unknown. It propagates before any `IterationBoundary` or `LlmRequest` is appended.
- **FR-008** `AcpPermissionTransport::list` **overrides** the default and forwards to its inner
  transport. Permission is asked per *call*; enumerating a catalogue is not a call.
- **FR-009** `skein-gateway`'s `ChatRequest` gains `tools`, serialized as OpenAI's
  `[{"type":"function","function":{"name","description","parameters"}}]` with `strict` omitted, and
  skipped entirely when empty.
- **FR-010** No new `StepKind`. The advertisement travels inside `TurnRequest`, which `run` already
  captures as `LlmRequest` through `Redactor::redact_json` — so descriptions and schemas are scrubbed
  by `redact_value`'s existing recursion for free.

## Success criteria

- **SC-001** A `ToolSpec` round-trips through a Ledger payload with its schema intact.
- **SC-002** A transport that does not override `list` advertises nothing.
- **SC-003** `advertise` over a catalogue wider than the allowlist returns only allowlisted names, in
  allowlist order, and omits an allowlisted name the server does not offer.
- **SC-004** `advertise` includes an unapproved `Mutating` tool.
- **SC-005** `NativeLoop::run` puts the advertised specs in **every** `TurnRequest` of a multi-turn
  run and calls `list` exactly once.
- **SC-006** A `list` failure ends the run with that error and **no** step on the chain.
- **SC-007** A run with an empty catalogue produces an `LlmRequest` payload with **no `tools` key**,
  and `openai_compat.rs`'s byte-exact no-tools assertion passes with an **unchanged body**.
- **SC-008** `AcpPermissionTransport::list` returns its inner transport's catalogue and asks the
  client no permission question to do it.
- **SC-009** One advertised tool produces the exact OpenAI `tools` bytes on the wire.
- **SC-010** All 120 pre-existing tests pass; the only edits to pre-existing test files are the
  mechanical `TurnRequest { … }` field additions the plan named in advance.
- **SC-011** `git diff dev -- crates/skein-cli/ crates/skein-silo/ spikes/ .github/
  rust-toolchain.toml` is **empty**, and so is `git diff dev -- Cargo.toml crates/*/Cargo.toml`:
  zero new packages, zero new dependency edges.

## Assumptions

- **The OpenAI wire shape was verified, not assumed** (plan D6). `strict` is omitted: it is an OpenAI
  structured-outputs extension, Ollama documents its compatibility layer as experimental while
  listing `tools` as supported, and an unrecognised key buys a local provider nothing.
- **The tool-result feedback path stays as it is.** Strict OpenAI wants `{"role":"tool",
  "tool_call_id":…}` replies; `NativeLoop::mediate` feeds results back as user-role text under a
  `[tool_result …]` label, and `ChatMessage` never echoes `tool_calls` into the assistant history.
  Because no `tool_calls` are ever *sent*, there is no dangling call id to satisfy and the history
  stays a valid OpenAI sequence (`user`, `assistant`, `user`). Changing it would reopen
  `native_loop.rs`'s explicit anti-injection decision, which Constitution VI backs. Recorded as a
  residual, not fixed here. **Closed by slice 022**, which found that decision's origin (slices 005
  and 006) to say something narrower than this reference implies, and closed the residual on a
  measured information-loss defect rather than on the compatibility grounds sketched here — those
  turned out to be near-empty. See `specs/022-tool-result-wire-format/spec.md`.
- **Machinery with no caller in the shipped binary is acceptable for this half**, on slice 005's
  precedent. Slice 014's "the fix has a caller in the shipped binary" language was about a **fix**.
- **The Windows leg is observed locally; the macOS and Linux legs are unobserved** until the
  repository has a remote — the standing caveat of specs 004–014.

## Out of scope

Deliberately not done, so nobody helpfully does it:

- **Any connector at all**, and therefore `crates/skein-connectors`, `FsRoot`, `--fs-root`, and every
  `skein-cli` change. That is slice 016, and it is the half that gives this one a caller.
- **`git` and `shell` tools.** Named in ADR-0004 D3 and deferred with reasons in 016's plan; shell
  only after an access-scope boundary exists.
- **Deriving `ToolAccess` from MCP tool annotations.** Classification stays operator configuration;
  `tool.rs` already commits to this, and a server that self-declares `readOnlyHint: true` would
  otherwise be trusted to classify its own risk.
- **Denying advertisement to an unapproved `Mutating` tool** — FR-004 says why it would break ACP.
- **`role: "tool"` / `tool_call_id` conversation replay**, `strict: true`, `tool_choice`, parallel
  tool calls, streaming (SSE).
- **A new `StepKind` for advertisement** — FR-010; it is already inside the captured `LlmRequest`.
- **Re-listing per turn.** The catalogue does not change mid-run; a per-turn `list` is one round trip
  per iteration for nothing.
- **A separate `ToolCatalog` trait.** A transport that can call but not enumerate is not a thing MCP
  models, and two traits would have to be injected and kept in sync at every wiring site.
- **Raw wire-byte capture**, provider authentication, a config file, `--json` output — still on slice
  014's "next slice" list, and none of them are this.
- **`crates/skein-cli/`, `crates/skein-silo/`, `spikes/`** (ADR-0004 D2), **`.github/`,
  `rust-toolchain.toml`.**
