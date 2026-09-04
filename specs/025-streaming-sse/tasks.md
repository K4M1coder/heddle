# Tasks: SSE streaming from the local provider, with live ACP `AgentMessageChunk`s (v0 slice)

**Spec:** `specs/025-streaming-sse/spec.md` · **Plan:** `specs/025-streaming-sse/plan.md` ·
TDD (red→green), branch `025-streaming-sse`, fast-forwarded onto `dev` at `9002f73`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)

- **I Headless core** ✅ no CLI of its own, no new flag, no new command, no new argument. `skein chat`
  is byte-identical: the client streams, `chat` installs no sink, and its documented contract —
  "stdout carries the assistant's answer and nothing else" — is unchanged. `skein ledger log` and
  `show` render the new payload with zero CLI change, verified against a live chain (see *Live
  verification*).
- **II Local-first** ✅ no new dependency, no `Cargo.toml` change, no second network call. `ureq` stays
  declared with `default-features = false`, so no TLS backend and no content-encoding decode; the
  slice reads the one HTTP response that already existed, differently.
- **III Test-First** ✅ five reds observed and recorded verbatim below. Red A is a compile absence,
  red B a behavioural one across 16 tests; reds C, D1 and D2 each isolate one decision by reverting
  exactly it from the finished implementation. Recorded honestly: `a12` **passed** as first written
  and was strengthened until it could not — see *Deviations* 4.
- **IV Inverted coupling** ✅ `skein-core` gains `TextSink` — one method taking a `&str` — and a
  defaulted `set_text_sink`. Neither names SSE, HTTP, a delta or a provider. Every fact about the
  chat-completions event format stays in `skein-gateway`, which remains the one crate that names it,
  and every fact about ACP stays in `skein-acp`. The port grows a defaulted method, so no other crate
  reasons about a client that streams.
- **V Traceability** ✅ `WireExchange.response` holds the literal event stream — every `data:` line and
  every blank one, in wire order — rather than the object reassembled from it, which preserves slice
  023's claim exactly. Under streaming it is worth *more*: the accumulator is the most error-prone
  thing this slice adds, so recording our reconstruction would hide precisely the bug worth
  witnessing. One step per exchange, not one per chunk (measured: 146 events for one short turn).
  `project_updates` is unchanged, so the ACP transcript is still a view of the chain and not a second
  record; the live chunks are the same text arriving earlier, and the run's `StepKind` sequence is
  identical to slice 023's.
- **VI Security** ✅ Two halves. **Chain:** unchanged and re-proved — each `data:` payload is
  serialized JSON, so `redact_wire`'s premise still holds, and a new test puts a quote-bearing secret
  inside a `data:` payload and asserts it reaches no payload in either form, that the events still
  parse, that `verify_chain` passes, and that the provider was still sent the real value.
  **Transcript:** the live path is a second way out of the process and gets its own per-delta
  `Redactor::redact`, asserted per delta rather than over the concatenation. The one gap — a secret
  spanning two deltas — is named as a residual rather than hidden, and does not touch the chain.
- **VII Neutrality** ✅ one defaulted trait method, one trait, one struct field, one new ACP file, one
  request field. No flag, no config key, no crate, no dependency, no `StepKind`. Ten alternatives are
  rejected with a reason each in `spec.md`'s register, three of them refused on *measurements* rather
  than taste (per-chunk steps, recording the reassembled object, absorbing `reasoning`). The one
  guard that could have been written twice is written once — see *Deviations* 2.
- **VIII Loop discipline** ✅ NON-NEGOTIABLE, and this slice tightens rather than touches it. The
  controller, budget, probe and exit conditions are neither read nor written. `metered` is unchanged
  and is now **load-bearing**: the provider sends no `usage` at all under a bare `stream: true`, so
  `stream_options.include_usage` is mandatory and a provider ignoring it fails loudly with the
  refusal `metered` was written to be. A test asserts exactly that. `final_output` is the same
  expression and was verified to produce the same verdict in both modes.
