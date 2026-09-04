# Feature Specification: a `ToolResult` capture is scrubbed as wire bytes, not as plain text (v0 slice)

**Feature Branch:** `029-toolresult-redaction` · **Created:** 2026-09-04 · **Status:** Implemented
(v0 slice) · **Input:** the residual slice 023 left when it introduced `Redactor::redact_wire` for
the model-I/O bodies and did **not** revisit the one other already-serialized body in the product ·
Constitution III (**test-first**), IV (**explicit boundaries**), V (**traceability**), VI
(**security**, NON-NEGOTIABLE) · design §7.13 (secrets are references, never captured material).

Slice 014 gave the gateway a `Redactor` and scrubbed `CapturedResult.content` with
`Redactor::redact`, whose needle is the secret **as written**. Slice 022 then made
`ToolOutcome.content` an already-serialized `CallToolResult` — `serde_json::to_string(&result)` at
`skein-mcp/src/lib.rs:57` — and slice 023 introduced `Redactor::redact_wire` for exactly that
premise, applying it to the two model-I/O bodies and to nothing else. The gateway's line was never
revisited. A configured secret containing a `"`, a `\` or a newline is on a tool result in JSON-escaped
form, so the literal needle is absent and the secret is captured in cleartext.

## What this slice changes for a user

**A secret with a quote in it is now scrubbed from a tool result.** Before, an operator who
registered `sk-"awkward"-SECRET-abc123` and whose tool read it off disk got it back, unscrubbed, on
the chain and on the wire. Now it is `***`, exactly as a quote-free secret already was.

**The leak was never confined to the `ToolResult` step.** `NativeLoop::mediate` feeds
`captured.content` back into the conversation (`native_loop.rs:186`), so the same unscrubbed bytes
reached the next turn's `LlmRequest` payload, that turn's `WireExchange` payload, **and the HTTP
request body sent to the provider**. All four are measured below. One line at the source closes all
four; nothing downstream can, because by the time the bytes reach the wire the secret is escaped
*twice* and `redact_wire`'s own needles no longer match either.

**Nothing changes for a secret without those characters.** `redact_wire` tries the literal form
first and adds the escaped form only when the two differ, so every existing run's capture is
byte-identical.

**No CLI flag, no tool, no connector, no port, no `StepKind` and no payload shape changes.** The
diff is one identifier in one line of `skein-core`, its comment, and the tests.

## Four things a reader must know up front

1. **This is a one-line fix, and the reason it is worth a slice is the test.** The assertion shape
   the existing suite uses — `payloads.iter().all(|p| !p.contains(SECRET))` at
   `skein-connectors/tests/governed_fs_run.rs:685` — **cannot fail on this defect**, measured. The
   `ToolResult` step payload is `serde_json::to_string(&CapturedResult)` over a `content` that is
   *itself* serialized JSON, so the secret sits there doubly escaped: `sk-\\\"awkward\\\"-…`. Neither
   the literal needle nor the file's own single-escaped `escaped()` helper is a substring of that. A
   test written the obvious way is green while the secret is in plain sight.
2. **Therefore every assertion in this slice runs against the *parsed* capture.** `replay_tool_calls`
   yields a `CapturedResult`; its `content` is parsed as a `serde_json::Value`; the assertion is on
   the decoded string at `content[0].text`. That is the only form in which the secret appears as
   written, and it is the form a replay consumer actually sees.
3. **`ToolCall`'s own capture is correct and is deliberately left alone.** `redact_call` scrubs the
   `Value` and `call_captured` serializes *afterwards*, so the needle there really is the secret as
   written. Measured: the same awkward secret in a call's arguments captures as `{"token":"***"}`.
   Changing that line too would be a change with no failing test behind it.
4. **Exactly one of the five plain-`redact` call sites in the product was wrong.** The other four
   scrub genuinely plain text — a model delta, a URL, a tool name from a refusal, a tool name from a
   parsed response. The audit is in `plan.md`'s D3 so no future slice has to re-derive it.

## Requirements

- **FR-001** `CapturedResult.content` MUST be scrubbed with the already-serialized-JSON premise:
  each configured secret matched in both its literal and its JSON-escaped form.
- **FR-002** A configured secret containing `"`, `\` or a newline MUST NOT appear in the decoded
  text of a `ToolResult` step's captured content.
- **FR-003** FR-002 MUST hold for a secret that arrives from a **real** tool through a **real** MCP
  server — not only from a transport double.
- **FR-004** The scrubbed capture MUST still parse as the JSON body it was, so replay is unaffected.
- **FR-005** The redaction marker MUST be visible in the decoded text, so the secret is provably
  scrubbed rather than merely absent for some other reason.
- **FR-006** The raw outcome handed to the trusted caller MUST still carry the real secret, and the
  transport MUST still receive the raw call. Unchanged from slice 014, and re-asserted here because
  the fix is on the line between them.
- **FR-007** A configured secret with no JSON-escapable character MUST capture byte-identically to
  before this slice.
- **FR-008** The `ToolCall` step's capture MUST be unchanged, including for an awkward secret in a
  call's arguments.
- **FR-009** The unconfigured case MUST remain uncovered and remain stated: a credential in a file
  the operator never registered still reaches the chain in cleartext. This slice changes which
  *configured* secrets are found, not what is configured.

## Rejected alternatives

| # | alternative | why not |
|---|---|---|
| 1 | scrub the `LlmRequest` payload / the wire body harder instead | by then the secret is escaped **twice** (measured: `sk-\\\\\\\"awkward…` on the wire) and `redact_wire`'s two needles both miss. A third needle for double-escaping, then a fourth, is an escaping arms race whose fix is to scrub at the one place the bytes are still singly escaped |
| 2 | make `redact` itself try both forms and delete `redact_wire` | it would splice `***` into plain text that merely *looks* escaped, and it erases a distinction the type-level comments at `tool.rs:213`/`tool.rs:231` exist to keep. `redact_json`'s premise — scrub before serializing — stays correct and stays different |
| 3 | parse `outcome.content` and use `redact_json` on the value | the transport's body is not guaranteed parseable by `skein-core`, which owns no MCP vocabulary; a tool that returned non-JSON would become an error where today it is captured. `redact_wire` needs no such assumption |
| 4 | change `ToolOutcome.content` to a `Value` | it pushes MCP's result shape into the core port that slice 022's D-notes deliberately kept out of it, for a one-identifier fix |
| 5 | assert with `contains()` over the raw step payload | measured to pass while the secret leaks. This is FR-002's whole reason for naming the *decoded* text |
| 6 | apply the same change to `redact_call` | no failing test behind it; the order there is already correct (reader note 3) |

## Out of scope

- **Secrets the operator never configured.** FR-009; unchanged from slice 014's own spec.
- **The double-escaped forms further down the pipe.** Closed as a consequence of fixing the source,
  not by scrubbing them; rejected alternative 1.
- **`Redactor`'s public surface.** No new method, no signature change.
- **The ACP `stream.rs` delta path.** Genuinely plain text, and its comment already says why.
