# Tasks: cancelling while a permission request is outstanding (v0 slice)

**Spec:** `specs/028-cancel-permission-wait/spec.md` · **Plan:**
`specs/028-cancel-permission-wait/plan.md` · TDD (red→green), branch `028-cancel-permission-wait`,
fast-forwarded onto `dev` at `a8a9d5e`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)

- **I Headless core** ✅ no new command, no new flag, no new argument, no output change. `heddle chat`
  is untouched — it has no permission gate at all. `heddle ledger log`/`show`/`verify` render a run
  cancelled under an open question with zero CLI change, read back from a live one below.
- **II Local-first** ✅ no new dependency, no `Cargo.toml` change, no socket, no second process, no
  new thread. The slice replaces one blocking `recv` with a `recv_timeout` loop: one atomic load and
  one timed wait per 50 ms of a wait that already existed, on a thread whose only other activity was
  being blocked.
- **III Test-First** ✅ four reds observed and recorded verbatim below. Red A is the slice's own
  behavioural red, and it is a *recorded failure* rather than a hang only because S1 landed first.
  Reds B and C are the two-directional revert proving neither cancellation check is redundant, and
  red B **caught a hollow assertion in red A's own test** — see *Deviations* 2. Red D reverts D2's
  exhaustive match and shows that `p1`/`p2`/`p3` do not notice.
- **IV Inverted coupling** ✅ `heddle-core` is **untouched**. `ToolTransport` is not widened; no port
  gains a method. `heddle-cli` is untouched too, which is the dividend of slice 027 having moved the
  flag into `SessionParts` first: the composition root learns nothing new, and there is no second
  `Arc` to wire wrongly.
- **V Traceability** ✅ no `StepKind`, no payload shape and no `WireExchange` field is added. The
  refusal reaches the chain the way every `ToolDenied` already does — as a tool-role message in the
  next turn's `llm_request` payload — and the run's chain verifies. Measured live, not assumed:
  see *Deviations* 3.
- **VI Security** ✅ strictly narrowing. The change can only cause **fewer** tool calls to run and
  **fewer** questions to be put in front of a person; there is no path by which a set flag lets
  anything through that would not have gone through. The policy gate ahead of it, the allowlist and
  the redactor are untouched.
- **VII Neutrality** ✅ one `const`, one field, one two-variant private enum, one loop, one string,
  two doc comments. Ten alternatives are rejected with a reason each in `spec.md`, including the two
  the shape most invites (a second channel selected over; a timeout on the request itself).
- **VIII Loop discipline** ✅ NON-NEGOTIABLE and unchanged. A refused tool call is a `ToolDenied`,
  which `NativeLoop::mediate` already adjudicates; the controller, budget, probe and exit conditions
  are neither read nor written. Nothing half-finished is laundered into a success — the tool never
  ran at all.
- **Cross-platform** ✅ nothing platform-specific is added. `permission.rs` names no OS API, and
  every new test runs on every platform.

## Tasks

- [x] **S0** fast-forwarded onto `dev` at `a8a9d5e`, control baseline measured, `spec.md` and
      `plan.md` written
- [x] **S1** harness bounded **before** the red that needs it — `recv_timeout(OBSERVE_TIMEOUT)` on
      the call, `tokio::time::timeout(2 × OBSERVE_TIMEOUT)` on the connection future. Verified by
      `p1`/`p2`/`p3` passing through the bound unchanged
- [x] **S2** RED — a session cancelled while the request is outstanding, and one cancelled before the
      call (red A)
- [x] **S3** GREEN — `permission.rs`: the fourth field, `POLL_SLICE`, the `Answer` enum, the polling
      loop with the exhaustive `RecvTimeoutError` match, the two checks, the new refusal; and the
      fourth clone in `HeddleSession::new`
- [x] **S4** controls — an answer given after twelve poll slices is still honoured; a connection that
      dies under the open question ends the wait and is reported as neither cancellation. The second
      **corrected FR-008 as drafted** — see *Deviations* 1
- [x] **S5** docs — `cancel.rs`'s module doc and `SessionParts.cancelled`'s doc comment, both of
      which said "three" readers and now say four. Not compiler-checked, and required