- **Cross-platform** ✅ no `#[cfg]`, no platform API, no filesystem or process work. The stubs are
  `std::net::TcpListener`s on loopback, as they already were. The three `#[ignore]`d live tests are
  gated on an environment variable, as slices 019–024 established.

## Tasks

- [x] **S0** fast-forwarded onto `origin/dev` at `9002f73`, control baseline measured, and
      `specs/025-streaming-sse/{spec.md,plan.md,tasks.md}` written
- [x] **S1** RED — the accumulator against Ollama's real shape: multi-delta text, `usage` event
- [x] **S2** RED — tool calls in both shapes, `reasoning` discarded, a stream with no metering refused
- [x] **S3** RED — the wire capture: literal SSE bytes, `streamed`, the request fields, redaction of a
      quote-bearing secret inside a `data:` payload, and the non-2xx exchange
- [x] **S4** GREEN — `skein-core`: `TextSink`, the defaulted `set_text_sink`, `WireExchange.streamed`,
      the re-export, and the one carried field in `native_loop.rs`
- [x] **S5** GREEN — `skein-gateway`: `stream_options`, the chunk types, the bounded byte-oriented
      reader, the accumulator, and the sink push
- [x] **S6** RED — the ACP transcript: one chunk per delta in order, no duplicate, and a redacted
      secret
- [x] **S7** GREEN — `skein-acp`: `stream.rs`'s `AcpTextSink`, the `CancellableModel` forward, and the
      session's install and counter
- [x] **S8** GREEN — suppress the duplicate at the one `project_updates` call site
- [x] **S9** GREEN — re-frame the stub providers to SSE — **six files, not the four the plan named**
      (see *Deviations* 3)
- [x] **S10** RED then GREEN — the end-to-end proof that a chunk arrives *while `session/prompt` is
      still outstanding*
- [x] **S11** the live provider tests — **part of this run**, run against the real Ollama
- [x] **S12** close-out

## Control baseline (S0)

Measured on this worktree immediately after the fast-forward to `9002f73`, before any edit:

| gate | result |
|---|---|
| `cargo test --workspace` | **258 passed, 0 failed, 7 ignored** |

At close, on the same worktree with the slice applied: `cargo fmt --all --check` pass, `cargo clippy
--workspace --all-targets -- -D warnings` pass, **268 passed, 0 failed, 8 ignored**. The delta is
**+10 passed** and **+1 ignored**. Eleven tests were added — seven in
`skein-gateway/tests/openai_compat.rs` (one of them the `#[ignore]`d live tool-call test), one in
`skein-gateway/tests/governed_run.rs`, two in `skein-acp/tests/acp_session.rs` and one in
`skein-cli/tests/cli_acp_agent.rs` — so ten of them run. No test was deleted, renamed or disabled; nothing moved from passed to
ignored.

## Verified before trusting: the plan's §0.2 measurements, re-driven

Re-run against the live Ollama on this machine before any of them was relied on, because the plan's
own instruction was to stop rather than adapt if they no longer held.

| plan claim | re-measured | verdict |
|---|---|---|
| §0.2(1) separator is bare `\n\n`, `Content-Type: text/event-stream`, `Transfer-Encoding: chunked` | `cat -A` shows every line ending `$`, never `^M$`; headers exactly as stated | **holds** |
| §0.2(2) no `usage` object at all under a bare `"stream": true` | `grep -c '"usage"'` over a whole `gemma4:latest` stream → `0` | **holds** |
| §0.2(3) `stream_options.include_usage` produces a final `"choices":[]` event with `usage`, before `[DONE]` | exactly that, carrying `total_tokens: 23` | **holds** |
| §0.2(4) tool calls arrive **whole**, each with an explicit `index` | holds — every call complete within its event, every one carrying `index` | **holds** |
| §0.2(4) *"a single delta carried the complete array"* | **contradicted.** `qwen3.8:27b` sent two complete calls in **two separate events**, `index` 0 and `index` 1, each with `"finish_reason":null` | **corrected — see *Deviations* 1** |
| §0.2(6) a non-2xx under `stream: true` is a plain JSON body | the existing status-check path is untouched and its test still passes unchanged | **holds** |

## Observed red

