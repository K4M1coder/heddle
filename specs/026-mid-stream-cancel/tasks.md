# Tasks: mid-stream cancellation (v0 slice)

**Spec:** `specs/026-mid-stream-cancel/spec.md` · **Plan:** `specs/026-mid-stream-cancel/plan.md` ·
TDD (red→green), branch `026-mid-stream-cancel`, fast-forwarded onto `dev` at `2806ecf`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)

- **I Headless core** ✅ no CLI of its own, no new flag, no new command, no new argument. `heddle chat`
  is byte-identical: it installs no sink, and `drain`'s check is `sink.as_ref().is_some_and(…)`, so a
  run with no sink reads exactly the bytes it read before. `heddle ledger log`/`show`/`verify` render a
  cancelled run's chain with zero CLI change, verified against a live one (see *Live verification*).
- **II Local-first** ✅ no new dependency, no `Cargo.toml` change, no second network call, no
  `Connector` and no `Transport`. `ureq` stays declared with `default-features = false`. The slice
  reads the one HTTP response that already existed, for a shorter time.
- **III Test-First** ✅ five reds observed and recorded verbatim below. Red A is a compile absence,
  red B a behavioural one; reds C, D and E each isolate one decision by reverting exactly it from the
  finished implementation. Red D isolates a **pure ordering**: with the `Cancelled` arm moved four
  lines down, 26 of 27 tests still pass.
- **IV Inverted coupling** ✅ `heddle-core` gains one defaulted method taking nothing and returning
  `bool`. It does not name cancellation, ACP, SSE, HTTP or a provider — it says a consumer may stop
  wanting text. Every fact about the event format stays in `heddle-gateway`; every fact about
  `session/cancel` stays in `heddle-acp`.
- **V Traceability** ✅ **no new `WireExchange` field.** A cancelled turn's capture is the bytes that
  arrived, verbatim, ending without `data: [DONE]` because the read stopped before it — the evidence
  *is* the record rather than a claim beside it, which is slice 025's reasoning applied to the case
  it created. `NativeLoop::run` already appends the `WireExchange` step before propagating the error,
  so the bytes outlive the failure; the live run's chain is three steps and verifies.
- **VI Security** ✅ untouched. No new payload, no new path out of the process, no change to
  redaction. The live transcript's per-delta scrub is unchanged, and cancelling can only make *fewer*
  deltas leave the process.
- **VII Neutrality** ✅ one defaulted trait method, one enum variant, one `if`, one match arm, one
  struct field. No flag, no config key, no crate, no dependency, no `StepKind`. Eight alternatives
  are rejected with a reason each in `spec.md`, and D5 rejects the machinery ureq's own API most
  invites — on that crate's source rather than on taste.
- **VIII Loop discipline** ✅ NON-NEGOTIABLE, and the slice defends it. A cancelled turn is an
  **error**, never a short `TurnResponse`: returning partial text would let an answer the model never
  finished be adjudicated by `LoopController::should_exit` as one it claimed (VIII(a)), and would put
  those words on the chain as an `LlmResponse`. The controller, budget, probe and exit conditions are
  neither read nor written.
- **Cross-platform** ✅ no `#[cfg]`, no platform API, no filesystem or process work. The stubs are
  `std::net::TcpListener`s on loopback, as they already were. The new live test is `#[ignore]`d and
  gated on `HEDDLE_LIVE_MODEL`, as slices 019–025 established.

## Tasks

- [x] **S0** fast-forwarded onto `dev` at `2806ecf`, control baseline measured, and
      `specs/026-mid-stream-cancel/{spec.md,plan.md,tasks.md}` written
- [x] **S1** RED — the gateway's read stops, the turn fails naming the cancellation, the capture
      holds what arrived and no `[DONE]`, and a sink that never overrides `wants_more` still reads
      the whole stream
- [x] **S2** RED — a sink that stops before the first event is reported as cancelled, not as an
      empty/unrecognised stream (the D3 ordering)
- [x] **S3** GREEN — `heddle-core`: the defaulted `TextSink::wants_more`
- [x] **S4** GREEN then RED-by-revert — `heddle-gateway`: `StreamFault::Cancelled`, the drain check,
      the `turn` arm; then the check reverted (red C) and the arm moved below `events == 0` (red D)
- [x] **S5** RED — `heddle-acp`: a `session/cancel` arriving after the client has seen its first
      chunk ends the turn and reports `Cancelled`
- [x] **S6** GREEN — `heddle-acp`: `AcpTextSink` answers `wants_more` from the session's flag;
      `HeddleSession` hands it over; `CancellableModel`'s docstring corrected. `a7` and `x1`
      **unmodified**
