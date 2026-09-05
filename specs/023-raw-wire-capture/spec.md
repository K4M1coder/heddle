# Feature Specification: the provider's literal bytes as a `WireExchange` step (v0 slice)

**Feature Branch:** `023-raw-wire-capture` · **Created:** 2026-09-04 · **Status:**
Implemented (v0 slice) · **Input:** the residual carried forward since slice 011 and named in
`specs/012-model-gateway/tasks.md` in both *Out of scope* and *Next slice* — **raw-wire-byte
capture** — repeated in slices 013–022 and deferred once more in slice 022's rejected-alternatives
register as *"the standing separate item"* · Constitution III (**test-first**), IV (**explicit
boundaries**), V (**traceability**), VI (**security**, NON-NEGOTIABLE), VII (**no capability without
a real need**) · design §4.5 (the Gateway as "traceability chokepoint"), §4.11 ("exact model I/O"),
Spike 1's criterion C1.

The Ledger recorded `StepKind::LlmRequest` from a `TurnRequest` and `StepKind::LlmResponse` from a
`TurnResponse` — **both heddle-core's own types**, on this side of the `ModelClient` port. The bytes
that actually crossed the socket were built and consumed entirely inside `OpenAiCompatClient::turn`
and dropped when it returned. This slice puts them on the chain.

## What this slice changes for a user

**A run's chain can now disagree with itself, and that is the point.** Before, an operator reading a
chain read the input to the translation and the output of it, never the wire between them. A bug in
`ChatRequest`'s `Serialize`, a provider answering a shape `ChatResponse` silently ignores, a
mistranslation in `impl From<&Message> for ChatMessage` — none of them was visible anywhere in the
product. Now `heddle ledger show` on the new step prints the exact bytes, and they can be read against
the `LlmRequest` and `LlmResponse` steps that bracket them.

**A failed turn now leaves the bytes that caused the failure.** A provider answering HTTP 500, or
answering a body the parser cannot read, previously produced an `Err` and left nothing on the chain
but the translated request. The exchange is now recorded before the error propagates. **This is the
case with no other witness anywhere in the product**, and it is the single most valuable thing the
slice buys.

**A turn that never reached a socket still records nothing.** A connection refused or a timeout
leaves no exchange, so the chain never claims bytes crossed when none did.

**No CLI flag, no new command, no config key.** `heddle ledger log` and `heddle ledger show` render
the new kind with **no CLI change at all**, because `kind_name` derives the column from
`serde_json::to_value` rather than matching the enum.

## Six things a reader must know up front

1. **The captured request is not a re-serialization — it is the transmitted buffer itself.**
   `turn` serializes exactly once into `let body: String` and hands `&body` to ureq, whose
   `impl AsSendBody for &str` wraps the slice as `BodyInner::ByteSlice` with `Some(self.len())` as
   content-length: no copy, no transformation, no re-encode. The `String` is then **moved** into the
   `WireExchange`. Divergence between what crossed the wire and what the chain says crossed it is
   not merely unlikely; there is only one buffer, so it is unrepresentable. See `plan.md` D2.
2. **Reusing the existing `Redactor` would have leaked, and this was measured rather than argued.**
   `redact_json` serializes *then* scrubs, precisely because a secret containing a quote is escaped
   during serialization. A raw wire body inverts that premise: it arrives **already escaped**, so
   the literal needle is absent from it. A `redact`-only implementation puts the escaped secret on
   the chain in cleartext — observed, recorded verbatim in `tasks.md` under *Observed red* (red B).
   Hence `Redactor::redact_wire`, which matches each secret in both forms.
3. **One step per exchange, not a `WireRequest`/`WireResponse` pair.** The request bytes only become
   available to the core *after* `turn` returns, so a two-step shape would append a "request" step
   strictly after its own response had happened — a chain whose ordering lies. `ToolCall`/`ToolResult`
   are a pair because they bracket an unbounded interval during which other steps land; a single
   HTTP round trip has no such interior.
