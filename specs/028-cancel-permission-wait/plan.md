# Plan — slice 028: cancelling while a permission request is outstanding

**Target artifacts:** `specs/028-cancel-permission-wait/{spec.md,plan.md,tasks.md}` plus the code
changes below. **Branch:** `028-cancel-permission-wait`, cut from `dev`. **No PR** (the bare mirror at
`D:/claudecode/heddle-origin.git` exists only for Archon's worktree isolation). Conventional Commits.
Strict TDD (Constitution III): red before green.

---

## 0. Read this first — the tree, and the one wait a cancel still cannot reach

### 0.1 The worktree was stale. Fast-forwarded before anything else.

`HEAD` here was `d364405`; `dev` is `a8a9d5e`, **43 commits ahead**. Everything this slice builds on
arrived with slice 027 and is absent from `d364405`: `SessionParts.cancelled` (the flag supplied
rather than minted) does not exist there at all, and neither does the third reader
(`heddle-sandbox`'s launcher poll) that the module doc this slice edits describes. **S0 of this run:
fast-forward `028-cancel-permission-wait` onto `dev` at `a8a9d5e` and re-measure the control
baseline.** Every anchor below is a `dev` anchor.

### 0.2 Anchors verified on `dev` at `a8a9d5e`

| anchor | file | fact |
|---|---|---|
| `AcpPermissionTransport` | `heddle-acp/src/permission.rs:19` | **three** fields: `inner`, `connection`, `session_id` |
| `AcpPermissionTransport::new` | `heddle-acp/src/permission.rs:26` | `(inner, connection, session_id)`, three arguments |
| `ask` | `heddle-acp/src/permission.rs:38` | returns `Result<RequestPermissionOutcome>`; `send_request(…).on_receiving_result(…)` then **`rx.recv()`** — untimed, unconditional |
| the closed-channel refusal | `heddle-acp/src/permission.rs:70` | `rx.recv().map_err(|_| HeddleError::Tool("acp connection closed"))` |
| the client-cancelled refusal | `heddle-acp/src/permission.rs:91` | `_ => Err(denied("acp permission request cancelled".into()))` — the **only** occurrence of that string in the workspace |
| the one construction site | `heddle-acp/src/lib.rs:103` | `AcpPermissionTransport::new(parts.transport, connection, id.clone())`, inside `HeddleSession::new` |
| the flag in that frame | `heddle-acp/src/lib.rs:84` | `let cancelled = parts.cancelled;` — already in scope, already cloned three times below |
| `SessionParts.cancelled`'s doc | `heddle-acp/src/lib.rs:50-57` | says the flag is read "from **three** places" |
| `cancel.rs`'s module doc | `heddle-acp/src/cancel.rs:6-18` | "There are two others, all three on the **one** flag"; enumerates `AcpTextSink::wants_more` and `heddle-sandbox`'s launcher |
| the test harness | `heddle-acp/tests/acp_session.rs:1018` | `ask_permission(outcome, tool_calls)`, driving `call` directly against a real ACP client |
| the harness's own wait | `heddle-acp/tests/acp_session.rs:1076` | `spawn_blocking(move || rx.recv().expect("answered"))` — **untimed**; a client that never answers hangs the test binary |
| `OBSERVE_TIMEOUT` | `heddle-acp/tests/acp_session.rs:26` | `10s`, the file's established "a signal that never arrives is a failure, not a hang" bound |
| the three answer tests | `heddle-acp/tests/acp_session.rs:1096,1109,1125` | `p1` allow, `p2` reject, `p3` client-cancelled — this slice's controls |
| `POLL_SLICE` | `heddle-sandbox/src/launch.rs` | `50ms`, slice 027's pinned number for the same job one crate down |

### 0.3 What a `session/cancel` reaches today, and where it stops

One `Arc<AtomicBool>` per session, supplied by `heddle-cli`'s ACP session factory, read from three
places: `CancellableModel::turn` before a turn (slice 013), `AcpTextSink::wants_more` per line of the
provider's stream (slice 026), and `heddle-sandbox`'s launcher per 50 ms of a child's life (slice
027). The fourth wait is not read at all. `AcpPermissionTransport::ask` sends
`session/request_permission`, registers a callback that forwards the answer down an `mpsc`, and then
blocks the loop thread on `rx.recv()`. Nothing but an answer or a dead connection returns from that
call.

**What the editor sees today.** A tool call needs approval. The dialog appears. The person decides
they want none of it and presses stop instead of answering. `session/cancel` sets the flag; the flag
is read by nobody who is waiting. The loop thread stays blocked in `recv()`. The dialog stays open.
`session/prompt` stays unanswered — for as long as the dialog is ignored, which unlike every other
wait in the product has no upper bound at all. The only way to end the run is to answer a question
about a tool call that will be discarded either way.

---

## 1. Problem

The one wait in the product with no timeout is the one wait cancellation cannot reach. Pressing stop
while a permission dialog is open does nothing until the dialog is answered.

## 2. Approach

### D1 — A fourth field on `AcpPermissionTransport`, from the frame that already holds the flag

```rust
pub struct AcpPermissionTransport<T: ToolTransport> {
    inner: T,
    connection: ConnectionTo<Client>,
    session_id: SessionId,
    cancelled: Arc<AtomicBool>,
}
```

`HeddleSession::new` is the only construction site in the product, and slice 027 already put
`cancelled` in scope there, three lines above, cloned into `CancellableModel` and `AcpTextSink`. The
gate gets a fourth clone of the same `Arc`:

```rust
AcpPermissionTransport::new(parts.transport, connection, id.clone(), cancelled.clone())
```

**No `heddle-cli` change, and that is the point of slice 027 having gone first.** The flag is already
caller-supplied and already reaches this frame; the composition root does not learn a new fact. This
is why the wiring risk slice 027 spent its S10 on does not recur here — there is no second `Arc` to
get wrong, because the only place that could mint one is a `parts.cancelled` field the compiler
requires the caller to fill.

**Rejected — a `CancellableTransport` decorator outside the gate.** Slice 027's rejected alternative
2 applies verbatim and for the same reason: a decorator can check before and after the call, and
today's code already effectively does that. The wait is *inside*.

**Rejected — a second `mpsc` the canceller sends on.** `std::sync::mpsc` has no `select`. Making one
receiver serve both an answer and a cancellation means either a channel the session must be given a
`Sender` for (a new wiring obligation, and a new way to wire two of them), a `crossbeam-channel`
dependency for one `select!`, or a thread per outstanding request whose only job is to translate an
`AtomicBool` into a message. The flag is already published, already `Sync`, and already read by three
other waits exactly this way.

### D2 — `recv()` becomes `recv_timeout(POLL_SLICE)` in a loop, with no overall deadline

```rust
const POLL_SLICE: Duration = Duration::from_millis(50);

loop {
    if self.cancelled.load(Ordering::SeqCst) {
        return Ok(Answer::SessionCancelled);
    }
    match rx.recv_timeout(POLL_SLICE) {
        Ok(answered) => {
            return answered
                .map(Answer::Client)
                .map_err(|e| HeddleError::Tool(format!("acp permission request failed: {e}")))
        }
        Err(RecvTimeoutError::Timeout) => continue,
        Err(RecvTimeoutError::Disconnected) => {
            return Err(HeddleError::Tool("acp connection closed".into()))
        }
    }
}
```

**There is no deadline, and that is a decision rather than an omission.** This is the deliberate
asymmetry with slice 027's `launch::wait`, which *does* compute an absolute deadline: `RUN_TIMEOUT`
is a real budget for a machine that should not burn a core indefinitely. The thing being waited on
here is a person reading a question. Minutes is a legitimate answer. A deadline would refuse tool
calls nobody cancelled, and choosing its value would need a configuration surface — a different
slice, with a different justification. The three exits are an answer, a set flag, and a dead channel.

**The match is exhaustive, and this is the one line in the loop a wildcard would silently break.**
`RecvTimeoutError`'s two variants mean opposite things:

| variant | means | must do |
|---|---|---|
| `Timeout` | nobody has answered yet — the normal case, 20 times a second, for the whole life of the dialog | continue |
| `Disconnected` | the `Sender` is gone: a callback was dropped uninvoked, and no answer will ever come | return the closed-connection refusal |

**A measurement from S4 that narrows the second row.** A *closed connection* does not produce
`Disconnected`: ACP invokes the pending callback with an `Err` instead of dropping it, so the failure
arrives as an answer and is refused as `"acp permission request failed: …"`. `Disconnected` remains
reachable only if a callback is dropped uninvoked, and its arm cannot be omitted regardless —
`recv_timeout` has two error variants and this slice refuses to waive either. What the S4 control
therefore pins is the *observable* requirement: a dead connection ends the wait promptly and is
reported as neither cancellation. Recorded rather than quietly rewritten, because the first draft of
FR-008 asserted the arm that S4 showed is not on that path.

Written as `_ => continue`, a dead connection spins this thread forever. Written as
`_ => Err("acp connection closed")` — the shape today's code has, because today the only error `recv`
can produce *is* disconnection — **every permission request in the product dies 50 ms after it is
asked**. The second is the mistake this loop actually invites, because it is what the existing line
turns into if it is mechanically converted rather than re-derived. Both arms are therefore written
out, and no `_` arm exists to absorb a future variant without a compile error.

**50 ms, reusing slice 027's number.** Worst-case latency from `store` to refusal is one slice. The
cost is one atomic load and one 50 ms timed wait per slice of a dialog's life, on a thread whose only
other activity is being blocked. Slice 027 pinned 50 ms for the same trade one crate down; a second,
different constant for the same purpose would be two numbers to keep in agreement by hand.

### D3 — `ask` returns `Answer`, not `RequestPermissionOutcome`

```rust
enum Answer {
    Client(RequestPermissionOutcome),
    SessionCancelled,
}
```

`ask` can now end in a way ACP has no vocabulary for. `RequestPermissionOutcome` is the *client's*
answer, and there is no honest value in it for "the client did not answer" —
`RequestPermissionOutcome::Cancelled` already means something else and is D5's whole subject.

**Rejected — `Result<Option<RequestPermissionOutcome>>`.** `None` would carry the fact but not its
name, and `call`'s match would read `None => …` at the site where the distinction has to be obvious.

**Rejected — an extra `HeddleError` variant returned straight from `ask`.** `call` is the frame that
knows the tool name, and `denied` is its local closure; refusing from inside `ask` would either
duplicate that knowledge or return a `HeddleError::Tool` where every other refusal on this path is a
`ToolDenied`, which is a different thing on the chain.

The enum is private to the module: nothing outside `permission.rs` needs to name it. (Note for a
reader of the test binary: `acp_session.rs` has an unrelated test-local `Answer { Allow, Reject }`,
which is the *client's* scripted behaviour in `with_facade`. They never meet.)

### D4 — Two checks, and they are not the same check twice

The check at the top of `ask` sits **before `send_request`**:

```rust
fn ask(&self, tool: &str) -> Result<Answer> {
    if self.cancelled.load(Ordering::SeqCst) {
        return Ok(Answer::SessionCancelled);
    }
    …
```

so a cancelled session never raises a dialog. The check inside the loop ends a wait already in
progress. The failure modes are disjoint and observable separately:

| revert | what still passes | what fails, and how |
|---|---|---|
| remove the pre-request check | the refusal, its message, and "the inner transport was not reached" — all unchanged | the client is asked a question for a session that is already over: the observed `session/request_permission` count is 1 where the test requires 0 |
| remove the in-loop check | the pre-request test, entirely | the wait never ends: the harness's bounded `recv_timeout` reports "the call never returned", which is FR-011's reason for existing |

S6 performs **both** reverts and records both failures. One revert would leave the other check
looking like belt-and-braces.

This is a departure from slice 027's D4, which *rejected* its analogous pre-launch check, and the
difference is what makes it worth stating. There, the check would have been unobservable: a process
started and killed within one poll slice is indistinguishable, from outside, from one never started.
Here the un-checked path puts a modal dialog in front of a person for a run that has already ended,
and a client-side request counter sees it. A check no test can distinguish is speculative machinery;
a check a test distinguishes is the behaviour.

### D5 — Two cancellations, two sentences

ACP's `RequestPermissionOutcome::Cancelled` means *the client withdrew the question* — it turned the
dialog away itself — and refuses with `"acp permission request cancelled"`, today's string, on
`permission.rs:91`. This slice's refusal means *the session was cancelled while the question was
open*. The reason string is deliberately different:

```rust
Answer::SessionCancelled => Err(denied("session cancelled while awaiting acp permission".into()))
```

**Where the sentence actually lands, measured in S7 rather than assumed.** A `ToolDenied` leaves no
`ToolResult` step at all: `NativeLoop::mediate` turns it into
`"the {tool} tool call was refused: {reason}"` and pushes that as a tool-role message
(`native_loop.rs:191-194`), so it reaches the chain inside the **next turn's `llm_request` payload**.
The live run's chain is `… tool_call, approval, iteration_boundary, llm_request` — the `approval`
step is the *policy's* (`{"decision":"allowed","reason":"allowed, read-only"}`), because the client
gate sits after it and refuses without recording a step of its own.

That makes the sentence the **only** trace the refusal leaves, which sharpens the decision rather
than softening it: a reader of that chain has nothing else to distinguish a client that manages its
own dialogs from an operator who pressed stop. This is the reasoning slice 027's D3 recorded for
keeping the cancelled run's sentence distinct from the timeout's, applied one layer up.

**One sentence for both of D4's checks, though.** Pre-request and in-loop are the *same* fact — the
session was cancelled, so this call was refused — differing only in when the flag was observed. That
is why S6's two reverts are distinguished by an observable *request count* rather than by a message.

### D6 — The test harness is bounded first, or the red is a hang

`ask_permission` currently ends in `spawn_blocking(move || rx.recv().expect("answered"))`, and the
new test's client is one that **never answers**. Against today's untimed harness that is not a
failing test — it is a test binary that never exits, on a suite the three gates run unattended.

So S1 lands before S2's red, and changes only the harness:

- the harness's own `rx.recv()` → `rx.recv_timeout(OBSERVE_TIMEOUT)`, with the timeout reported as
  an assertion failure naming the call that never returned;
- the agent-side future wrapped in `tokio::time::timeout(OBSERVE_TIMEOUT, …)`, because the harness's
  thread is not the only place a never-answered request can wedge — `connect_with` awaits the whole
  closure;
- a third `PermissionOutcome` variant for a client that receives the request and drops the responder
  without answering, plus an optional "set this flag once the request has been seen" hook so the
  cancellation is triggered by *delivery* rather than by a guess about timing (the property
  `Observed::wait_for_chunks` already exists to give the streaming tests).

S1 is verified by `p1`, `p2` and `p3` passing **unmodified in behaviour** through the new harness:
the bound must not change what an answered request does.

### D7 — `heddle-core`, `heddle-cli`, and the wire are untouched

No port gains a method, no `StepKind` is added, no payload shape changes, no CLI argument appears, no
`Cargo.toml` changes. The diff is one file of production code, one field, one loop, one enum, one
string, two doc comments, and the tests.

---

## 3. Steps

- **S0** fast-forward onto `dev` at `a8a9d5e`, measure the control baseline, write
  `specs/028-cancel-permission-wait/{spec.md,plan.md}`. *(`tasks.md` is S8's close-out.)*
- **S1** harness — bound `ask_permission`'s wait (`recv_timeout`, `tokio::time::timeout`), add the
  never-answering client and the on-delivery flag hook. **Must land before S2**, or S2's red is a
  hung test binary rather than a recorded failure. Verified by `p1`/`p2`/`p3` still passing.
- **S2** RED — the two new unit tests: a flag set *while* a permission request is outstanding refuses
  the call fast, with the new sentence, without reaching the inner transport (FR-003…006); and a flag
  already set refuses **without sending a request at all** (FR-002). Both reds recorded verbatim.
- **S3** GREEN — `permission.rs`: the fourth field, `POLL_SLICE`, the `Answer` enum, the polling loop
  with the exhaustive `RecvTimeoutError` match, the two checks, the new refusal; and the fourth clone
  at `lib.rs:103`.
- **S4** controls — the properties D2's loop puts at risk and nothing else would catch: a client that
  answers only after **many** poll slices, flag never set, is still honoured (FR-007 — the
  `Disconnected`-collapse mistake); and a connection that dies under the open question ends the wait
  and is reported as neither cancellation (FR-008, **as corrected by its own measurement** — see
  D2). Plus `p1`/`p2`/`p3` unmodified.
- **S5** docs — `cancel.rs`'s module doc ("two others, all three") and `SessionParts.cancelled`'s doc
  ("three places") are both now wrong: there is a fourth reader. Not compiler-checked, and required.
- **S6** RED-by-revert, **both directions**: remove the pre-request check and record its red; restore;
  remove the in-loop check and record its red; restore. Neither check is redundant and this is the
  proof.
- **S7** live hand-verification on this machine, timestamped: `heddle acp-agent --allow-run` against
  the real provider, a real permission request left unanswered, a real `session/cancel`, and the
  prompt answered `Cancelled` while the dialog is still open. Part of this run **if a real provider
  is available**; if not, that is recorded as not-done rather than reported as done.
- **S8** close-out: `tasks.md` with the reds verbatim, the live run, the deviations and the
  residuals; the three gates green.

## 4. Validation

| gate | command |
|---|---|
| format | `cargo fmt --all -- --check` |
| lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| tests | `cargo test --workspace` |

The control baseline is the same three gates measured immediately after the fast-forward, before any
edit.

## 5. Risks and rollback

- **The `Disconnected` collapse.** A wildcard arm returning the closed-connection refusal ends every
  permission request one poll slice after it is asked. It is the mistake a mechanical conversion of
  the existing line produces, and it would pass nothing: `p1` would fail. S4's slow-answer control is
  the assertion that pins it deliberately rather than incidentally.
- **The `Timeout` collapse.** A wildcard arm continuing the loop spins forever on a `Disconnected`
  channel. S4's measurement showed a *closed connection* is not that channel state, so this risk is
  now guarded only by the compiler's exhaustiveness — which is precisely why no `_` arm exists to
  waive it. Recorded as a residual in `tasks.md` rather than claimed as tested.
- **A red that is a hang.** S1 before S2, non-negotiable, and FR-011 states it as a requirement of
  the suite rather than as a convention of this slice.
- **The flag not being reset.** Unchanged and not this slice's: `HeddleSession::run` already clears it
  per run, and the gate reads the same `Arc`, so a cancelled prompt does not poison the next one.
  `a6` (two prompts in one session) is the existing control.
- **Rollback** is the revert of one field, one loop, one enum and one string. No wire format, no
  chain payload, no `StepKind`, no CLI flag, no port and no dependency changes.

## 6. Out of scope

- **Any timeout on a human's decision** (D2; spec rejected alternative 1).
- **Withdrawing the request on the wire.** ACP has no agent-initiated cancel for an outstanding
  `session/request_permission`. Heddle stops waiting; a late answer is dropped with the channel.
- **`heddle chat`.** No permission gate exists there at all.
- **Cancelling a non-process tool already executing.** Unchanged from slice 027.
