# Feature Specification: SSE streaming from the local provider, with live ACP `AgentMessageChunk`s (v0 slice)

**Feature Branch:** `025-streaming-sse` · **Created:** 2026-09-04 · **Status:**
Implemented (v0 slice) · **Input:** the residual named in `specs/013-acp-agent/tasks.md` — *"streaming
(SSE), together with incremental ACP `AgentMessageChunk` notifications. Today a client sees one chunk
per turn, after the turn."* — and carried forward unchanged through slices 014–024, where slice 023's
own *Out of scope* records it as a standing separate item because *"streaming would change the capture
shape entirely"* · Constitution III (**test-first**), IV (**explicit boundaries**), V
(**traceability**), VI (**security**), VII (**no capability without a real need**), VIII (**loop
discipline**, NON-NEGOTIABLE) · design §4.5 (the Gateway as traceability chokepoint), §4.11 (exact
model I/O).

`OpenAiCompatClient::turn` sent `"stream": false` and blocked until one complete HTTP body arrived.
`heddle-acp`'s `project_updates` derived exactly one `AgentMessageChunk` per `StepKind::LlmResponse`
step, and the `session/prompt` handler sent every one of them *after* the whole run finished. An
editor driving `heddle acp-agent` saw nothing for the entire duration of a model turn, then the whole
answer at once. This slice makes the provider stream and the answer arrive as it is produced.

## What this slice changes for a user

**An editor sees the answer being written.** `session/prompt` is still one request with one response,
but `session/update` notifications carrying `AgentMessageChunk`s now arrive *while it is outstanding*,
one per non-empty content delta the provider produced. That is the whole product claim, and it is the
one asserted end-to-end in `cli_acp_agent.rs` by holding the provider's socket open mid-stream and
proving a chunk has already reached the client.

**The chain now records SSE, not a JSON object.** `StepKind::WireExchange`'s `response` holds the
literal event stream — every `data:` line and every blank separator, verbatim, in wire order — and the
step gains a `streamed` flag saying so. Slice 023's claim that the chain holds *the bytes that
crossed*, not our reconstruction of them, is preserved exactly; under streaming it is worth more,
because the reassembly is now the most error-prone thing between the socket and `TurnResponse`.

**Nothing else about a run changes.** `heddle chat`'s stdout is byte-identical. The `StepKind`
sequence of every run is unchanged. The token budget is still enforced from the provider's own
metering. A client whose model does not stream sees precisely what it saw before.

## Six things a reader must know up front

Everything here was measured against the real Ollama on this machine, not assumed; `plan.md` §0.2
records the driving session in full.

1. **Streaming is unconditional, and the non-streaming request path is deleted.** There is no
   `--stream` flag and no config key. `ChatResponse`/`Choice`/`ChoiceMessage`/`ResponseToolCall` are
   gone, replaced by the chunk types. One wire path, not two.
2. **`"stream_options": {"include_usage": true}` is mandatory, not decorative.** Ollama sends **no
   `usage` object at all** under a bare `"stream": true`, and `OpenAiCompatClient::metered` refuses a
   turn with no metering — deliberately, so Constitution VIII stays enforceable. Flipping `stream`
   without `stream_options` breaks every run. With it, a final event carrying `"choices":[]` and the
   provider's own `usage` arrives immediately before `data: [DONE]`.