- [x] **S6** RED-by-revert, **both directions** (reds B and C), plus D2's wildcard collapse in both
      of *its* directions (red D). Every revert restored
- [x] **S7** live hand-verification — **part of this run**, against the real Ollama, the real binary,
      and a permission request left genuinely unanswered
- [x] **S8** close-out

## Control baseline (S0)

Measured on this worktree immediately after the fast-forward to `a8a9d5e`, before any edit:

| gate | result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo test --workspace` | **279 passed, 0 failed** |

## The reds

### Red A (S2) — the slice's reason to exist

`p5_a_session_cancelled_while_the_request_is_outstanding_denies_the_call` and
`p6_a_session_cancelled_before_the_call_never_asks_the_client`, against the unmodified
`AcpPermissionTransport`:

```
running 5 tests
test p2_a_reject_answer_denies_without_reaching_the_transport ... ok
test p3_a_cancelled_answer_denies_without_reaching_the_transport ... ok
test p1_an_allow_answer_reaches_the_inner_transport ... ok
test p5_a_session_cancelled_while_the_request_is_outstanding_denies_the_call ... FAILED
test p6_a_session_cancelled_before_the_call_never_asks_the_client ... FAILED

---- p5_… stdout ----
panicked at crates\heddle-acp\tests\acp_session.rs:1160:16:
`AcpPermissionTransport::call` returned: Timeout

---- p6_… stdout ----
panicked at crates\heddle-acp\tests\acp_session.rs:1160:16:
`AcpPermissionTransport::call` returned: Timeout

test result: FAILED. 3 passed; 2 failed; … finished in 10.01s
```

**`Timeout` is the harness's bound, not the product's** — there is no timeout on a permission request
and this slice does not add one. That the red reads as a *failure at 10.01 s* rather than as a test
binary that never exits is exactly what S1 bought, and it is why S1 had to land first.

### Red B (S6) — the pre-request check removed

The `if self.cancelled.load(…)` above `send_request` deleted, everything else intact. **First
attempt: `p6` passed anyway.** That is the finding, not a formality — see *Deviations* 2. With the
assertion repaired:

```
test p6_a_session_cancelled_before_the_call_never_asks_the_client ... FAILED
test p5_a_session_cancelled_while_the_request_is_outstanding_denies_the_call ... ok

panicked at crates\heddle-acp\tests\acp_session.rs:1333:5:
assertion `left == right` failed: a cancelled session raised a permission request anyway
  left: 1
 right: 0
```

`p5` **passes** under this revert. The in-loop check cannot do the pre-request check's job.

### Red C (S6) — the in-loop check removed

The pre-request check restored, the `if self.cancelled.load(…)` at the top of the loop deleted:

```
test p6_a_session_cancelled_before_the_call_never_asks_the_client ... ok
test p5_a_session_cancelled_while_the_request_is_outstanding_denies_the_call ... FAILED

panicked at crates\heddle-acp\tests\acp_session.rs:1205:16:
`AcpPermissionTransport::call` returned: Timeout

test result: FAILED. 1 passed; 1 failed; … finished in 10.01s
```

`p6` **passes** under this revert. Neither check is redundant, in either direction, and the two reds
are distinguished by *what was observed* — a request count versus a wait that never ended — rather
than by the refusal, which is deliberately one sentence for both (D5).

### Red D (S6) — D2's exhaustive match collapsed to a wildcard, in both directions

**Toward `Disconnected`** (`Err(_) => return Err(HeddleError::Tool("acp connection closed"))`, which
is what a mechanical conversion of the old untimed line produces):

```
test p1_an_allow_answer_reaches_the_inner_transport ... ok
test p3_a_cancelled_answer_denies_without_reaching_the_transport ... ok
test p2_a_reject_answer_denies_without_reaching_the_transport ... ok
test p8_a_connection_that_closes_under_the_question_ends_the_wait ... ok
test p6_a_session_cancelled_before_the_call_never_asks_the_client ... ok
test p7_an_answer_given_after_many_poll_slices_is_still_honoured ... FAILED
test p5_a_session_cancelled_while_the_request_is_outstanding_denies_the_call ... FAILED