Five reds. A and B are the absence of the feature; C, D1 and D2 each isolate one decision by
reverting exactly it from the finished implementation, so each is evidence about *this* code rather
than about a sketch of it.

### Red A — S3, the capture field does not exist

Tests applied, sources at `9002f73`. `cargo test -p skein-gateway`:

```
error[E0609]: no field `streamed` on type `skein_core::WireExchange`
   --> crates\skein-gateway\tests\governed_run.rs:368:22
error[E0609]: no field `streamed` on type `skein_core::WireExchange`
   --> crates\skein-gateway\tests\governed_run.rs:528:22
error[E0609]: no field `streamed` on type `skein_core::WireExchange`
   --> crates\skein-gateway\tests\governed_run.rs:625:19
error: could not compile `skein-gateway` (test "governed_run") due to 3 previous errors
```

### Red B — S1/S2, the client cannot read a stream

`openai_compat` compiles without the feature, so this red is behavioural rather than structural,
which is the more informative of the two: it shows the *semantics* missing, not just an API.
`cargo test -p skein-gateway --test openai_compat`:

```
test result: FAILED. 6 passed; 16 failed; 2 ignored; 0 measured; 0 filtered out
```

Every one of the sixteen failed the same way — the client feeding a whole SSE body to
`serde_json::from_str`:

```
the stub answers: Model("http://127.0.0.1:55774/v1 returned an unrecognised chat-completions
response: expected value at line 1 column 1: data: {\"choices\":[{\"delta\":{\"content\":\"42 is
the answer.\",\"role\":\"assistant\"},\"index\":0}],…}\n\ndata: [DONE]\n\n")
```

### Red C — S6, the transcript is one lump after the turn

`cargo test -p skein-acp --test acp_session`, with the full gateway implementation already green:

```
  left: ["The answer is 42."]
 right: ["The ", "answer ", "is ", "42."]

  left: ["your key *** is fine"]
 right: ["your key ", "***", " is fine"]

test result: FAILED. 16 passed; 2 failed; 0 ignored
```

The left column *is* the behaviour being replaced: the whole answer, once, after the run.

### Red D1 — S10, without the sink install: no chunk, and therefore no answer

`SkeinSession::new`'s `client.set_text_sink(…)` removed, everything else present. The provider holds
its socket until a chunk reaches the client, so no chunk means no release, means no turn, means no
response:

```
thread 'a_chunk_reaches_the_client_while_the_prompt_is_still_outstanding' panicked at
crates\skein-cli\tests\cli_acp_agent.rs:222:10:
the ACP client finished within 60s: Disconnected
test result: FAILED. 0 passed; 1 failed; 0 ignored; finished in 30.96s
```

The 30s is the child's own `--timeout-secs` giving up on the stalled provider. This is the red that
proves the test cannot pass without live delivery.

### Red D2 — S10, without the projection filter: the answer arrives twice

The four-line `matches!(update, SessionUpdate::AgentMessageChunk(_))` skip removed from the prompt
handler, everything else present:

```
  left: ["The ", "answer ", "is 42.", "The answer is 42."]
 right: ["The ", "answer ", "is 42."]
test result: FAILED. 0 passed; 1 failed; 0 ignored; finished in 0.83s
```

The fourth entry is the chain-derived projection repeating what the client already had — what an
editor renders as the answer appearing twice.

## Live verification (S11 and the hand-verification)

### The `#[ignore]`d live tests, against the real Ollama on this machine

```
$env:SKEIN_LIVE_MODEL = "gemma4:latest"
cargo test -p skein-gateway --test openai_compat -- --ignored --nocapture
```

```
live wire gemma4:latest @ http://localhost:11434/v1
  request  = {"model":"gemma4:latest","messages":[…],"stream":true,"stream_options":{"include_usage":true}}
  response = data: {…"delta":{"role":"assistant","content":"pong"},"finish_reason":null}]}

             data: {…"delta":{},"finish_reason":"stop"}]}

             data: {…"choices":[],"usage":{"prompt_tokens":23,…,"total_tokens":25}}

             data: [DONE]

test a_live_local_provider_exchange_is_captured_with_its_own_metering ... ok
live gemma4:latest @ http://localhost:11434/v1
  content = "pong"   tokens_used = 25   final_output = true
test a_live_local_provider_answers ... ok
test result: ok. 2 passed; 0 failed
```