3. **Ollama delivers each tool call whole, and the accumulator concatenates anyway.** The
   accumulator is keyed by the delta's own `index` and appends, because `heddle-cli/src/wiring.rs`
   documents a LiteLLM sidecar as a supported deployment ("a different `--base-url` and no code
   change") and LiteLLM proxying a real OpenAI or Anthropic model *does* fragment `arguments` across
   chunks. The whole-arrival case is the degenerate case of the same three lines.

   **Three framings are tested, because three were seen.** `plan.md` §0.2(4) recorded all calls
   arriving in one delta; re-driving `qwen3.8:27b` during implementation produced two complete calls
   in two **separate** events with `index` 0 and 1. Both halves that the design rests on held — each
   call complete within its event, each carrying an explicit `index` — but the observed shape lies
   between the plan's two cases and is exactly what an accumulator that *replaced* its call list per
   event, rather than merging into it, would fail on while passing everything else. See
   `tasks.md` *Deviations* 1.
4. **`delta.reasoning` is discarded.** Ollama sends it in *both* modes and the non-streamed
   `ChoiceMessage` never had such a field, so it was already being ignored. Absorbing it now would put
   text into `TurnResponse.message` the non-streamed path never had, and would show an editor a
   chain-of-thought that appears nowhere in the Ledger.
5. **One Ledger step per exchange, not one per chunk.** Measured: a single short turn from a reasoning
   model is **146 SSE events, 32,906 bytes**, against 685 bytes non-streamed — roughly **48×**. A step
   per chunk would put thousands of steps on one run's chain for zero added fidelity, since the
   concatenation of the chunks *is* the stream.
6. **The event separator is bare `\n\n`.** Confirmed with `cat -A`: every line ends LF, never CRLF.
   The reader is nonetheless byte-oriented and keeps its terminators, so a provider that frames with
   CRLF is captured as it framed rather than silently normalized.

## Requirements

- **FR-001** Every chat-completions request MUST carry `"stream": true` and
  `"stream_options": {"include_usage": true}`. There MUST be no flag, argument or config key by which
  a run sends either differently.
- **FR-002** The client MUST accumulate `delta.content` across events into the single
  `TurnResponse.message`, in wire order.
- **FR-003** The client MUST accumulate tool calls keyed by the delta's `index`, concatenating `id`,
  `function.name` and `function.arguments` fragments, and MUST produce the identical `TurnResponse`
  whether a call arrived whole or split across events.
- **FR-004** An accumulated `arguments` of `""` MUST be parsed as `{}`, so a no-argument call streams
  to the same `ToolCall` the non-streamed path produced from `"arguments":"{}"`.
- **FR-005** `delta.reasoning` MUST NOT reach `TurnResponse.message`.
- **FR-006** A stream carrying no `usage` MUST fail with the existing metering refusal, never be
  metered as zero (Constitution VIII).
- **FR-007** `tokens_used` MUST come from the stream's own `usage` event, and `final_output` MUST be
  computed from `finish_reason == "stop"` with no tool calls — the same expression, and the same
  verdict, as before this slice.
- **FR-008** `WireExchange.response` MUST hold the literal bytes read off the socket — every `data:`
  line and every blank line, in order — and MUST NOT be a re-serialization of the reassembled object.
- **FR-009** `WireExchange` MUST gain a `streamed: bool`, `#[serde(default)]`, true when the recorded
  response is an event stream and false when it is one body — which is what a non-2xx error genuinely
  is, since the provider answers a plain JSON body with a non-2xx status.
- **FR-010** A turn that received *any* answer MUST record the exchange even when the run then fails,
  including a stream that failed part-way, which MUST record what had arrived.
- **FR-011** `ModelClient` MUST gain a **defaulted** `set_text_sink`, so every existing implementation
  compiles unchanged and a client that produces its text atomically keeps dropping the sink — the true
  answer, not a convenience.
- **FR-012** `CancellableModel` MUST forward `set_text_sink`; inheriting the default would silently
  disable streaming in the one crate that needs it.
- **FR-013** An ACP session MUST install its sink in `HeddleSession::new`, so a session cannot be
  constructed without one.
- **FR-014** Text pushed to the ACP client MUST be scrubbed with the session's `Redactor`, and an
  empty delta MUST NOT produce a notification.
- **FR-015** When a session streamed, the post-run projection MUST NOT re-send the same text; when it
  did not, the projection MUST be exactly what it was before this slice. `project_updates` itself MUST
  NOT change meaning.
- **FR-016** The stream reader MUST be explicitly bounded. `ureq`'s `Body::as_reader()` is unlimited
  by default, so the 10 MiB cap `read_to_string` applied MUST be restored deliberately.
- **FR-017** Reading MUST be lossy-UTF-8 and MUST keep line terminators, so the capture is
  byte-faithful and a non-UTF-8 byte does not error the read where it previously became a
  substitution.
- **FR-018** A 200 that is not an event stream at all MUST be refused **showing its body**, and a
  well-framed stream whose events never carry a `choices[0]` MUST be refused as such — neither may
  fall through to the metering refusal, which would name the wrong problem.

## Success criteria

- **SC-001** A stream of several content deltas plus a `finish_reason` chunk plus a `usage` chunk
  yields a `TurnResponse` whose `message.text()` is the concatenation, whose `tokens_used` is the
  stream's `total_tokens`, and whose `final_output` is `true`.
- **SC-002** Ollama's **measured** shape — two complete tool calls with distinct `index` values in one
  delta — yields two `ToolCall`s with the provider's own ids and parsed arguments, and
  `final_output == false`.
- **SC-003** The same two calls split across five deltas, with `arguments` in fragments and `name`
  split across two events, yields the **identical** `TurnResponse`.
- **SC-004** A stream carrying `reasoning` deltas yields a `message.text()` holding none of it.
- **SC-005** A stream with no `usage` event fails with `HeddleError::Model` naming the missing
  metering.
- **SC-006** The recorded `response` is string-equal to the exact SSE bytes the stub socket wrote,
  `streamed` is `true`, the recorded `request` contains `"stream":true` and
  `"stream_options":{"include_usage":true}`, and the run's `StepKind` sequence is what slice 023
  recorded.
- **SC-007** A quote-bearing secret inside a `data:` payload reaches no payload of the run in either
  its literal or its escaped form; the exchange carries `***`; the payload still deserializes;
  `verify_chain` passes; and the **provider was still sent the real secret**.
- **SC-008** A non-2xx turn records its exchange with `streamed == false`.
- **SC-009** An ACP client receives **one `AgentMessageChunk` per delta, in order**, with no duplicate
  of the whole answer, and a secret in a delta arrives as `***`.
- **SC-010** Driving the real `heddle acp-agent` binary against a provider that holds its socket
  open mid-stream, at least one `AgentMessageChunk` has arrived **while the `session/prompt` request
  is still outstanding**; releasing the provider then yields `StopReason::EndTurn` and the expected
  chain.
- **SC-011** Against a real local provider, a streamed turn accumulates a correct answer, the captured
  `response` begins `data:` and carries the provider's own `usage`, and a streamed **tool call**
  accumulates correctly — with every call the provider framed surviving into the `TurnResponse`.
- **SC-012** A 200 carrying an HTML page is refused naming that body; a stream of nothing but the
  metering event is refused as `no choices[0]`.

## The rejected-alternatives register

- **A `--stream` flag or a config key.** Rejected: it makes the worse behaviour the default and adds a
  knob with no requesting user (Constitution VII), and it doubles the wire surface permanently to
  preserve a path nothing would call. This is slice 023's D5 reasoning applied unchanged.
- **Widening `turn` to take a sink parameter.** Rejected: it rewrites every implementation and call
  site to thread an argument all but one ignores. Verbatim the alternative slice 023 rejected for
  `WireSink`.
- **Letting `NativeLoop` push chunks by observing the chain mid-run.** Rejected because it cannot
  work: the chain only learns the text after `turn` returns, which is precisely the latency being
  removed.
- **A callback taking `&mut Ledger`.** Rejected: `NativeLoop::run` borrows the ledger for its whole
  call, so this degenerates into interior mutability plus a runtime borrow panic — the same finding
  slice 023 recorded before landing on the pull/push handoff.
- **One Ledger step per SSE chunk.** Rejected and *measured*: 146 events for one short turn, for zero
  added fidelity.
- **Recording the reassembled `chat.completion`-shaped object instead of the raw SSE.** The tempting
  answer to the 48× size cost, and refused: the recorded bytes would be *our reconstruction*, so a bug
  in the accumulator would be invisible to the chain — the exact defect slice 023 existed to close.
- **A sibling `StepKind::StreamExchange`.** Rejected: two overlapping capture mechanisms for one
  concept. One kind, one payload, one flag-free capture.
- **Absorbing `delta.reasoning` into the message.** Rejected: it breaks parity with the non-streamed
  path, which never had the field.
- **Streaming `heddle chat`'s stdout.** Rejected deliberately rather than forgotten: `chat`'s contract
  is "stdout carries the assistant's answer and nothing else", so incremental printing produces the
  same final bytes for no requesting user — a second sink implementation Constitution VII refuses.
- **Buffering deltas to close the split-secret redaction gap.** Rejected: it reintroduces the exact
  latency the slice exists to remove. Recorded as a residual instead.

## Assumptions and residuals

- **Assumption — the reader's bound is restored, not inherited.** `Body::read_to_string` applied
  `MAX_BODY_SIZE = 10 MiB`; `Body::as_reader()` is documented "not limited by default". The stream
  path uses `with_config().limit(MAX_STREAM_BODY).reader()` with the same number, so the property
  slice 023 recorded as an accepted assumption still holds and is now stated in one named constant.
- **Assumption — the lossy step is ours rather than ureq's.** `read_to_string` applied
  `lossy_utf8(true)`, substituting `?`; the stream reader takes bytes and applies
  `String::from_utf8_lossy`, substituting U+FFFD. Both are lossy and neither errors, so slice 023's
  recorded assumption is preserved, but the substitute character differs.
- **Note — the empty-delta guard lives in the gateway, not in the sink.** One guard for one failure
  mode: the gateway is where the measurement lives and it protects every sink rather than the one
  that remembered to check.
- **Assumption — chain growth is roughly 48× for model I/O.** Measured, accepted deliberately
  (`plan.md` D5), bounded by the reader limit. If it becomes a real constraint it is a silo retention
  concern (design §7), not a per-run switch.
- **Residual — a secret split across two deltas can appear in the live ACP transcript.** Per-delta
  `Redactor::redact` cannot match a needle spanning a chunk boundary. **The chain is unaffected**: the
  accumulated body is scrubbed with `redact_wire` before it lands, and `project_updates` reads only
  redacted payloads. `permission.rs` already records the related acknowledgment that an out-of-process
  client's transcript is not governed by the Redactor. Best-effort per-delta redaction ships; closing
  it needs buffering, which is the latency this slice removes.
- **Residual — a compat layer that omits `index` collapses multiple tool calls into slot 0.** `index`
  is `#[serde(default)]`; all three measured Ollama models always send it, so the default is never
  exercised against the provider this slice targets. Named rather than guessed around.
- **Residual — mid-stream cancellation is now *possible* and is not implemented.** See *Out of scope*.
- **Backward compatibility is claimed; forward compatibility is not.** `WireExchange.streamed` is
  `#[serde(default)]` and `set_text_sink` is defaulted, so a pre-025 chain loads unchanged and no
  existing step's id moves. A chain written *by* this slice loads under pre-025 code too, except that
  its `response` payloads hold SSE text — readable, just differently framed. Same stance slice 023
  recorded.

## Out of scope

- **Mid-turn cancellation.** `CancellableModel` checks *before* a turn and its docstring already
  states "a model call already in flight completes". Streaming makes mid-stream abort possible; it
  does not make it requested.
- **Streaming `heddle chat`'s stdout.** Decided, not omitted — see the register.
- **Streaming tool calls to the ACP transcript as they arrive.** The provider delivers them whole in
  one delta, so there is nothing incremental to show.
- **Per-chunk Ledger steps, a `--stream` flag, a config key, sampling or retention policy for the new
  payload.**
- **HTTP headers, the request line, transport framing, provider authentication.** Slice 023's D6
  boundary, unchanged.
- **`--json` output, a config file, sampling parameters.** Separately named residuals on slice 013's
  list.
- **`heddle-connectors`, `heddle-sandbox`, `heddle-silo`, `heddle-mcp`.** No reason found to touch them.
- **`spikes/`** (ADR-0004 D2) — left byte-identical.
- **A PR.** No real remote; the bare mirror under `D:/claudecode/heddle-origin.git` exists only for
  Archon's worktree isolation.