4. **The port grew a defaulted method, not a changed signature.** `ModelClient::take_wire_exchange`
   defaults to `None`, which for a scripted or in-process client is the *true* answer rather than a
   convenience: there were no bytes. Every existing implementation compiles and behaves identically,
   so no `StepKind` sequence asserted by a stub-model test changes.
5. **Capture is unconditional. There is no flag.** A flag would make "no evidence" the default,
   reinstating the defect under another name; Constitution V says traceability cannot be bypassed,
   and Constitution VII cuts against a capability with no caller. See `plan.md` D5.
6. **Bodies only — not headers, not the request line, not framing.** This is exactly where Spike 1's
   own adversarial correction drew the line, and it keeps the slice clear of the separately-named
   provider-authentication residual: the moment headers are captured, an `Authorization: Bearer`
   becomes a chain payload. See `plan.md` D6.

## Requirements

- **FR-001** The Ledger MUST record, for every provider round trip that completed, a
  `StepKind::WireExchange` step carrying the literal request body, the literal response body, the
  URL and the HTTP status.
- **FR-002** The recorded request bytes MUST be the same buffer transmitted to the provider, not a
  re-serialization of the same value.
- **FR-003** The recorded response bytes MUST be the same string the parser consumed.
- **FR-004** A turn whose transport failed before an answer MUST record **no** `WireExchange` step.
- **FR-005** A turn that received an answer MUST record the exchange **even when the run then
  fails** — a non-2xx status, or a body the parser cannot read.
- **FR-006** Both bodies MUST be scrubbed with a redactor that matches each secret in its literal
  form **and** in the form `serde_json` would have escaped it to.
- **FR-007** `ModelClient::take_wire_exchange` MUST be defaulted, so every existing implementation
  compiles unchanged and reports the truth (`None`) for a client with no wire.
- **FR-008** An exchange MUST belong to exactly one turn: taken, not borrowed, so a client that
  fails before reaching a socket cannot re-offer the previous turn's bytes.
- **FR-009** Adding the variant MUST NOT move any existing step's id, and a pre-023 chain MUST still
  load and verify.
- **FR-010** The new step MUST require no change to the CLI, the ACP transcript, or the silo schema.

## Success criteria

- **SC-001** The recorded `request` is string-equal to the raw body a real socket on the other end
  read, and the recorded `response` is string-equal to the exact bytes that socket wrote.
- **SC-002** A secret containing a `"` reaches no payload of the run in either its literal or its
  escaped form; the exchange carries `***`; the payload still deserializes; `verify_chain` passes;
  and the **provider was still sent the real secret** — the control that proves only the record was
  scrubbed.
- **SC-003** A plain alphanumeric secret is scrubbed identically.
- **SC-004** An unreachable provider yields the kinds `[IterationBoundary, LlmRequest]` and nothing
  more.
- **SC-005** A provider answering `500`, and one answering an unparseable `200` body, each yield
  `[IterationBoundary, LlmRequest, WireExchange]` with the offending bytes recorded verbatim.
- **SC-006** A chain of only pre-023 kinds, persisted through `SqliteLedgerStore` and reopened,
  returns identical ids and payloads and still verifies; appending a `WireExchange` step extends
  that same chain.
- **SC-007** Against a real local provider, the captured response carries the provider's own `usage`
  object, and its `total_tokens` equals the number the loop budgeted against.

## The rejected-alternatives register

- **Widening `turn` to `turn(&mut self, req, sink: &mut dyn WireSink)`.** Rejected: it rewrites every
  implementation and every call site to thread a parameter all but one implementation ignores, and
  puts a capture concern into the signature of the port's only real method (Constitution VII).
- **An observer closure installed on `OpenAiCompatClient`.** Rejected: the closure would need
  `&mut Ledger`, which `NativeLoop::run` borrows for the whole call. The workaround is a shared
  `RefCell` the loop drains afterwards — i.e. exactly `take_wire_exchange`, with interior mutability
  and a runtime borrow panic added. Strictly worse.
