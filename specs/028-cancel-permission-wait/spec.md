# Feature Specification: cancelling while a permission request is outstanding (v0 slice)

**Feature Branch:** `028-cancel-permission-wait` · **Created:** 2026-09-04 · **Status:** Implemented
(v0 slice) · **Input:** the residual slice 027 named in its own *Out of scope* — *"Cancelling while a
permission request is outstanding. `AcpPermissionTransport::call` blocks on an untimed `recv()`; a
cancel arriving then is observed when the human answers. A real gap, and a different one — it needs a
second channel into that `recv`."* · Constitution III (**test-first**), IV (**explicit boundaries**),
VI (**security / deny-by-default**), VII (**no capability without a real need**), VIII (**loop
discipline**, NON-NEGOTIABLE) · ADR-0003 decision 2 and ADR-0004 D3 (the ACP facade).

Slice 013 made `session/cancel` stop the *next* turn. Slice 026 made it stop the *current answer*.
Slice 027 made it kill a *running child process*. The one wait left that a cancel cannot reach is the
longest of them all and the only one with a person in it: `AcpPermissionTransport::ask` blocks the
loop thread on `rx.recv()` until the client answers the permission request. If the person who pressed
stop is the same person the dialog is waiting on, the run cannot end until they answer a question
about a tool call they have already decided against.

## What this slice changes for a user

**Stop works while the dialog is open.** A `session/cancel` arriving while a permission request is
outstanding is observed within one 50 ms poll. The tool call is refused, the loop ends at the next
turn boundary, and `session/prompt` is answered `StopReason::Cancelled` — without the client ever
having to answer.

**A cancelled session does not ask a new question.** A tool call reaching the gate on a session whose
flag is already set is refused without a `session/request_permission` being sent at all, so no dialog
appears for a run that is already over.

**The wait itself is not shortened.** There is no new timeout on a permission request. A person may
take minutes to read one, and this slice does not put a clock on that decision — only a second way
out of it.

**Nothing else about a permission answer changes.** Allow still allows, reject still rejects, a
client-side `Cancelled` outcome still refuses with the sentence it always did, and a connection that
dies under an open question is still refused in the transport's own words. A run nobody cancels
behaves exactly as before.

## Four things a reader must know up front

1. **The signal is the flag the session already has** — the one `Arc<AtomicBool>` slice 027 moved
   into `SessionParts`. `AcpPermissionTransport` gains a fourth field holding a clone of it, handed
   over by `SkeinSession::new`, which is the one frame that already holds both. No new port, no new
   channel, no widening of `ToolTransport`, and nothing new for `skein-cli` to wire.
2. **The blocking `recv()` becomes a polled `recv_timeout` loop with no overall deadline.** The only
   three ways out are an answer — including the error ACP delivers *as* an answer when the transport
   dies — a set flag, and a disconnected channel. `RecvTimeoutError` is matched **exhaustively**:
   `Timeout` continues the loop and `Disconnected` ends it. There is no wildcard arm, because the one
   that would be written by habit collapses two opposite meanings.
3. **A cancelled wait is a `ToolDenied`, and it says which of the two cancellations happened.** ACP
   already has a `RequestPermissionOutcome::Cancelled` — *the client* withdrew the question — which
   refuses with `"acp permission request cancelled"`. This slice's refusal is a different fact — *the
   session* was cancelled while the question was open — and gets a deliberately different sentence,
   so a chain reader can tell them apart.
4. **The two checks are not one check written twice.** The check at the top of `ask` prevents a
   dialog from ever being raised for a cancelled session; the check inside the loop ends a wait
   already in progress. Neither can do the other's job, and this slice proves it by reverting each
   one separately and recording the two different failures.

## Requirements

- **FR-001** `AcpPermissionTransport` MUST hold this session's cancellation flag, supplied by the
  caller that constructs it rather than minted by it.
- **FR-002** `ask` MUST refuse without sending a permission request when the flag is already set.
- **FR-003** `ask` MUST observe the flag becoming set while a permission request is outstanding, and
  MUST refuse then, without the client having answered.
- **FR-004** FR-003 MUST happen within 50 ms of the flag being set, plus the loop's own overhead.
- **FR-005** FR-002 and FR-003 MUST produce a `SkeinError::ToolDenied` for the tool under call, whose
  reason is **distinct** from the `RequestPermissionOutcome::Cancelled` reason.