- [x] **S7** RED-by-revert — `AcpTextSink::wants_more` reverted with S5 applied (red E). See
      *Deviations* 1: the plan said "the drain check", which is not on this test's path
- [x] **S8** live verification — **part of this run**, against the real Ollama and against the real
      `heddle acp-agent` binary
- [x] **S9** close-out

## Control baseline (S0)

Measured on this worktree immediately after the fast-forward to `2806ecf`, before any edit:

| gate | result |
|---|---|
| `cargo test --workspace` | **268 passed, 0 failed, 8 ignored** |

At close, on the same worktree with the slice applied: `cargo fmt --all -- --check` pass, `cargo
clippy --workspace --all-targets -- -D warnings` pass, **273 passed, 0 failed, 9 ignored**. The delta
is **+5 passed** and **+1 ignored**. Six tests were added — five in
`heddle-gateway/tests/openai_compat.rs` (one of them the `#[ignore]`d live cancellation test) and one
in `heddle-acp/tests/acp_session.rs`. No test was deleted, renamed or disabled; nothing moved from
passed to ignored.

## Verified before trusting: the plan's §0.3 ureq research, re-read

Every claim in `plan.md` §0.3 was read out of `ureq-3.4.0`'s own source before D5 was relied on,
because the plan's instruction was to stop rather than adapt if it did not hold.

| plan claim | re-checked | verdict |
|---|---|---|
| §0.3(1) `Connection::reuse` is the only path back into the pool | `pool.rs:125`; `pool.add(self)` is reached from nowhere else | **holds** |
| §0.3(2) `cleanup` is `reuse`'s only caller | `grep -rn "\.reuse(" src/` → one hit, `run.rs:614` | **holds** |
| §0.3(3) `cleanup` is only reached once the body is fulfilled | `run.rs:239/250/747`; `BodyHandler::ended` guards the third | **holds** |
| §0.3(4) no `Drop` impl anywhere in the crate | `grep -rn "Drop" src/` → no matches | **holds** |
| §0.3(5) the documented pool contract | `body/mod.rs:68-72`, *"the body must be read to end"* | **holds** |
| §0.3 consequence — the socket closes and the client stays usable | observed live: a cancelled turn's follow-up request on the **same client** answered `"pong"` | **holds** |

## Observed red

Five reds. A and B are the absence of the feature; C, D and E each isolate one decision by reverting
exactly it from the finished implementation, so each is evidence about *this* code rather than about
a sketch of it.

### Red A — S1/S2, the port has no such method

Tests applied, sources at `2806ecf`. `cargo test -p heddle-gateway --test openai_compat`:

```
error[E0407]: method `wants_more` is not a member of trait `TextSink`
    --> crates\heddle-gateway\tests\openai_compat.rs:1029:5
     |
1029 | /     fn wants_more(&self) -> bool {
1030 | |         self.seen.lock().unwrap().len() < self.stop_after
1031 | |     }
     | |_____^ not a member of trait `TextSink`
```

### Red B — S3 applied, the reader ignores the answer

With the defaulted `wants_more` in `heddle-core` and nothing in the gateway, the file compiles, so
this red is behavioural — the more informative of the two, because it shows the semantics missing
rather than an API:

```
thread 'a_sink_that_stops_wanting_text_ends_the_read_mid_stream' panicked at
crates\heddle-gateway\tests\openai_compat.rs:1054:25:
expected a cancellation, got TurnResponse { message: Message { role: Assistant, parts: [Text {
text: "The answer is 42." }], ... }, tokens_used: 61, final_output: true, tool_calls: [] }

test result: FAILED. 24 passed; 3 failed; 3 ignored
```

`a_sink_that_does_not_override_wants_more_reads_the_whole_stream` **passed** here, which is the
point of it: the default is `true`, so the slice is invisible to every sink written before it.

### Red C — S4, without the drain check

`StreamFault::Cancelled` and the `turn` arm present; only the four-line check at the top of `drain`
removed. Identical failure to red B, and that identity is the evidence: the check is the whole of
the behaviour, and the variant and the arm are inert without it.

```
test result: FAILED. 24 passed; 3 failed; 3 ignored
```

### Red D — S4, with the `Cancelled` arm moved below the `events == 0` check

Everything present, the arm relocated four lines down. **26 of 27 pass.** The one that does not is
the one this ordering exists for:

```
thread 'a_sink_that_stops_before_the_first_event_is_reported_as_cancelled_not_as_an_empty_stream'
panicked at crates\heddle-gateway\tests\openai_compat.rs:1147:5:
the refusal must name the cancellation, got: http://127.0.0.1:53820/v1 returned an unrecognised
chat-completions response: no SSE events:

test result: FAILED. 26 passed; 1 failed; 3 ignored
```