The live **tool call**, with `SKEIN_LIVE_MODEL = "qwen3.8:27b"`:

```
live tool call qwen3.8:27b @ http://localhost:11434/v1
  calls  = [ToolCall { id: "call_w9qpgvx5", tool: "fs_read", args: Object {"path": String("/etc/hosts")} }]
  tokens = 335   final  = false
test a_live_local_provider_streams_a_tool_call ... ok
test result: ok. 1 passed; 0 failed; finished in 103.11s
```

### The hand-verification: the real binary, a real editor's transport, a real model

The real `skein acp-agent` binary spawned as a subprocess and driven over its actual stdio with
newline-delimited JSON-RPC, against `gemma4:latest` on `http://localhost:11434/v1`, every
notification timestamped relative to the `session/prompt` response:

```
  [ 13.88s] chunk 'One'
  [ 13.95s] chunk '\n'
  [ 14.02s] chunk 'Two'
  …
  [ 17.05s] chunk '\n'
  [ 17.11s] chunk 'Twenty'
  [ 17.20s] <== session/prompt RESPONSE {'stopReason': 'end_turn'}

chunks: 47
prompt answered at: 17.20s
chunks that arrived BEFORE the prompt response: 47/47
first chunk at 13.88s, last at 17.11s
answer = 'One\nTwo\nThree\n…\nNineteen\nTwenty'
```

**47 of 47 chunks arrived before the response**, opening a 3.3-second window in which an editor was
rendering text that this slice's predecessor would have shown all at once at 17.20s. The
concatenation is exactly the answer, and there is no forty-eighth chunk repeating it.

The chain that run left, read by a second process:

```
> skein ledger log --root … --silo alpha --run "skein-1#1"
skein-1#1  0  iteration_boundary  1d470fb7…
skein-1#1  1  llm_request         73302c30…
skein-1#1  2  wire_exchange       34b150bd…
skein-1#1  3  llm_response        63ae7ec5…
skein-1#1  4  budget_spent        9f4237ba…
skein-1#1  5  exit                9575d692…

> skein ledger verify --root … --silo alpha
skein-1#1  ok  6 steps
```

The same `StepKind` sequence slice 023 recorded — one `wire_exchange`, not forty-seven. And the
payload holds the provider's own bytes, framing included, with the new flag:

```
{"url":"http://localhost:11434/v1/chat/completions","status":200,
 "request":"{\"model\":\"gemma4:latest\",…,\"stream\":true,\"stream_options\":{\"include_usage\":true}}",
 "response":"data: {…\"content\":\"One\"…}\n\ndata: {…\"content\":\"\\n\"…}\n\n…
             data: {…\"finish_reason\":\"stop\"…}\n\n
             data: {…\"choices\":[],\"usage\":{…,\"total_tokens\":76}}\n\ndata: [DONE]\n\n",
 "streamed":true}
```

## Deviations from the plan, stated

1. **`plan.md` §0.2(4)'s framing claim is corrected, and the correction strengthens the design.**
   The plan recorded that "on all three models a single delta carried the complete array". Driving
   `qwen3.8:27b` during this run produced **two complete calls in two separate events**, `index` 0
   and `index` 1. The load-bearing halves of the measurement hold — each call is complete within its
   event, and each carries an explicit `index` — so no code changed; but the observed framing is a
   *third* shape lying between the plan's two test cases and covered by neither, and it is the shape
   an accumulator that **replaced** its call list per event instead of merging into it would fail on
   while passing every other test. A stub test now pins it
   (`tool_calls_arriving_one_whole_call_per_delta_are_translated`), and the live tool-call test sums
   the calls the provider actually framed and demands they all survive. Reported rather than
   silently adapted, per the plan's own instruction.
