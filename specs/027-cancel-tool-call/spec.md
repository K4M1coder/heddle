# Feature Specification: cancelling a tool call in flight (v0 slice)

**Feature Branch:** `027-cancel-tool-call` · **Created:** 2026-09-04 · **Status:** Implemented (v0
slice) · **Input:** the residual slice 026 named in its own *Out of scope* — *"Cancelling a **tool
call** already in flight. `AcpPermissionTransport` runs a tool to completion; `session/cancel` during
one is still observed at the next turn boundary."* — and repeated as rejected alternative 8 of that
slice's spec · Constitution III (**test-first**), IV (**explicit boundaries**), VI (**security /
deny-by-default**), VII (**no capability without a real need**), VIII (**loop discipline**,
NON-NEGOTIABLE) · design §4.3 (embedded connectors), ADR-0006 (the Windows sandbox).

Slice 013 made `session/cancel` stop the *next* turn. Slice 026 made it stop the *current answer*.
Neither reaches the one thing in the product that can run for thirty seconds: a process `proc_run`
launched. Stop is pressed, the flag is set, and a `cmd.exe` keeps burning a core inside its
AppContainer until `RUN_TIMEOUT` expires — after which its output is fed into a turn that is refused
anyway. This slice makes the stop button stop the process.

## What this slice changes for a user

**The stop button kills the running command.** A `session/cancel` arriving while `proc_run` is
executing terminates the child and its whole tree within one 50 ms poll, the tool call comes back as
an error saying it was cancelled, and `session/prompt` is answered `StopReason::Cancelled`.

**The wait is bounded by the button, not by the budget.** Before this slice the shortest possible
answer to a mid-`proc_run` cancel was however long the command had left, up to thirty seconds. It is
now a poll slice.

**Nothing else about a run changes.** `skein chat` is byte-identical — it has no cancel channel and
passes a flag nothing sets. A run that is not cancelled runs exactly as long as it ran before, exits
with the same code, and reports the same two streams. No flag is added to any command, no `StepKind`
is added, no chain payload changes, and the timeout keeps its own distinct message.

## Four things a reader must know up front

1. **The signal is the flag the session already has, threaded down by value.** No new port, no
   widening of `ToolTransport`, no decorator. `skein-cli`'s ACP session factory mints one
   `Arc<AtomicBool>` per session and hands the **same** one to the session and to the tool
   transport; it flows `ToolArgs::transport` → `local_connector_with_run` →
   `EmbeddedServer::with_run` → `run::execute` → `Sandbox::run` → `launch::wait`.
2. **The kill is the kill that was already there.** `launch::wait` becomes a loop over 50 ms slices;
   a cancelled run and a timed-out run both reach the same `TerminateProcess`, and both still have
   the rest of their tree killed by the same `drop(job)` in `run`. There is no second kill path, and
   a cancelled child dies exactly as thoroughly as a timed-out one.
3. **A cancelled `proc_run` is a tool error, and it says which of the two happened.** It takes the
   path `RUN_TIMEOUT` already takes — an `Err` rmcp reports as `isError: true` and
   `NativeLoop::mediate` survives — with a different sentence. Reporting a cancellation as a timeout
   would be a wrong answer in a right answer's shape.
4. **The composition root is where this can silently go wrong, so it has its own test.** Wire two
   different `Arc`s and *nothing fails loudly*: the run still ends `Cancelled`, thirty seconds later.
   Only an assertion on elapsed wall clock catches it, and only a deliberate two-`Arc` sabotage
   proves that assertion works. Both are in this slice.

## Requirements

- **FR-001** `SessionParts` MUST carry the cancellation flag, and `SkeinSession` MUST use the one it
  is given rather than minting its own. The flag MUST still be reset at the start of each run.
- **FR-002** `Sandbox::run` MUST take a cancellation flag and MUST observe it while the child runs.
- **FR-003** Observing a set flag MUST terminate the child and its whole process tree, by the same
  mechanism a timeout does.
- **FR-004** FR-003 MUST happen within 50 ms of the flag being set, plus the time the kill itself
  takes.
- **FR-005** A cancelled run MUST return an `Err` whose message names the cancellation, distinct from
  the timeout's message. It MUST NOT return an exit code as if the process had finished.
- **FR-006** The timeout path MUST be unchanged in behaviour and in message: a run that outlives
  `RUN_TIMEOUT` with the flag never set MUST still be refused as a timeout, MUST NOT be reported as
  cancelled, and MUST NOT be refused early because the wait is now sliced.
- **FR-007** A run that outlives several poll slices but finishes inside its budget MUST return its
  real exit code and its real streams.
- **FR-008** `EmbeddedServer` MUST make "a sandbox without a cancel channel" and "a cancel channel
  with no sandbox" unrepresentable.
- **FR-009** `skein acp-agent`'s session factory MUST give the session and its tool transport the
  **same** flag. A test MUST fail if two are wired, and MUST fail on elapsed time rather than on the
  stop reason.
- **FR-010** `skein chat` MUST be behaviourally unchanged, with no cancellation surface added.
- **FR-011** The two existing cancellation paths MUST be unchanged: `CancellableModel`'s pre-turn
  refusal and `AcpTextSink::wants_more`'s per-line check, with their tests passing unmodified.

## Rejected alternatives

| # | alternative | why not |
|---|---|---|
| 1 | widen `ToolTransport` with `cancel()` | `skein-core` would name cancellation twice in two shapes, for one implementation out of four; and it cannot work — `AcpPermissionTransport::call` holds `&mut self` for the whole call, so no `&mut` is left for a canceller to reach through |
| 2 | a `CancellableTransport` decorator mirroring `CancellableModel` | the mirror is superficial: `CancellableModel` refuses *before* delegating, which is meaningful; the defect here is *inside* a call already delegated, past two protocol boundaries and a tokio runtime |
| 3 | store the flag on `Sandbox` | `EmbeddedServer` is `Clone` and rmcp clones per request; and one sandbox serves every call in a session, so the channel would belong to the sandbox rather than to the run |
| 4 | `Option<Arc<AtomicBool>>` through the chain | six signatures each carrying a `None` that means what a never-set flag already means, plus a branch per poll for no behavioural difference |
| 5 | two parallel `Option` fields on `EmbeddedServer` | an invariant kept right by hand where a two-field struct keeps it by construction, which is `RunAccess::Allowed`'s recorded reasoning |
| 6 | a second waitable object + `WaitForMultipleObjects` | removes 50 ms and costs an `Event` handle per launch with creation and close on every path, in the one crate holding every `unsafe` block in the product (VII) |
| 7 | a check before `CreateProcessW` | narrows a window the permission gate already bounds, and no test could distinguish it from the check at the top of the wait |
| 8 | report a cancellation with the timeout's message | two different facts about a session, landing on the chain as the same `ToolResult` payload |
| 9 | let the cancelled run return its partial output as a success | an unfinished side effect adjudicated by `LoopController::should_exit` as a completed one (VIII(a)) |

## Out of scope

- Cancelling a tool call that is not a process. The other five tools are bounded by their own caps
  and complete in milliseconds; a 50 ms poll has nothing to interrupt.
- Cancelling while a permission request is outstanding. `AcpPermissionTransport::call` blocks on an
  untimed `recv()`; a cancel arriving then is observed when the human answers. A real gap, and a
  different one — it needs a second channel into that `recv`.
- A pre-launch check (rejected alternative 7).
- A cancellation surface for `skein chat` (slice 026 recorded the same boundary).
- Non-Windows. `Sandbox` is uninhabited off Windows and `--allow-run` is a refusal there.