An operator's own stop button reported as an interposing proxy. Every line executes, in the wrong
order, and only this test says so.

### Red E — S7, without `AcpTextSink::wants_more`

The gateway fully green, the sink's override removed, everything else present:

```
thread 'a13_a_cancel_arriving_mid_stream_ends_the_turn_and_reports_cancelled' panicked at
crates\heddle-acp\tests\acp_session.rs:992:5:
  left: ["The ", "answer ", "is ", "42."]
 right: ["The "]

warning: field `cancelled` is never read
test result: FAILED. 18 passed; 1 failed; 0 ignored; finished in 10.01s
```

**`assert_eq!(stop, StopReason::Cancelled)` passed against the defect.** `HeddleSession::run` maps a
set flag to `Cancelled` however the turn ended, so the stop reason was already right while the
client was still being sent all four deltas. Only the exact chunk sequence catches it — recorded
because a test asserting the stop reason alone would have been green against the bug it was written
for.

## Live verification (S8)

### The `#[ignore]`d live tests, against the real Ollama on this machine

`2026-09-04T05:13:35Z` — `2026-09-04T05:14:05Z`

```
$env:HEDDLE_LIVE_MODEL = "gemma4:latest"
cargo test -p heddle-gateway --test openai_compat -- --ignored --nocapture --test-threads=1
```

```
live cancel gemma4:latest @ http://localhost:11434/v1
  deltas    = 8 (stopped after 8)
  text      = "1\n2\n3\n4\n"
  turn took = 22.7659863s, of which 285.1µs after the last delta
  capture   = 66229 bytes, ends "{\"index\":0,\"delta\":{\"content\":\"\\n\"},\"finish_reason\":null}]}\n"
  next turn = "pong"
test a_live_local_provider_stops_when_the_sink_stops_wanting_text ... ok

test result: ok. 4 passed; 0 failed
```

The model was asked to count to 300 and the read ended **285 µs after the eighth delta**, not at the
end of the answer. The other three live tests — slice 025's — pass unchanged beside it.

**66 229 bytes for eight content deltas** is D1's argument as a measurement. Almost all of that
stream carries `delta.reasoning`, which is discarded and never reaches `on_text`; a check riding on
`on_text` would have been blind for the whole of it. An earlier run of the same test against a warm
model produced 1 686 bytes and `957.8ms / 237.1µs` — the mechanism is identical, the model's
thinking is not.

### The hand-verification: the real binary, a real editor's transport, a real model

`2026-09-04T05:14:28Z` — `2026-09-04T05:14:32Z`. The real `heddle acp-agent` spawned as a subprocess
and driven over its actual stdio with newline-delimited JSON-RPC, against `gemma4:latest` on
`http://localhost:11434/v1`, prompted to count to 300, cancelled after six chunks, everything
timestamped relative to the `session/prompt` request:

```
session = heddle-1
[  2.98s] ==> session/cancel sent after 6 chunks

  [  2.62s] chunk '1'
  [  2.70s] chunk '\n'
  [  2.76s] chunk '2'
  [  2.84s] chunk '\n'
  [  2.91s] chunk '3'
  [  2.97s] chunk '\n'
  [  3.05s] chunk '4'
  [  3.05s] <== session/prompt RESPONSE {'stopReason': 'cancelled'}

chunks after the cancel   : 1  ['4']
prompt answered at        : 3.05s  (0.06s after the cancel)
stopReason                : cancelled
answer so far             : '1\n2\n3\n4'
```

**The prompt was answered 60 ms after the cancel**, having stopped at 4 of 300. Exactly one chunk
landed after the notification was sent — the line already in flight, which is D2's accepted residual
shown rather than described. Before this slice the same run would have written all 300 numbers, at
the ~70 ms per number visible above, and *then* reported `cancelled`.

The chain that run left, read by a second process:

```
> heddle ledger log --root … --silo alpha --run "heddle-1#1"
heddle-1#1  0  iteration_boundary  1d470fb7…
heddle-1#1  1  llm_request         ab77b60e…
heddle-1#1  2  wire_exchange       5ae7f9c6…

> heddle ledger verify --root … --silo alpha
heddle-1#1  ok  3 steps
```

Three steps, and no `Exit`: the run did not end, it was ended. The `wire_exchange` payload is D4 in
full — the provider's own bytes, framing included, stopping where the read stopped:

```
{"url":"http://localhost:11434/v1/chat/completions","status":200,
 "request":"{\"model\":\"gemma4:latest\",\"messages\":[{\"role\":\"user\",\"content\":\"Count from 1 to 300…",
 "response":"…data: {…\"delta\":{\"content\":\"\\n\"}…}\n\ndata: {…\"delta\":{\"content\":\"4\"},\"finish_reason\":null}]}\n",
 "streamed":true}
```