---- p7_… stdout ----
panicked at crates\heddle-acp\tests\acp_session.rs:1287:38:
the late answer was honoured: Tool("acp connection closed")
```

**`p1`, `p2` and `p3` do not notice.** Their client answers inside the first 50 ms slice, so the bug
that refuses every permission request in the product one poll after it is asked ships with five of
seven green. `p7` — an answer given after twelve slices — is the only test that sees it, which is the
whole reason S4 wrote it.

**Toward `Timeout`** (`Err(_) => continue`):

```
test result: ok. 7 passed; 0 failed; … finished in 0.61s
```

**Nothing notices**, and that is recorded rather than glossed: a `Disconnected` channel state is not
what a closed ACP connection produces (*Deviations* 1), so no test in this suite can provoke the
spin. That direction is guarded by the compiler's exhaustiveness alone — which is precisely why
`permission.rs` has no `_` arm to waive it, and why D2 states the reasoning in the code rather than
only here.

## Live verification (S7)

`2026-09-04T07:53:09Z` — `2026-09-04T07:53:33Z`. The real `target/debug/heddle.exe acp-agent` spawned
as a subprocess and driven over its actual stdio with newline-delimited JSON-RPC, against the real
`gemma4:latest` on `http://localhost:11434/v1`. The model — not a stub — chose the tool call. The
permission request it raised was **never answered by the client**; a `session/cancel` was sent
instead:

```
start 2026-09-04T07:53:09.429194+00:00
[  0.03s]   stderr: serving acp on stdio: silo live028 at http://localhost:11434/v1
[  0.05s] initialized
[  0.05s] session = heddle-1
[  0.05s] ==> session/prompt sent
[ 24.25s] <== session/request_permission for fs_read (id a9d399f3-…) — LEFT UNANSWERED
[ 24.25s] ==> session/cancel sent
[ 24.33s] <== session/prompt RESPONSE {'stopReason': 'cancelled'}
[ 24.33s]     63 ms after the cancel
[ 24.33s] permission requests seen: 1, answered by this client: 0
[ 24.33s] agent exited rc=0

stopReason = 'cancelled'
cancel -> response = 63 ms
```

**Without this slice that run does not end.** This is a stronger statement than slices 026 and 027
could make: there, the cancel was late by a stream or by `RUN_TIMEOUT`. Here the wait it replaces has
**no upper bound at all** — the run would have stayed open for as long as the question went
unanswered, which is forever if nobody ever answers it.

The 63 ms spans a cancel notification crossing a real pipe, the poll slice, the refusal, the loop's
turn boundary, `CancellableModel`'s pre-turn refusal, and the response crossing back.

The chain that run left, read back by a second process:

```
> heddle ledger log --root … --silo live028 --run "heddle-1#1"
heddle-1#1  0  iteration_boundary  1d470fb7…
heddle-1#1  1  llm_request         88615743…
heddle-1#1  2  wire_exchange       ba105527…
heddle-1#1  3  llm_response        e8336832…
heddle-1#1  4  budget_spent        a14a622c…
heddle-1#1  5  tool_call           fc6ed669…
heddle-1#1  6  approval            0cb26630…
heddle-1#1  7  iteration_boundary  bc1d90b7…
heddle-1#1  8  llm_request         94510472…

> heddle ledger verify --root … --silo live028
heddle-1#1  ok  9 steps
```

Nine steps, verifying, and the **absence** is the story: there is a `tool_call` and an `approval` but
**no `tool_result`**, because the tool never ran. The `approval` is the *policy's*
(`{"tool":"fs_read","decision":"allowed","reason":"allowed, read-only"}`) — the client gate sits
after it and refuses without a step of its own.

D5's sentence reached the chain verbatim, in step 8's payload, which is where a `ToolDenied` lands:

```json
{"role": "tool", "tool_call_id": "call_q0z6usk3",
 "parts": [{"type": "text",
            "text": "the fs_read tool call was refused: session cancelled while awaiting acp permission"}]}
```

That is the live proof of D5 and of Constitution V together: the model was told *which* cancellation
happened, in the words the slice chose, and a chain reader is told the same thing from the same
payload.

## Deviations from the plan