- **Carrying the bytes on `TurnResponse`.** Rejected: `TurnResponse` is itself serialized into the
  `LlmResponse` payload, so the bytes would be duplicated inside it; and an `Err` return carries no
  `TurnResponse` at all, losing the failure case that is the point of the slice.
- **A `WireRequest`/`WireResponse` pair.** Rejected, point 3 above.
- **Reusing `redact_json` or `redact` for the bodies.** Rejected and *measured*: point 2 above.
- **Teaching `redact` itself about escaped forms.** Rejected: it would change behaviour at three
  existing call sites (`redact_call`'s tool name, `call_captured`'s outcome content, `mediate`'s
  denial wording) for no need in this slice. A separate method keeps the blast radius at one new
  call site.
- **Parsing the raw body and scrubbing it as a `Value`.** Rejected: it would re-serialize the body,
  destroying the exact property the slice exists to establish.
- **A `--capture-wire` flag or config key.** Rejected, point 5 above.

## Assumptions and residuals

- **Assumption — the response capture inherits ureq's existing decoding.** The captured string is
  ureq's `Body::read_to_string`, which applies a 10 MB limit and `lossy_utf8(true)`, so a non-UTF-8
  byte becomes U+FFFD and a larger body is cut. **Both properties already govern what `ChatResponse`
  is parsed from**; this slice neither introduces nor worsens them, and the captured bytes are
  exactly the bytes heddle-core acted on — which is the auditable claim that matters. `gzip` is not a
  factor: `ureq` is declared with `default-features = false`, so no content-encoding is decoded.
- **Assumption — chain and memory growth roughly doubles for model I/O.** Accepted deliberately
  (`plan.md` D5), bounded by ureq's existing 10 MB body limit. If chain size becomes a real
  constraint it is a retention concern for the silo (design §7), not a per-run switch.
- **Backward compatibility is claimed; forward compatibility is not.** An old chain read by new code
  works (FR-009). A chain written *with* `wire_exchange` steps and then read by pre-023 code fails
  its `serde_json::from_str::<StepKind>` in `SqliteLedgerStore::load` — **loudly, at load, never
  silently**. That is the direction the additive-only rule actually governs.
- **Residual — HTTP headers, the request line and transport framing are not captured.** To be
  revisited *in the same slice that adds provider authentication*, not before (`plan.md` D6).
- **Residual — the same JSON-escape redaction hole exists today on `StepKind::ToolResult`**,
  whenever a tool's `content` is itself JSON text carrying a quote-bearing secret. Discovered while
  writing D4. `redact_wire` is the fix and pointing `call_captured` at it is a one-line change, but
  it is a different payload with different tests, so it is named here rather than done here.
- **Residual — streaming (SSE)** would change the capture shape entirely. A standing separate item.

## Out of scope

- **HTTP headers, the request line, and transport framing.** Revisit with provider authentication.
- **Provider authentication.** `plan.md` D6 is written the way it is to keep this slice from
  touching it.
- **MCP / tool-transport wire bytes.** A separate concern with its own capture via
  `ToolCall`/`ToolResult`.
- **Any general network-tracing subsystem**, any on/off flag, any config key, any sampling or
  retention policy for the new step.
- **Fixing the `ToolResult` JSON-escape redaction hole**, recorded as a residual above.
- **Replay from `WireExchange`**, a `heddle ledger diff` comparing raw and translated payloads, or
  any ACP `SessionUpdate` for the new kind. No caller. The new kind is deliberately inert on the ACP
  transcript: raw provider bytes are audit evidence, not something to stream to an editor.
- **Streaming (SSE).**
- **`spikes/`** (ADR-0004 D2) — read as evidence for C1's actual wording, left byte-identical.
- **A PR.** No real remote; the bare mirror under `D:/claudecode/heddle-origin.git` exists only for
  Archon's worktree isolation.
