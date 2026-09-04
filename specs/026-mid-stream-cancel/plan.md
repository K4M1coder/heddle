# Plan — slice 026: mid-stream cancellation

**Target artifacts:** `specs/026-mid-stream-cancel/{spec.md,plan.md,tasks.md}` plus the code changes
below. **Branch:** `026-mid-stream-cancel`, cut from `dev`. **No PR** (the bare mirror at
`D:/claudecode/skein-origin.git` exists only for Archon's worktree isolation). Conventional Commits.
Strict TDD (Constitution III): red before green.

---

## 0. Read this first — the tree, and what ureq actually does

### 0.1 The worktree is stale. Fast-forward before anything else.

`HEAD` here was `d364405`; `dev` is `2806ecf`, **30 commits ahead**, and everything this slice
touches — `TextSink`, `AcpTextSink`, `drain`, `StreamFault`, `WireExchange.streamed` — arrived with
slice 025 and is absent from `d364405`. **T0 of the implementation run: fast-forward
`026-mid-stream-cancel` onto `dev` at `2806ecf` and re-measure the control baseline**
(`cargo test --workspace`). Every anchor named below is a `dev` anchor.

### 0.2 Anchors verified on `dev` at `2806ecf`

| anchor | file | fact |
|---|---|---|
| `TextSink` | `skein-core/src/model.rs` | one method, `on_text(&mut self, delta: &str)`; `Send` |
| `ModelClient::set_text_sink` | `skein-core/src/model.rs` | defaulted to dropping the sink |
| `drain` | `skein-gateway/src/lib.rs` | `loop { read_until(b'\n') … }`, one line per iteration |
| `StreamFault` | `skein-gateway/src/lib.rs` | private enum, two variants: `Unreadable`, `Unparseable` |
| the fault match | `skein-gateway/src/lib.rs`, in `turn` | sits **above** `if answer.events == 0` |
| `WireExchange` | `skein-core/src/model.rs` | `url`, `status`, `request`, `response`, `streamed` |
| `AcpTextSink` | `skein-acp/src/stream.rs` | holds connection, session id, redactor, `emitted` |
| `SkeinSession::new` | `skein-acp/src/lib.rs` | holds the `Arc<AtomicBool>` **and** builds the sink |
| `SkeinSession::run` | `skein-acp/src/lib.rs` | resets the flag per run; maps a set flag to `StopReason::Cancelled` |
| `CancellableModel` | `skein-acp/src/cancel.rs` | checks the flag **before** delegating `turn` |
| `a7_…`, `x1_…` | `skein-acp/tests/acp_session.rs` | the existing cancellation proofs |

### 0.3 What ureq 3.4.0 does with a half-read body — read from its source, not assumed

Resolved version is `ureq v3.4.0` (`cargo tree -p skein-gateway -i ureq`).

1. `ureq-3.4.0/src/pool.rs:125` — `Connection::reuse` is the only path back into the agent's pool.
2. `ureq-3.4.0/src/run.rs:610` — `cleanup(connection, must_close, now)` is the **only** caller of
   `reuse`, and its own callers are the response-complete paths and `BodyHandler::ended`.
3. `ureq-3.4.0/src/run.rs` — `BodyHandler::ended` is reached only once the body is fulfilled
   (content-length met, terminal chunk seen, or remote closed).
4. `grep -rn "Drop" ureq-3.4.0/src/` — **no `Drop` impl anywhere in the crate.** Cleanup is
   ownership, not a destructor hook: `Connection` owns `Box<dyn Transport>` owns the socket.
5. `ureq-3.4.0/src/body/mod.rs:68-72`, "Pool reuse" — *"To return a connection to the Agent's pool,
   the body must be read to end."*

**Consequence:** abandoning the reader mid-stream drops `Response` → `BodyHandler` → `Connection` →
the socket, which sends FIN. The connection is not pooled, the provider sees the peer go away, and
generation stops. There is nothing for this slice to build.

---

## 1. Problem

Slice 025 made the provider stream and made assistant text reach an ACP client as it is produced, and
closed with the residual it created:

> **Mid-stream cancellation is now possible and is not implemented.** `CancellableModel` still checks
> before a turn, and its docstring's *"a model call already in flight completes"* is still true.

So `session/cancel` arriving while a 300-token answer is being written does nothing until the model
finishes it. The editor's stop button is a request to stop *after* the current turn. Meanwhile the
run keeps spending the provider's tokens on an answer nobody will read, and the socket stays open for
the whole of it.

## 2. Approach

### D1 — The port grows one defaulted method: `TextSink::wants_more(&self) -> bool`

```rust
pub trait TextSink: Send {
    fn on_text(&mut self, delta: &str);

    /// Whether the caller still wants what this turn has left to say.
    fn wants_more(&self) -> bool { true }
}
```

Defaulted to `true` because that is the *true* answer for a sink with nothing to cancel, in the same
way `take_wire_exchange`'s `None` and `set_text_sink`'s drop are true answers rather than
conveniences. `skein-core` does not learn what cancellation is; it learns that a consumer may stop
wanting text.

**Rejected — `on_text` returns `bool`.** It conflates "here is text" with "keep going", changes the
signature of every existing sink, and — the deciding reason — can only be asked when a content delta
arrives. A cancel landing during a run of tool-call fragment events, or during a reasoning model's
~150 empty-content events, would not be seen until the next non-empty delta.

**Rejected — thread an `Arc<AtomicBool>` into `OpenAiCompatClient`.** That makes the gateway name a
cancellation mechanism owned by ACP, and gives `skein chat` — which installs no sink — a flag it has
no way to set. The sink is *already* the one object the caller owns on the far side of the port.

**Rejected — a `Cancellable` trait separate from `TextSink`.** Two ports where one suffices
(Constitution VII). Every caller that can cancel mid-stream is by construction a caller that is being
streamed to.

### D2 — The check is the first thing in the drain loop, before the blocking read

```rust
loop {
    if sink.as_ref().is_some_and(|s| !s.wants_more()) {
        answer.fault = Some(StreamFault::Cancelled);
        break;
    }
    line.clear();
    match reader.read_until(b'\n', &mut line) { … }
}
```

Per **line**, not per event: a cancel is noticed on the next line of any kind. `is_some_and` on the
`Option<Box<dyn TextSink>>` makes "no sink installed" mean "nobody to cancel", which is exactly
`skein chat`.

**Known and accepted: a cancel cannot interrupt a `read_until` already blocked.** It takes effect
when the next line lands. On a live token stream that is one token — measured in S8. On a provider
that has stopped writing entirely it is the client's global timeout, which is the same budget that
governed that case before this slice. Closing the gap properly needs a non-blocking read, which needs
a custom `Connector`/`Transport` — see D5.

### D3 — `StreamFault::Cancelled`, raised out of `turn` **before** the `events == 0` check

```rust
Some(StreamFault::Cancelled) => {
    return Err(SkeinError::Model(format!(
        "{} stopped mid-stream: the client cancelled the turn",
        self.endpoint.base_url
    )))
}
```

It goes in the existing fault match, which already sits above `if answer.events == 0`. **That
ordering is the decision, not an accident of where the code is.** A cancellation that lands before
the first event leaves `answer.events == 0`, and the `events == 0` guard's diagnostic — *"no SSE
events: <body>"* — was written for an interposing proxy's HTML page. Reporting the operator's own
stop button as a phantom proxy is the failure this ordering prevents, and S2's test pins it by
cancelling before the first event.

An error, not a truncated success: a partial answer returned as a `TurnResponse` would flow into
`LoopController::should_exit` and be adjudicated as if the model had said it (Constitution VIII(a)),
and the partial text would land on the chain as an `LlmResponse` the model never completed. The
error path already exists and already leaves the chain verifiable — `NativeLoop::run` appends the
`WireExchange` step *before* `resp?`, which slice 023 built for exactly this.

### D4 — No new `WireExchange` field. The bytes are already the evidence.

A cancelled stream's captured `response` **ends without `data: [DONE]`**, because the read stopped
before it. Adding `cancelled: bool` would put a *claim about* the bytes beside the bytes — the same
thing slice 025 refused when it recorded the raw SSE rather than the reassembled object
(Constitution V). `streamed: true` still holds and still means what it meant.

The distinction the field would buy — cancellation versus a socket that failed — is not the
`WireExchange`'s to make: `WireExchange` records what crossed the wire, and *why the read stopped* is
the run's outcome, which the chain records separately (the run ends with no `Exit` step) and which
ACP reports as `StopReason::Cancelled`.

### D5 — No `Connector`, no `Transport`, no cancellation-aware socket. Explicitly rejected.

ureq exposes `Connector`/`Transport` for exactly this kind of substitution, and this slice **does not
build one.** §0.3 shows why it would be machinery with nothing to do: dropping the half-read reader
already closes the socket and already keeps the connection out of the pool, because ureq cleans up by
ownership rather than by a destructor hook.

What a custom transport *would* buy is D2's residual — interrupting a blocked read rather than
waiting for the next line. That is a latency of one token on a live stream, against a bespoke
transport in the one crate the Constitution most wants small, plus a second socket lifecycle to keep
correct. Not this slice, and not until something measures the latency as a real cost
(Constitution VII).

### D6 — `AcpTextSink` answers `wants_more` from the flag the session already holds

`SkeinSession::new` is the one place holding both the `Arc<AtomicBool>` and the sink's construction,
so the sink takes a clone of the flag it already resets per run. No new state, no second channel, no
change to how `session/cancel` is received or to how `StopReason::Cancelled` is decided —
`SkeinSession::run`'s existing "flag set ⇒ `Cancelled`" mapping is what turns D3's error into the
right stop reason, unchanged.

`CancellableModel` is **not** touched, beyond its docstring: its pre-turn check is still the right
behaviour for the turn that has not started, and its claim *"a model call already in flight
completes"* becomes false and must be corrected.

---

## 3. Steps

- **S0** fast-forward onto `dev` at `2806ecf`, measure the control baseline, write
  `specs/026-mid-stream-cancel/{spec.md,plan.md,tasks.md}`.
- **S1** RED — `skein-gateway/tests/openai_compat.rs`: a sink that stops wanting text ends the read
  mid-stream; the turn fails naming cancellation; the capture holds what arrived and no `[DONE]`; a
  plain sink that never overrides `wants_more` still reads the whole stream.
- **S2** RED — same file: a sink that stops **before the first event** is reported as cancelled, not
  as an unrecognised body (the D3 ordering).
- **S3** GREEN — `skein-core`: the defaulted `TextSink::wants_more`.
- **S4** GREEN then RED-by-revert — `skein-gateway`: `StreamFault::Cancelled`, the drain check, the
  `turn` arm. Then temporarily revert **only the drain check** and record the red, as slice 025 did
  for its D1/D2, so the red is evidence about this code rather than about its absence.
- **S5** RED — `skein-acp/tests/acp_session.rs`: a `session/cancel` arriving after the client has
  seen the first chunk ends the turn and reports `Cancelled`, with no further chunk and no
  chain-derived repeat.
- **S6** GREEN — `skein-acp`: `AcpTextSink` takes the flag and answers `wants_more`; `SkeinSession`
  hands it over; `CancellableModel`'s docstring corrected.
  **`a7_session_cancel_ends_the_run_and_reports_cancelled` and
  `x1_cancellable_model_stops_delegating_once_the_flag_is_set` are not modified** — their passing
  unchanged is the proof the pre-turn path still works.
- **S7** RED-by-revert — revert the drain check again with S5 applied and record the ACP-level red.
- **S8** live verification against the real Ollama on this machine, timestamped: a cancel mid-answer
  stops the provider, the capture ends without `[DONE]`, and the chain still verifies. **Part of this
  run.**
- **S9** close-out: `tasks.md` with the reds verbatim, the live run, the deviations and the
  residuals.

## 4. Validation

| gate | command |
|---|---|
| format | `cargo fmt --all -- --check` |
| lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| tests | `cargo test --workspace` |
| live | `$env:SKEIN_LIVE_MODEL = "gemma4:latest"; cargo test -p skein-gateway --test openai_compat -- --ignored --nocapture` |

The control baseline is the same three gates measured immediately after the fast-forward, before any
edit.

## 5. Risks and rollback

- **A `wants_more` that is asked too often.** It is asked once per line of a stream that can be tens
  of thousands of lines. It must stay an atomic load behind a trait call; anything that locks or
  allocates would be a per-line cost. Reviewed at S6.
- **Suppressing the projection for a cancelled run.** The session suppresses the chain-derived
  transcript when `streamed() > 0`, which is already true of a cancelled run — so a client that saw
  two chunks is not sent a third repeating them. S5 asserts the exact chunk sequence, which is what
  would catch a regression here.
- **Rollback** is the revert of one defaulted trait method, one enum variant, one `if` in `drain`,
  one match arm and one struct field. No wire format, no chain payload, no `StepKind` and no CLI
  surface changes, so a revert leaves no artifact behind.

## 6. Out of scope

- Interrupting a **blocked** read (D2's residual, D5's rejected machinery).
- Cancelling a **tool call** already in flight. `AcpPermissionTransport` runs a tool to completion;
  `session/cancel` during one is still observed at the next turn boundary.
- Any `skein chat` cancellation surface. `chat` installs no sink and has no cancel channel; giving it
  one is a CLI slice with its own decisions.
- Buffered redaction for the live transcript (slice 025's other standing residual).