1. **FR-008 was wrong as drafted, and S4 measured it.** The plan asserted that a closed connection
   arrives as `RecvTimeoutError::Disconnected` and is refused as `"acp connection closed"`. It is
   not: ACP **invokes** a pending `on_receiving_result` callback with an `Err` rather than dropping
   it, so a dead transport arrives down the answer channel *as an answer*:

   ```
   panicked at crates\heddle-acp\tests\acp_session.rs:1287:5:
   expected the closed-connection refusal, got Tool("acp permission request failed: \
   Incoming transport closed: {\n  \"reason\": \"incoming_transport_closed\",\n  \
   \"method\": \"session/request_permission\"\n}")
   ```

   `p8` now asserts the observable requirement — the wait ends, and is reported as neither
   cancellation — and `spec.md`, `plan.md` and the code comment all record the measurement. The
   `Disconnected` arm keeps the message it had before this slice and **cannot** be removed:
   `recv_timeout` has two error variants, and this slice waives neither.

2. **Red B found a hollow assertion in red A's own test.** With the pre-request check removed, `p6`
   passed. `call` returns from the loop's first flag check before the client task is next polled, so
   `requests()` read 0 whether or not a request had been put on the wire — the assertion was winning
   a race, not observing a property. A request already sent is still a question a person is about to
   see, so the harness now gives delivery a bounded 250 ms chance before the count is believed. Only
   the negative case pays it. **This is what S6 is for**: without the revert, a test asserting the
   absence of something would have shipped green and meaningless.

3. **D5's claim about *where* the refusal lands was wrong, and S7 measured it.** The plan said both
   cancellations "land on the chain as an `Approval` step and a `ToolResult` payload". A `ToolDenied`
   leaves no `ToolResult` step at all — `NativeLoop::mediate` (`native_loop.rs:191-194`) turns it
   into a tool-role message in the **next** turn's `llm_request` payload. Corrected in `plan.md` and
   `spec.md`. It sharpens the decision rather than softening it: that sentence is the *only* trace
   the refusal leaves.

4. **S7 used `fs_read` under `--fs-root`, not `proc_run` under `--allow-run`.** Every tool call that
   passes the policy goes through `AcpPermissionTransport::call`, so any approved tool raises the
   question this slice is about, and `fs_read` raises it without granting an AppContainer identity a
   lasting entry on a directory's ACL. The narrower privilege produces the same evidence.

5. **A third revert was added to S6** (red D, both directions). The plan asked only for the
   two-directional check revert. D2's exhaustive match is this slice's other load-bearing decision
   and the revert is what turns "a wildcard would be a bug" from an argument into a measurement —
   including the half of it that nothing can measure.

6. **The harness's `PermissionOutcome` became `ClientScript`.** Three of its variants no longer
   produce an outcome — they model a client that does not answer — so the old name would have been
   false. The three answer paths are unchanged in behaviour.

## Residuals

- **`RecvTimeoutError::Disconnected` is unreachable through any path this suite can provoke.** It
  needs a callback dropped uninvoked, and ACP invokes them. The arm exists because the match is
  exhaustive, and it keeps the pre-existing message; red D's second direction records that nothing
  would notice if it were folded into `Timeout`. Guarded by the compiler alone, by design.
- **`list` still ends in an untimed `rx.recv()`** in its own test harness
  (`list_through_permission`). It asks no permission, so it has no unanswered question to wait on;
  bounding it would be churn outside this slice.
- **ACP has no agent-initiated withdrawal** of an outstanding `session/request_permission`. Heddle
  stops waiting; the client is left to notice the session it cancelled, and an answer that arrives
  later is dropped with the channel. An editor that leaves the dialog on screen after its own cancel
  is displaying a question nobody will read — a client-side concern, and out of scope.
- **No timeout on a human's decision**, deliberately (spec rejected alternative 1).
- **`heddle chat`** has no permission gate and no cancel channel; slices 026 and 027 recorded the same
  boundary.

## Validation

| gate | command | result |
|---|---|---|
| format | `cargo fmt --all -- --check` | clean |
| lint | `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| tests | `cargo test --workspace` | **283 passed, 0 failed** |

Four tests more than the S0 baseline's 279: `p5`, `p6`, `p7`, `p8`.
