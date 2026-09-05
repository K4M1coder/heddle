# Plan — slice 025: SSE streaming from the local provider, with live ACP `AgentMessageChunk`s

**Target artifacts:** `specs/025-streaming-sse/{spec.md,plan.md,tasks.md}` plus the code changes
below. **Branch:** `025-streaming-sse`, cut from `dev`. **No PR** (the bare mirror at
`D:/claudecode/heddle-origin.git` exists only for Archon's worktree isolation). Conventional
Commits. Strict TDD (Constitution III): red before green.

---

## 0. Read this first — what the tree actually is, and what the live provider actually does

Everything below was verified this session: source via `git show origin/dev:<path>`, wire format by
driving the **real Ollama instance running on this machine** (`http://localhost:11434`).

### 0.1 The worktree is stale. Rebase before anything else.

| Claim | What was verified |
|---|---|
| "dev is green after slices 020–024" | True of **`dev`**, not of this worktree. `HEAD` here is `d364405`; `origin/dev` is `9002f73`, **21 commits ahead**. Slices 022, 023 and 024 are absent from the checkout: `specs/` here stops at `020`, `crates/heddle-gateway/src/lib.rs` here has no `WireExchange`, no `last_exchange` and no `tool_call_id`, and its `ChatRequest` still carries the comment *"streaming is out of scope for this slice."* |

**T0 of the implementation run: fast-forward `025-streaming-sse` onto `origin/dev` at `9002f73` and
re-measure the control baseline** (`cargo test --workspace`), exactly as slices 022 and 023 recorded
doing. Every anchor named in this plan is a **`dev`** anchor and does not exist in the current
checkout.

### 0.2 What Ollama's SSE actually looks like (measured, not assumed)

Driven against `http://localhost:11434/v1/chat/completions` with `"stream": true`, using
`lfm2.5:latest`, `gemma4:latest` and `qwen3.8:27b`.

Response headers: `Content-Type: text/event-stream`, `Transfer-Encoding: chunked`.

1. **The event separator is bare `\n\n`, not CRLF.** Confirmed with `cat -A`: every line ends `$`
   (LF), never `^M$`. Framing is `data: {json}\n\n`, repeated, terminated by `data: [DONE]\n\n`.
   There are no `event:`, `id:` or comment lines.
2. **There is no `usage` object at all by default.** The `finish_reason` chunk is the last one
   carrying a choice, and nothing after it meters the turn. Since `OpenAiCompatClient::metered`
   *refuses* a turn with no metering — deliberately, to keep Constitution VIII enforceable —
   **naively flipping `stream` to `true` breaks every run.**
3. **`"stream_options": {"include_usage": true}` fixes it, and Ollama supports it.** Verified: a
   final extra event arrives with `"choices":[]` and
   `"usage":{"prompt_tokens":14,…,"completion_tokens":47,"total_tokens":61}`, immediately before
   `data: [DONE]`. This request field is **mandatory** for this slice.
4. **Tool calls arrive WHOLE, never fragmented.** This contradicts the request's stated assumption.
   On all three models a single delta carried the complete array — ids, names and complete
   `arguments` JSON strings — e.g. two calls in one chunk:
   `"tool_calls":[{"id":"call_43lp106j","index":0,"type":"function","function":{"name":"fs_read","arguments":"{\"path\":\"/etc/hosts\"}"}},{"id":"call_jtd04izj","index":1,…}]`.
   Each call carries an explicit `index`. See D3 for what the accumulator does about this.
5. **A `reasoning` field appears in the delta, and `content` is `""` on those chunks.** Verified on
   `lfm2.5`. Critically, `reasoning` is **also present on the non-streamed response** and the current
   `ChoiceMessage` has no such field, so it is already silently ignored. The accumulator must ignore
   it too — see D4.
6. **An error under `"stream": true` is a plain non-SSE JSON body with a non-2xx status.** Verified:
   `HTTP/1.1 404`, `Content-Type: application/json`,
   `{"error":{"message":"model 'nope:latest' not found",…}}`. The existing status-check-then-error
   path therefore still works unchanged.
7. **`finish_reason` semantics are identical across modes.** Non-streamed tool-calling turn:
   `"finish_reason":"tool_calls"`, `"content":""`. Streamed: the same. Non-streamed plain answer:
   `"stop"`. Streamed: `"stop"`. So `final_output = finish_reason == "stop" && tool_calls.is_empty()`
   needs no change and produces the same verdict.
8. **Size, measured.** A single short turn ("Reply with only: hi", a reasoning model): **146 SSE
   events, 32,906 bytes**, against **685 bytes** for the equivalent non-streamed body — roughly
   **48×**. This number drives D5 and must not be glossed.

### 0.3 Anchors verified on `dev` (all exist; nothing below is assumed)

- `crates/heddle-core/src/model.rs`: `TurnRequest`, `TurnResponse`, `WireExchange { url, status,
  request, response }`, `trait ModelClient { fn turn; fn take_wire_exchange() -> Option<WireExchange> }`
  (the latter **defaulted to `None`**).
- `crates/heddle-core/src/ledger.rs`: `StepKind::WireExchange` sits between `LlmRequest` and
  `LlmResponse`.
- `crates/heddle-core/src/native_loop.rs`: `NativeLoop::run` calls `self.client.turn(&req)`, then
  `self.client.take_wire_exchange()`, scrubs field-by-field with `redact` for `url` and
  `redact_wire` for both bodies, appends `StepKind::WireExchange`, **then** `let resp = resp?`.
- `crates/heddle-core/src/tool.rs`: `Redactor::redact`, `redact_json`, `redact_wire` (matches each
  secret literally **and** in `serde_json`-escaped form), `redact_call`.
- `crates/heddle-gateway/src/lib.rs`: `OpenAiCompatClient { endpoint, model, agent, last_exchange }`,
  `fn post(&self, url, body) -> Result<(u16, String)>`, `fn metered(&self, Option<Usage>) -> Result<u64>`,
  `fn unrecognised`, `fn truncated`, `struct ChatRequest { model, messages, stream, tools }`,
  `ChatResponse`/`Choice`/`ChoiceMessage`/`ResponseToolCall`/`ToolFunction`/`Usage`.
- `crates/heddle-acp/src/lib.rs`: `SessionParts`, `HeddleSession { id, engine, ledger, budget,
  prompts, cancelled }`, `HeddleSession::new(id, parts, connection)`, `HeddleSession::run`,
  `pub fn project_updates(ledger, run_id) -> Vec<SessionUpdate>` (its `StepKind::LlmResponse` arm
  emits exactly one `AgentMessageChunk`), and the `PromptRequest` handler's `std::thread::spawn`
  that loops `for update in project_updates(session.ledger(), &run_id)` and calls
  `cx.send_notification(SessionNotification::new(...))`.
- `crates/heddle-acp/src/cancel.rs`: `CancellableModel` forwards `take_wire_exchange` with a recorded
  reason ("inheriting the default would drop the exchange without erroring").
- `crates/heddle-acp/src/permission.rs`: `AcpPermissionTransport { inner, connection:
  ConnectionTo<Client>, session_id }` — proof that a `ConnectionTo<Client>` already lives inside the
  session and is already `Send` across the loop worker thread.
- Tests: `crates/heddle-gateway/tests/{governed_run.rs,openai_compat.rs}`,
  `crates/heddle-acp/tests/acp_session.rs` (its `ScriptedModel` already has a `gate:
  Option<Receiver<()>>` field — the blocking-turn precedent this slice reuses), and
  `crates/heddle-cli/tests/{cli_chat.rs,cli_acp_agent.rs}` (each with its own `StubProvider` +
  `reply(...)` helper).
- `ureq` 3.4.0: `Body::read_to_string` applies `MAX_BODY_SIZE = 10 MiB` and `lossy_utf8(true)`;
  `Body::as_reader()` is documented **"not limited by default"**.
  `Body::with_config().limit(n).reader()` is the bounded form.
- Residual wording confirmed verbatim in `specs/013-acp-agent/tasks.md`: *"streaming (SSE), together
  with incremental ACP `AgentMessageChunk` notifications. Today a client sees one chunk per turn,
  after the turn."*

---

## 1. Problem

`OpenAiCompatClient::turn` sends `"stream": false` and blocks until one complete HTTP body has
arrived. `heddle-acp`'s `project_updates` then derives exactly one `AgentMessageChunk` per
`StepKind::LlmResponse` step, and the `PromptRequest` handler sends every one of them *after* the
whole run finishes. An editor driving `heddle acp-agent` therefore sees nothing at all for the entire
duration of a model turn — many seconds for a long answer — and then the whole answer at once. ACP
is the one product surface designed for live human interaction, so this is a real usability defect
on the surface least able to absorb it.

## 2. Approach

Six decisions. Each names the strongest alternative and why it lost.

### D1 — Streaming is unconditional. There is no `--stream` flag, and the non-streaming request path is deleted.

`ChatRequest.stream` becomes `true` permanently and gains `stream_options: {"include_usage": true}`.
`ChatResponse`/`Choice`/`ChoiceMessage` are **deleted** and replaced by the chunk types; there is one
wire path, not two.

*Why:* slice 023's D5 reasoning applies directly — a flag makes the worse behaviour the default and
adds a knob with no requesting user (Constitution VII). Two paths means two shapes to test forever,
and the non-streamed one would have no caller.

*Rejected — keep both behind a flag or a config key.* It doubles the wire surface permanently to
preserve a path nothing asks for. Rejected for VII.

*Cost, stated honestly:* every stub provider in the suite currently serves a non-SSE JSON body and
must be re-framed. That is **four files**, but each funnels its bodies through one `reply(...)`
helper and one HTTP-writing `format!`, so the change is small and mechanical:
`crates/heddle-gateway/tests/governed_run.rs`, `crates/heddle-gateway/tests/openai_compat.rs`,
`crates/heddle-cli/tests/cli_chat.rs`, `crates/heddle-cli/tests/cli_acp_agent.rs` (whose
`tool_call_reply` helper needs the same treatment).

### D2 — Live text reaches ACP through a new defaulted port method, `ModelClient::set_text_sink`.

In `heddle-core/src/model.rs`:

```rust
/// Where a client pushes assistant text as the provider produces it, for a
/// caller that must show it before the turn ends.
pub trait TextSink: Send {
    fn on_text(&mut self, delta: &str);
}
```

`Send` for exactly the reason `LedgerStore: Send` carries in `ledger.rs` — it crosses to
`heddle-acp`'s prompt worker thread. And on the port:

```rust
fn set_text_sink(&mut self, _sink: Box<dyn TextSink>) {}
```

Defaulted to dropping the sink, which — as with `take_wire_exchange`'s `None` — is the *true* answer
rather than a convenience: a client that produces its text atomically has nothing to push before the
turn ends, and its caller still receives that text through `TurnResponse` and `project_updates` as
it always did. Every existing `ModelClient` implementation compiles and behaves identically, so **no
`StepKind` sequence asserted by any stub-model test changes.**

`CancellableModel` **must forward it**, with the reason its `take_wire_exchange` override already
records: inheriting the default would drop the sink and silently disable streaming in the one crate
that needs it.

`HeddleSession::new` installs the sink — it is the one place that holds both the
`ConnectionTo<Client>` and the `SessionId`, and it installs before wrapping the client in
`CancellableModel`, so a session cannot be constructed without one. `AcpPermissionTransport` already
stores a `ConnectionTo<Client>` inside the session, which is the existing proof that doing so is
`Send`-legal.

*Rejected — widen `turn` to take a sink parameter.* Rewrites every implementation and call site to
thread an argument all but one ignores; this is verbatim the alternative slice 023 rejected for
`WireSink`, and the same reasoning holds.

*Rejected — let `NativeLoop` push chunks by observing the chain mid-run.* The chain only learns the
text after `turn` returns, which is precisely the latency being removed. It cannot work.

*Rejected — a callback taking `&mut Ledger`.* `NativeLoop::run` borrows the ledger for its whole
call; slice 023 already worked this through and landed on the pull/push handoff rather than interior
mutability.

### D3 — The accumulator keys tool calls by `index` and concatenates, even though Ollama never fragments.

```rust
#[derive(Default)]
struct Accumulated {
    content: String,
    calls: BTreeMap<u64, PartialCall>,   // keyed by delta index; BTreeMap so order is the wire's
    finish_reason: Option<String>,
    usage: Option<Usage>,
}
struct PartialCall { id: String, name: String, arguments: String }
```

Every `delta.tool_calls[i]` does `entry(tc.index).or_default()` then `push_str` for whichever of
`id` / `function.name` / `function.arguments` is present.

*Why concatenate when §0.2(4) proves Ollama sends them whole?* Because concatenating strings that
arrive whole is a no-op — the code is the same three lines either way — and because
`heddle-cli/src/wiring.rs` documents **a LiteLLM sidecar as a supported deployment** ("a LiteLLM
sidecar is a different `--base-url` and no code change"). LiteLLM proxying a real OpenAI or Anthropic
model *does* fragment `arguments` across chunks. This is therefore not speculative generality; it is
the documented deployment's actual behaviour, and the whole-arrival case is the degenerate case of
the same code.

*This corrects the request's premise* and changes what must be tested: the acceptance criterion said
"if Ollama's real behavior does this". It does not. So **both shapes get a stub test** — one
asserting the real Ollama whole-call shape (the one that must work against the live provider), one
asserting the fragmented shape (the one the `index` keying exists for).

Two smaller accumulator rules, each with a verified reason:

- **`index` is `#[serde(default)]`.** All three models always send it, so the default is never
  exercised against Ollama; a compat layer that omits it would collapse multiple calls into slot 0,
  which is named as a residual rather than guessed around.
- **Accumulated `arguments == ""` is treated as `"{}"`.** Non-streamed, a no-argument call arrives as
  `"arguments":"{}"` and parses. Streamed, a call with no argument deltas accumulates to `""`, which
  `serde_json::from_str` rejects. Without this, streaming would introduce a failure the non-streamed
  path did not have — a direct violation of the Principle VIII parity invariant.

### D4 — `delta.reasoning` is ignored, and empty deltas are never emitted.

The accumulator absorbs `delta.content` only. `reasoning` is dropped, because the **non-streamed**
`ChoiceMessage` has no `reasoning` field and already discards it (verified: Ollama returns
`reasoning` in both modes). Absorbing it would put text into `TurnResponse.message` that the
non-streamed path never had — breaking the parity invariant — and would show an ACP client a
chain-of-thought that appears nowhere in the Ledger or the final answer.

Correspondingly, `AcpTextSink::on_text` **returns early on an empty delta**. This is not
defensiveness: §0.2(5) shows Ollama sends `"content":""` on every reasoning chunk, so without the
guard a single turn floods the editor transcript with ~150 empty `AgentMessageChunk`s.

### D5 — The Ledger reuses `WireExchange`, records the raw SSE verbatim, and gains one `streamed: bool` field. One step per exchange, not one per chunk.

```rust
pub struct WireExchange {
    pub url: String,
    pub status: u16,
    pub request: String,
    pub response: String,
    /// Whether `response` holds an SSE event stream rather than one body.
    #[serde(default)]
    pub streamed: bool,
}
```

`response` holds **the literal bytes read off the socket** — every `data:` line and every blank line,
verbatim, in order — preserving slice 023's FR-002/FR-003/SC-001 claim unchanged. The bytes are
accumulated into `raw` as they are read, so a mid-stream failure still records what arrived, which is
strictly better fidelity than 023 had.

`streamed` has a real code reader, not just a test: on the **non-2xx path the provider does not send
a stream at all** (§0.2(6) — a plain JSON error body), so the same struct legitimately carries
`streamed: false` for a failed turn and `true` for a successful one. That is exactly the role
`status` plays in 023 — "the one wire fact neither body carries" — and it means an auditor reading
`data: {…}` framing in `response` knows it is intended rather than corruption. It is additive and
`#[serde(default)]`, so a pre-025 chain still deserializes and no existing step's id moves.

*Rejected — one Ledger step per SSE chunk.* Measured, not hypothesized: §0.2(8) recorded **146 events
for one short turn**. A multi-iteration run would put thousands of steps on one chain, inflating
`verify_chain`'s cost and the `log`/`show` surface, for **zero added fidelity** — the concatenation of
the chunks *is* the stream. Constitution VII refuses the machinery.

*Rejected — record the reassembled `chat.completion`-shaped object instead of the raw SSE.* This is
the tempting answer to the 48× size cost (§0.2(8): 32,906 bytes vs 685). It is refused because it
reintroduces the exact defect slice 023 existed to close: the recorded bytes would be *our
reconstruction*, so a bug in the accumulator would be invisible to the chain. Under streaming the
translation is **more** complex than it was — the accumulator is the single most error-prone thing
this slice adds — so it is more worth witnessing, not less. The size cost is accepted deliberately,
bounded by the reader limit in D6, and retention stays a silo concern exactly as slice 023 recorded
for its own (smaller) growth.

*Rejected — a sibling `StepKind::StreamExchange`.* Two overlapping capture mechanisms for one
concept, which the brief's Principle V invariant explicitly warns against. One kind, one payload, one
flag-free capture.

**Redaction is unchanged and already correct.** `NativeLoop::run`'s scrub block keeps `redact_wire`
for `response`: each SSE `data:` payload is serialized JSON, so a quote-bearing secret is on it in
escaped form — the exact premise `redact_wire` was written for in slice 023, and it matches both the
literal and the escaped needle. The only edit is carrying `streamed` through the reconstructed
struct.

### D6 — The stream reader is byte-oriented, lossy-UTF-8, and explicitly bounded.

`Body::as_reader()` is **unlimited by default**, so switching from `read_to_string` would silently
drop the 10 MiB cap that governs the current parse. Use
`response.body_mut().with_config().limit(MAX_STREAM_BODY).reader()` with a `MAX_STREAM_BODY` const of
`10 * 1024 * 1024` in `heddle-gateway`, matching ureq's own `MAX_BODY_SIZE`.

Read with `BufRead::read_until(b'\n', &mut Vec<u8>)` and `String::from_utf8_lossy`, **not**
`read_line`/`lines()`:

- `read_until` **keeps the terminator**, so `raw` is byte-identical to the wire even if some other
  provider frames with CRLF. `lines()` would strip it and quietly forge the capture.
- `from_utf8_lossy` preserves the `lossy_utf8(true)` behaviour `read_to_string` applied and slice
  023's spec explicitly recorded as an accepted assumption. `read_line` would *error* on a non-UTF-8
  byte, a behaviour change with no justification.

Parse each line by trimming `\r`/`\n`, taking `strip_prefix("data:")` then one optional leading space
(per SSE), breaking on `[DONE]`, and **ignoring any other line** (blank separators, and
`event:`/`id:`/comment lines a different provider might send).

---

## 3. Steps

Ordered; each independently verifiable. Anchors are named functions and types, never line numbers.

**S0 — rebase and baseline.** Fast-forward `025-streaming-sse` onto `origin/dev` at `9002f73`. Run
`cargo test --workspace` and record the green count. Write
`specs/025-streaming-sse/{spec.md,plan.md,tasks.md}` mirroring slice 023's shape: a `## Constitution
Check (ADR-0004 D1 solo-v0 bar)` bullet list with one entry per principle plus `Cross-platform`, a
`## Tasks` checklist, an `## Observed red` section, a rejected-alternatives register in `spec.md`,
and `## Assumptions and residuals` / `## Out of scope` sections.

**S1 — RED: the accumulator, against Ollama's real shape.** In
`crates/heddle-gateway/tests/openai_compat.rs`, re-frame `StubProvider` to serve
`Content-Type: text/event-stream` and add an `sse(events: Vec<&str>) -> String` helper producing
`data: {json}\n\n…data: [DONE]\n\n`. Add a test asserting that a stream of several `content` deltas
plus one `finish_reason:"stop"` chunk plus one `choices:[], usage:{...}` chunk accumulates to a
`TurnResponse` whose `message.text()` is the concatenation, whose `tokens_used` is `total_tokens`,
and whose `final_output` is `true`. **Compile/assert red.**

**S2 — RED: tool calls, both shapes.** Two sibling tests in the same file. (a) *Ollama's real shape*:
one delta carrying two complete `tool_calls` with distinct `index` values and complete `arguments`,
`finish_reason:"tool_calls"` — asserts two `ToolCall`s with the provider's ids, parsed args, and
`final_output == false`. (b) *The fragmented shape*: the same two calls split across five deltas,
with `arguments` arriving in fragments and `name` split across two chunks — asserts the identical
`TurnResponse`. Plus a third asserting `delta.reasoning` never reaches `message.text()`, and a fourth
asserting a stream **without** a usage chunk yields `Err(HeddleError::Model(_))` naming the missing
metering (the Principle VIII guard).

**S3 — RED: the wire capture.** In `crates/heddle-gateway/tests/governed_run.rs`, re-frame its
`StubProvider`/`reply` to SSE and extend `the_chain_records_the_literal_bytes_of_the_exchange` (and
add a sibling) to assert: the recorded `response` is **string-equal to the exact SSE bytes the stub
wrote**, `streamed == true`, the recorded `request` contains `"stream":true` and
`"stream_options":{"include_usage":true}`, and the run's `StepKind` sequence is unchanged from slice
023's. Add a redaction test with a **quote-bearing** secret placed inside a `data:` payload, asserting
`***` in the chain, no literal and no escaped form anywhere in any payload, that the payload still
deserializes, that `verify_chain` passes, and — the control — that the stub still received the real
secret. Add a non-2xx test asserting the exchange is still recorded with `streamed == false`.

**S4 — GREEN: `heddle-core`.** Add `TextSink`, the defaulted `ModelClient::set_text_sink`, and
`WireExchange.streamed` (`#[serde(default)]`) in `model.rs`; export `TextSink` from `lib.rs`. In
`native_loop.rs`, carry `streamed: exchange.streamed` through the scrub block — the only change
there.

**S5 — GREEN: `heddle-gateway`.** Add `sink: Option<Box<dyn TextSink>>` to `OpenAiCompatClient` and
implement `set_text_sink`. Add `stream_options` to `ChatRequest` (immediately after `stream`, before
the skipped `tools`, because field order is wire order and the tests assert bytes). Delete
`ChatResponse`/`Choice`/`ChoiceMessage`/`ResponseToolCall`; add
`ChatChunk`/`ChunkChoice`/`Delta`/`DeltaToolCall`/`DeltaFunction`. Replace `post` with a
send-and-stream path per D6, building `raw` and `Accumulated` together and pushing each non-empty
`content` delta to the sink as it is absorbed. Store the `WireExchange` (moving `body` in, preserving
023's single-buffer property) **before** any error propagates. Keep `metered`, `unrecognised`,
`truncated` and the `final_output` expression byte-for-byte as they are.

**S6 — RED: the ACP transcript.** In `crates/heddle-acp/tests/acp_session.rs`, give `ScriptedModel` a
`deltas: Vec<String>` and a `sink: Option<Box<dyn TextSink>>` field plus a `set_text_sink` override,
so `turn` pushes each delta before returning its scripted `TurnResponse`. Add a test asserting
`Observed::chunks()` (the existing helper) holds **one entry per delta, in order, and no duplicate of
the whole answer** — i.e. `chunks().len() > 1` and the concatenation equals the final message. Add a
redaction test proving a secret in a delta reaches the ACP client as `***`.

**S7 — GREEN: `heddle-acp`.** New `src/stream.rs` with `AcpTextSink { connection, session_id,
redactor, emitted: Arc<AtomicU64> }`; `on_text` returns early on an empty delta (D4), applies
`Redactor::redact` (plain text, not escaped JSON — so `redact`, not `redact_wire`), increments
`emitted`, and sends the `SessionNotification`. `CancellableModel` forwards `set_text_sink`.
`HeddleSession` gains `streamed: Arc<AtomicU64>` beside its existing `cancelled: Arc<AtomicBool>`,
built and installed in `new`; `run` resets it to `0` on the same line that resets `cancelled`; a
`pub fn streamed(&self) -> bool` reports `> 0`.

**S8 — GREEN: suppress the duplicate.** In the `PromptRequest` handler's spawned thread, skip
already-delivered text:

```rust
let streamed = session.streamed();
for update in project_updates(session.ledger(), &run_id) {
    if streamed && matches!(update, SessionUpdate::AgentMessageChunk(_)) { continue; }
    …
}
```

**`project_updates` itself is deliberately not changed.** It keeps its exact meaning — the complete
chain-derived transcript — so `u1_project_updates_maps_each_ledger_step_kind`,
`a1_one_acp_session_drives_one_governed_turn_end_to_end` and every other slice-008/013/018 test stay
green, and a client whose model does not stream sees precisely what it saw before. The filter is four
lines at the one call site, and it is the *only* place the two paths could collide.

**S9 — GREEN: re-frame the CLI stubs.** `crates/heddle-cli/tests/cli_chat.rs` and `cli_acp_agent.rs`:
`StubProvider` writes `content-type: text/event-stream`, and `reply(...)` / `tool_call_reply(...)`
emit SSE framing including the trailing usage event and `[DONE]`. Mechanical; no assertion about
chain shape or exit codes changes.

**S10 — RED then GREEN: the end-to-end live-delivery proof.** In `cli_acp_agent.rs`, a test in which
the `StubProvider` writes the first few SSE events, then **blocks on an `mpsc::Receiver` gate** before
writing the rest (the gate pattern `ScriptedModel` already uses in `acp_session.rs`). The real ACP
client counts `AgentMessageChunk` notifications; the test asserts at least one arrived **while the
`session/prompt` request was still outstanding**, then releases the gate and asserts the final
`StopReason::EndTurn` and the chain's `StepKind` sequence. This is the test that proves "during, not
after" rather than merely "more than one".

**S11 — the live provider tests.** Two `#[ignore]`d tests gated on `HEDDLE_LIVE_MODEL`, following the
exact convention of `governed_fs_run.rs`
(`#[ignore = "needs a real tool-capable local provider; set HEDDLE_LIVE_MODEL to run"]`, with an
`eprintln!` skip when the variable is unset): one asserting a real streamed turn accumulates a correct
answer and that the captured `WireExchange.response` begins `data:` and carries the provider's own
`usage`; one asserting a real streamed **tool call** accumulates correctly.

**S12 — close out.** Record the observed reds verbatim, the live verification, and the residuals in
`tasks.md`, per slices 019–024's house practice.

---

## 4. Validation

Project gates: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`,
`cargo test --workspace` — all green, with the pre-change baseline from S0 as the control.

New tests, all behaviour-proving:

| Test | Proves |
|---|---|
| S1 accumulation | Multi-delta text → correct `TurnResponse` (`message`, `tokens_used`, `final_output`) |
| S2(a) whole tool call | Ollama's **real** measured shape accumulates to two correct `ToolCall`s |
| S2(b) fragmented tool call | The `index`-keyed accumulator handles the LiteLLM shape identically |
| S2 reasoning | `delta.reasoning` never reaches `message.text()` — the parity invariant |
| S2 no-usage | A stream without metering **fails loudly** — Constitution VIII guard |
| S3 literal bytes | `WireExchange.response` is string-equal to the SSE the socket wrote; `streamed == true` |
| S3 redaction | A quote-bearing secret in a `data:` payload is `***` on the chain in both forms; `verify_chain` passes; the provider still got the real value |
| S3 non-2xx | The exchange is recorded with `streamed == false` |
| S6 ACP transcript | **Multiple** ordered `AgentMessageChunk`s per turn, no duplicated full answer |
| S6 ACP redaction | A secret in a delta reaches the client as `***` |
| S10 end-to-end | A chunk arrives **while `session/prompt` is still outstanding** — the actual UX claim |
| S11 live ×2 | Real Ollama: streamed text and a streamed tool call both accumulate correctly |

**Hand-verification after implementation** (per the acceptance criteria): drive a real
`heddle acp-agent` against a live Ollama model using slice 018's `cli_acp_agent.rs` harness pattern (a
real `AcpAgent` spawning the real binary), observe multiple notifications arriving during one turn,
then confirm the final answer and inspect the chain with `heddle ledger log` / `heddle ledger show`.
Record the transcript in `tasks.md` under *Live verification*, as slices 019, 020 and 024 each did.

## 5. Risks and rollback

- **Blast radius.** `heddle-core` (two additive items + one field carried through), `heddle-gateway`
  (the response path rewritten), `heddle-acp` (one new file, three small edits), and four test files
  re-framed to SSE. `heddle-connectors`, `heddle-sandbox`, `heddle-silo` and `heddle-mcp` are untouched.
  No new dependency, no `Cargo.toml` change, no `#[cfg]`, no CLI flag.
- **Highest risk — the 10 MiB cap.** `as_reader()` is unlimited; forgetting `.limit()` turns a
  malicious or looping provider into unbounded memory growth. D6 mandates it; a reviewer should check
  that one line first.
- **Second risk — a provider that ignores `stream_options`.** It yields no usage, `metered` refuses,
  and the run fails loudly with a message naming the missing metering. This is the correct failure
  mode and is exactly the guard `metered` was written to be; it is **not** silent.
- **Third risk — chain growth.** Measured at ~48× for model I/O (§0.2(8)). Accepted per D5, bounded
  by the reader limit. If it becomes a real constraint it is a silo retention concern (design §7),
  not a per-run switch.
- **Known gap, named not hidden — a secret split across two deltas.** Per-delta `Redactor::redact`
  cannot match a needle spanning a chunk boundary, so a split secret could appear in the **live ACP
  transcript**. The **chain is unaffected**: the accumulated body is scrubbed with `redact_wire`
  before it lands, and `project_updates` reads only redacted payloads. `permission.rs` already
  records the related acknowledgment that "an out-of-process client's transcript is not governed by
  the Redactor". Ship best-effort per-delta redaction and record this as an explicit residual;
  buffering deltas to close it would reintroduce the latency the slice exists to remove.
- **Rollback.** Revert the branch. `WireExchange.streamed` is `#[serde(default)]` and `set_text_sink`
  is defaulted, so a chain written by this slice still loads under pre-025 code **except** that its
  `response` payloads hold SSE text — readable, just differently framed. Forward compatibility is not
  claimed, matching slice 023's recorded stance.

## 6. Out of scope

- **Mid-turn cancellation.** `CancellableModel` checks *before* a turn and its docstring already
  states "a model call already in flight completes". Streaming makes mid-stream abort *possible*; it
  does not make it requested. Separate slice.
- **Streaming `heddle chat`'s stdout.** Decided plainly and deliberately: the client streams, `chat`
  installs **no sink**, and its output is byte-identical to today. `chat`'s documented contract is
  "stdout carries the assistant's answer and nothing else"; incremental printing would produce the
  same final bytes for no requesting user, so it is a second sink implementation Constitution VII
  refuses. Recorded in `spec.md`, not left implicit.
- **Streaming tool calls to the ACP transcript as they arrive.** §0.2(4) proves Ollama delivers them
  whole in one delta, so there is nothing incremental to show.
- **Per-chunk Ledger steps, a `--stream` flag, a config key, sampling or retention policy for the new
  payload.** D1 and D5.
- **HTTP headers, the request line, transport framing, provider authentication.** Slice 023's D6
  boundary, unchanged.
- **`--json` output, a config file, sampling parameters.** Separately named residuals on slice 013's
  list.
- **`heddle-connectors`, `heddle-sandbox`, `heddle-silo`.** No reason found to touch them.
- **`spikes/`** (ADR-0004 D2) — left byte-identical.
- **A PR.**
