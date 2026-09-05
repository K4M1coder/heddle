# Feature Specification: mid-stream cancellation (v0 slice)

**Feature Branch:** `026-mid-stream-cancel` · **Created:** 2026-09-04 · **Status:** Implemented (v0
slice) · **Input:** the residual slice 025 created and named — *"Mid-stream cancellation is now
possible and is not implemented. `CancellableModel` still checks before a turn, and its docstring's
'a model call already in flight completes' is still true. A separate slice."* — and repeated as that
slice's *Next slice* entry · Constitution III (**test-first**), IV (**explicit boundaries**), V
(**traceability**), VII (**no capability without a real need**), VIII (**loop discipline**,
NON-NEGOTIABLE) · design §4.5 (the Gateway as traceability chokepoint), §4.2 (the model port).

Slice 025 made assistant text arrive while `session/prompt` is still outstanding. It did not make the
client able to stop it. `session/cancel` set a flag that `CancellableModel` reads **before** a turn,
so a cancellation arriving during a 300-token answer did nothing until that answer was finished: the
editor's stop button was a request to stop *after* the current turn. This slice makes the stop button
stop the turn.

## What this slice changes for a user

**The stop button stops the answer being written.** A `session/cancel` arriving mid-answer ends the
read of the provider's event stream at the next line, closes the socket to the provider, and answers
`session/prompt` with `StopReason::Cancelled`. The client keeps every chunk it already received and
is sent no more.

**The provider stops generating.** Abandoning the response body drops ureq's connection, which closes
the socket; the provider sees its peer go away. Tokens are not spent on an answer nobody will read.

**Nothing else about a run changes.** `heddle chat` is byte-identical — it installs no sink, and a
client with no sink is a client with nothing to cancel. A run that is not cancelled reads exactly the
same bytes it read before. The `StepKind` sequence, the chain payloads and the CLI surface are
untouched.

## Four things a reader must know up front

1. **The signal travels on the sink, not on a new port.** `TextSink` gains one defaulted method,
   `wants_more(&self) -> bool`, returning `true`. `heddle-core` never learns what cancellation is; it
   learns that a consumer may stop wanting text. Every existing sink keeps compiling and keeps
   behaving identically.
2. **The check is per line of the stream, not per delta.** It is the first thing in the gateway's
   drain loop. A cancel arriving during a run of tool-call fragments, or during the ~150 empty-content
   reasoning events slice 025 measured, is seen on the next line of any kind rather than at the next
   non-empty delta.
3. **A cancelled turn is an error, not a short answer.** Returning the partial text as a
   `TurnResponse` would put words the model never finished onto the chain as an `LlmResponse`, and
   would feed a truncated turn to `LoopController::should_exit` as if the model had claimed it
   (Constitution VIII(a)). The error propagates through `NativeLoop::run`, which already appends the
   `WireExchange` step **before** propagating — so the bytes that arrived reach the chain.
4. **The `[DONE]`-less capture is the evidence, and there is no new field.** A cancelled stream's
   captured `response` ends without `data: [DONE]` because the read stopped before it. A
   `cancelled: bool` beside it would be a claim *about* the bytes sitting next to the bytes — the
   thing slice 025 refused when it recorded raw SSE rather than the reassembled object.

## Requirements

- **FR-001** `TextSink` MUST expose `wants_more(&self) -> bool`, defaulted to `true`. An existing
  implementation MUST require no change.
- **FR-002** The gateway's stream read MUST consult the installed sink's `wants_more` **before each
  line it reads**, and MUST stop reading the moment it answers `false`.
- **FR-003** A read stopped by FR-002 MUST make `ModelClient::turn` return an error naming the
  endpoint and the cancellation. It MUST NOT return a `TurnResponse` carrying the partial text.
- **FR-004** FR-003's error MUST be raised **before** the "no SSE events" refusal, so a cancellation
  landing before the first event is reported as a cancellation rather than as an unrecognised body.
- **FR-005** The `WireExchange` for a cancelled turn MUST still be captured and MUST still reach the
  chain, holding the bytes that arrived, verbatim, with `streamed: true` and the provider's status.
  No field MUST be added to `WireExchange`.
- **FR-006** The connection to the provider MUST NOT be returned to the client's connection pool
  after a cancelled read, and the socket MUST be closed.
- **FR-007** `AcpTextSink::wants_more` MUST answer from the session's existing cancellation flag. A
  `session/cancel` arriving mid-stream MUST end the run and MUST make `session/prompt` answer
  `StopReason::Cancelled`.
- **FR-008** A client that has already received chunks MUST NOT be sent them again by the
  chain-derived projection when the run is cancelled.
- **FR-009** A cancelled run's chain MUST still pass `verify_chain`.
- **FR-010** The pre-turn cancellation path MUST be unchanged: `CancellableModel` MUST still refuse
  before delegating once the flag is set, and both existing tests of it MUST pass unmodified.

## Rejected alternatives

| # | alternative | why not |
|---|---|---|
| 1 | `on_text` returns `bool` | conflates "here is text" with "keep going"; changes every sink's signature; can only be asked when a *content* delta arrives, so a cancel during tool-call or reasoning events waits |
| 2 | an `Arc<AtomicBool>` threaded into `OpenAiCompatClient` | makes the gateway name a mechanism ACP owns, and hands `heddle chat` a flag it has no way to set |
| 3 | a `Cancellable` port separate from `TextSink` | two ports for one thing (VII); every caller that can cancel mid-stream is already being streamed to |
| 4 | a custom ureq `Connector`/`Transport` with a cancellable socket | ureq already closes the socket and skips the pool for a half-read body, by ownership rather than by a `Drop` hook — verified in its source (`plan.md` §0.3). It would buy only the interruption of an already-blocked read |
| 5 | `WireExchange { cancelled: bool }` | a claim about the bytes beside the bytes (V); *why* a read stopped is the run's outcome, which the chain and `StopReason` already record |
| 6 | return the partial text as a successful `TurnResponse` | launders an unfinished answer past `LoopController` (VIII(a)) and writes words the model never finished onto the chain |
| 7 | check `wants_more` once per SSE **event** rather than per line | cheaper by a rounding error, and blind for the whole of a fragment or reasoning run |
| 8 | cancel the in-flight **tool call** too | a different boundary with its own decisions (a half-run tool leaves an unknown effect); out of scope and recorded as a residual |

## Out of scope

- Interrupting a `read_until` that is already blocked on a silent provider. The client's global
  timeout governs that case exactly as before.
- Cancelling a tool call in flight.
- A cancellation surface for `heddle chat`.
- Buffered redaction for the live transcript (slice 025's standing residual, untouched).