`[DONE]` is absent, `streamed` is `true`, and the last event is the `4` the client saw. Nothing in
the record says "cancelled"; the record simply stops, which is the whole of D4.

## Deviations from the plan, stated

1. **S7's red reverts `AcpTextSink::wants_more`, not the drain check.** The plan (and the run's
   instructions) said the ACP red was to be obtained by temporarily reverting the drain check, as
   slice 025 did. It cannot be: `a13` drives a `ScriptedModel`, not `OpenAiCompatClient`, so
   `heddle-gateway`'s `drain` is not on its path and reverting the check there leaves the test green.
   The equivalent isolation at that level is the sink's override, and reverting it produces red E
   above — with `warning: field 'cancelled' is never read` confirming the revert was exactly that one
   thing. Reported rather than silently substituted.
2. **No stub-level test asserts that the socket closes (FR-006).** The `Stub` in `openai_compat.rs`
   answers with `connection: close` on every reply — deliberately, so multi-turn tests count accepts
   without racing ureq's pool — so a test built on it could not distinguish ureq closing the
   connection from the stub asking for it. Rather than re-frame the harness to prove something ureq's
   own source already states, FR-006 rests on the §0.3 citations plus the live observation that the
   **same client** answered its next request after a cancelled turn. Named here because it is the one
   requirement with no dedicated in-process assertion.
3. **`ScriptedModel` gained a third optional behaviour, `awaits_stop`.** The double now asks
   `wants_more` before each delta and, when `awaits_stop` is set, waits **once after the first delta**
   for the sink itself to stop wanting text. Waiting on the real signal rather than on a channel is
   what lets `a13` cancel over a real ACP connection without racing its delivery — the gate/`started`
   pair `a7` uses cannot express "after the client has seen text". Bounded at 10 s, so a cancellation
   that never arrives fails an assertion instead of hanging the suite. `a7`, `a11` and `a12` are
   unaffected: `a7` streams no deltas, and the other two set no flag.
4. **The live test deadlocked itself as first written, and is recorded rather than quietly fixed.**
   It held its `Mutex` guard (`let seen = seen.lock().unwrap();`) across the follow-up turn while the
   same sink was still installed, so `on_text` re-locked from the same thread. It also left a sink
   whose `wants_more` was permanently `false`, which would have cancelled the follow-up turn
   immediately. Both are test defects, not product ones — but the first cost twenty minutes of a
   hang that looked like a provider problem, and the second would have made a passing assertion
   meaningless.
5. **`Observed` gained `wait_for_chunks`, and `acp_session.rs` gained an `OBSERVE_TIMEOUT`.** A test
   that acts *during* a turn has to act on delivery rather than on a guess about timing; without it,
   `a13` would sometimes cancel before the first chunk and become a second copy of `x1`.

## Residuals

- **A cancel cannot interrupt a `read_until` already blocked.** It takes effect on the next line —
  285 µs live, and one chunk after the notification in the hand-verification. On a provider that has
  stopped writing entirely, the client's global timeout governs, exactly as before. Closing this
  needs a custom ureq `Connector`/`Transport`, rejected in D5 until something measures the latency as
  a real cost.
- **A tool call already in flight is not cancelled.** `AcpPermissionTransport` runs it to completion
  and the cancellation is observed at the next turn boundary, as before this slice.
- **`heddle chat` has no cancellation surface.** It installs no sink and has no channel to set one; a
  `Ctrl-C` story for it is a CLI slice with its own decisions.
- **A cancelled run's chain has no `Exit` step.** It verifies, and the absent step is honest — the
  run was ended rather than ending — but a reader counting exits per run must know a cancelled run
  has none. Unchanged by this slice: the pre-turn path already produced it.
- **A secret split across two deltas can reach the live ACP transcript** — slice 025's residual,
  untouched, and now bounded slightly more tightly, since a cancelled turn emits fewer deltas.
- **The residual slice 025 created — "mid-stream cancellation is now possible and is not
  implemented" — drops off this list.**

## Close (S9)

The stop button stops the answer. A `session/cancel` arriving mid-stream ends the read at the next
line, closes the socket to the provider, and answers `session/prompt` with `cancelled` — measured at
60 ms against a real model that had been asked for 300 lines and stopped at 4. The gates are green,
the five reds are recorded, the ureq research was re-read out of that crate's source before it was
relied on, and the machinery the same research invited was rejected in writing rather than built.

## Next slice

- **Cancelling a tool call in flight**, which is the other half of "stop" an editor's user means.
- **A cancellation surface for `heddle chat`**, if a terminal user ever needs the same button.
- **Buffered redaction for the live transcript**, if the split-secret residual ever proves to matter
  more than the latency closing it would cost.