- **FR-006** The inner transport MUST NOT be reached in either case.
- **FR-007** There MUST be no overall deadline on a permission request. A client that answers after
  many poll slices, with the flag never set, MUST still have its answer honoured.
- **FR-008** A connection that closes with a question outstanding MUST end the wait and MUST be
  reported as a transport failure, distinct from **both** cancellations. *Measured during S4, and it
  corrected this requirement's first draft:* ACP **invokes** a pending `on_receiving_result` callback
  with an `Err` when the transport closes rather than dropping it, so a dead connection arrives down
  the answer channel as an answer and is refused as `"acp permission request failed: …"`.
  `RecvTimeoutError::Disconnected` is therefore not the path a closed connection takes — it is
  reachable only if a callback is dropped uninvoked. Its arm keeps the `"acp connection closed"`
  message it had before this slice and cannot be omitted, because `recv_timeout` has two error
  variants.
- **FR-009** The three existing answer paths — allow, reject, and the client's own `Cancelled`
  outcome — MUST be unchanged in behaviour and in message.
- **FR-010** The three existing cancellation readers MUST be unchanged: `CancellableModel`'s pre-turn
  refusal, `AcpTextSink::wants_more`'s per-line check, and `skein-sandbox`'s launcher poll.
- **FR-011** No test in the suite may hang when a permission request is never answered. The test
  harness MUST turn that into a recorded failure.

## Rejected alternatives

| # | alternative | why not |
|---|---|---|
| 1 | put a timeout on the permission request itself | it answers a different question. A person may legitimately take minutes; a deadline would refuse tool calls nobody cancelled, and would have to be configurable, which is a slice of its own |
| 2 | a wildcard arm on `RecvTimeoutError` | its two variants mean opposite things. Collapsed to "give up", the run dies every 50 ms; collapsed to "retry", a dead connection spins forever. The compiler can enforce the distinction only if nothing waives it |
| 3 | a second `mpsc` channel the canceller sends on, selected over | `std::sync::mpsc` has no `select`; it would need a shared channel, a crossbeam dependency, or a thread per wait — to carry one bit the session already publishes as an `AtomicBool` |
| 4 | widen `ToolTransport` with `cancel()` | slice 027's rejected alternative 1, unchanged: `call` holds `&mut self` for the whole call, so no `&mut` is left for a canceller to reach through |
| 5 | a `CancellableTransport` decorator outside the gate | it can only check before and after the call, which is what today's code effectively does, and that is the bug |
| 6 | drop the pre-request check and rely on the in-loop one | it would raise a dialog for a session already cancelled and refuse it a poll later. The person who pressed stop would watch a question appear and vanish |
| 7 | drop the in-loop check and rely on the pre-request one | it is the whole slice: the cancel that matters arrives *after* the question was asked |
| 8 | reuse `"acp permission request cancelled"` for both | two different facts about a session — the client withdrew the question, versus the session ended under it — reaching the chain as the same sentence. Measured in S7: that sentence is the *only* trace either refusal leaves, since a `ToolDenied` becomes a tool-role message in the next turn's `llm_request` payload rather than a `ToolResult` step |
| 9 | a shorter poll slice | 50 ms is already below what a person notices, and it is the number slice 027 pinned for the same job one crate down. A second, different constant for the same purpose would be a number to keep in agreement by hand |
| 10 | treat a cancelled wait as a successful "reject" answer | the client did not reject anything. The chain would record an answer nobody gave |

## Out of scope

- **Any timeout on a human's decision** (rejected alternative 1).
- **`skein chat`.** It has no permission gate at all: `AcpPermissionTransport` exists only inside the
  ACP facade, and `skein chat` has no cancel channel either (slices 026 and 027 recorded the same
  boundary).
- **Withdrawing the request on the wire.** ACP has no agent-initiated cancel for an outstanding
  `session/request_permission`; the client is left to notice the session it cancelled. Skein stops
  waiting for the answer, and an answer that arrives later is dropped with the channel.
- **Cancelling a non-process tool already executing.** Unchanged from slice 027: the five remaining
  tools are bounded by their own caps and complete in milliseconds.