2. **The empty-delta guard lives in the gateway, not in `AcpTextSink`.** `plan.md` D4 put it in
   `on_text`; S5 also said the gateway pushes "each non-empty content delta". Both is one guard too
   many for one failure mode (Constitution VII), and the gateway is the correct single home: it is
   where the measurement lives (the provider sends `"content":""` on every reasoning event) and it
   protects *every* sink rather than the one that remembered. `AcpTextSink::on_text` therefore has no
   emptiness check.
3. **Six stub files were re-framed, not four.** `plan.md` D1 named `governed_run.rs`,
   `openai_compat.rs`, `cli_chat.rs` and `cli_acp_agent.rs`. `skein-connectors/tests/governed_fs_run.rs`
   and `governed_git_run.rs` each carry their own provider stub and needed the same treatment, and
   `skein-silo/tests/silo_ledger.rs` constructs a `WireExchange` literal that now names `streamed`.
   No assertion about chain shape, exit code or CLI output changed in any of them.
4. **`a12` was written weak and had to be strengthened.** As first written it asserted that no chunk
   contained the secret and that some chunk contained `***` — and it **passed against the unstreamed
   implementation**, because the redacted *concatenation* satisfies both. Recorded rather than
   quietly fixed: an assertion loose enough for the old behaviour to satisfy is not a red. It now
   asserts the exact per-delta sequence `["your key ", "***", " is fine"]`.
5. **Two guards exist that `plan.md` S5 did not enumerate, each preserving an existing test's exact
   diagnostic.** A 200 carrying no `data:` line at all is refused as `no SSE events: <body>`, and a
   well-framed stream whose events never carry a `choices[0]` is refused as `no choices[0]`. Without
   the first, an interposing proxy's HTML page would fall through to the metering refusal, naming
   the wrong problem and never showing the operator what answered; without the second, a stream of
   nothing but the metering event — which carries `"choices":[]` by design — would become an empty
   assistant message. Both are asserted by
   `an_unrecognised_response_body_is_refused`, which is the pre-existing test whose guarantees they
   preserve.
6. **`ScriptedModel` gained a `playing(…)` constructor** (`skein-acp/tests/acp_session.rs`) and the
   file gained two type aliases. Five of its seven construction sites spelled every field; adding two
   more fields would have made that seven-field literal the norm. Adjacent to the change and on the
   path it touches, per the house rule about leaving a touched path simpler.
7. **The lossy step is now ours rather than ureq's.** `Body::read_to_string` applied
   `lossy_utf8(true)`, which substitutes `?`. The stream reader takes bytes and applies
   `String::from_utf8_lossy`, which substitutes U+FFFD. Both are lossy and neither errors — the
   property slice 023's spec recorded as an accepted assumption is preserved — but the substitute
   character differs, so it is named here rather than left to be discovered.

## Residuals

- **A secret split across two deltas can reach the live ACP transcript**, because a per-delta scrub
  cannot match a needle spanning a chunk boundary. The chain is unaffected. Closing it needs
  buffering, which is the latency this slice removes. `permission.rs` already records the related
  acknowledgment about out-of-process transcripts.
- **A compat layer that omits `index` collapses every tool call into slot 0.** `index` is
  `#[serde(default)]`; every provider model measured always sends it.
- **Mid-stream cancellation is now possible and is not implemented.** `CancellableModel` still checks
  before a turn, and its docstring's "a model call already in flight completes" is still true. A
  separate slice.
- **Chain growth of roughly 48× for model I/O**, measured and accepted, bounded by the reader's
  10 MiB ceiling. A silo retention concern (design §7) if it ever becomes a real constraint.
- **The residual carried since slice 011 and repeated through 013–024 — "streaming (SSE), together
  with incremental ACP `AgentMessageChunk` notifications" — drops off this list.**

## Close (S12)

The defect slice 013 named and slices 014–024 each carried forward is closed: an editor driving
`skein acp-agent` now sees the answer being written rather than nothing followed by everything. The
gates are green, the reds are recorded, and the one measurement that disagreed with the plan is
corrected in writing with a test pinning what was actually observed.

## Next slice

- **Mid-stream cancellation**, which streaming makes possible for the first time.
- **Buffered redaction for the live transcript**, if the split-secret residual ever proves to matter
  more than the latency closing it would cost.
