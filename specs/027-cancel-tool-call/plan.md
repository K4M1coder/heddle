# Plan — slice 027: cancelling a tool call in flight

**Target artifacts:** `specs/027-cancel-tool-call/{spec.md,plan.md,tasks.md}` plus the code changes
below. **Branch:** `027-cancel-tool-call`, cut from `dev`. **No PR** (the bare mirror at
`D:/claudecode/skein-origin.git` exists only for Archon's worktree isolation). Conventional Commits.
Strict TDD (Constitution III): red before green.

---

## 0. Read this first — the tree, and what a cancel can and cannot reach today

### 0.1 The worktree was stale. Fast-forwarded before anything else.

`HEAD` here was `d364405`; `dev` is `ac37966`, **36 commits ahead** — the run order said 32, and the
number measured on the tree is the one recorded. Everything this slice touches on the streaming side
(`TextSink::wants_more`, `StreamFault::Cancelled`, `AcpTextSink`'s flag) arrived with slices 025–026
and is absent from `d364405`. **S0 of this run: fast-forward `027-cancel-tool-call` onto `dev` at
`ac37966` and re-measure the control baseline.** Every anchor below is a `dev` anchor.

### 0.2 Anchors verified on `dev` at `ac37966`

| anchor | file | fact |
|---|---|---|
| `launch::wait` | `skein-sandbox/src/launch.rs` | one `WaitForSingleObject(process, millis)`; on anything but `WAIT_OBJECT_0` it calls `TerminateProcess` and returns the timeout refusal |
| the job drop | `skein-sandbox/src/launch.rs`, in `run` | `drop(job)` sits between `wait` and the reader joins, on the success path too |
| `Sandbox::run` | `skein-sandbox/src/lib.rs` | `(&self, exe, args, stream_cap, timeout)`; `#[cfg(not(windows))]` twin is `match self.0 {}` |
| `run::execute` | `skein-connectors/src/run.rs` | `resolve_exe` then `sandbox.run(&exe, args, RUN_OUTPUT_BYTE_CAP, RUN_TIMEOUT)` |
| `EmbeddedServer.sandbox` | `skein-connectors/src/server.rs` | `Option<Arc<Sandbox>>`, `Some` exactly when the `proc_run` route is enabled |
| `EmbeddedServer::with_run` | `skein-connectors/src/server.rs` | fallible; `RunAccess::Allowed(dirs)` builds the `Sandbox` |
| `local_connector_with_run` | `skein-connectors/src/connector.rs` | `(root, run)`; owns the tokio runtime the server task runs on |
| `ToolArgs::transport` | `skein-cli/src/wiring.rs` | `(&self, run: RunAccess)`; called by `chat.rs` and by `acp.rs`'s session factory |
| `SessionParts` | `skein-acp/src/lib.rs` | seven public fields; **no** `cancelled` |
| `SkeinSession::new` | `skein-acp/src/lib.rs` | **mints** `Arc::new(AtomicBool::new(false))` itself, then clones it into `CancellableModel`, `AcpTextSink` and `Registered` |
| `SkeinSession::run` | `skein-acp/src/lib.rs` | resets the flag per run; maps a set flag to `StopReason::Cancelled` |
| the cancel notification | `skein-acp/src/lib.rs`, in `serve` | `registered.cancelled.store(true, SeqCst)` |
| the timeout proof | `skein-sandbox/tests/escape.rs:158` | `the_job_object_kills_the_tree_when_the_clock_runs_out` — **slice 019 did write one** |

### 0.3 What a `session/cancel` reaches today, and where it stops

One `Arc<AtomicBool>` per session, minted inside `SkeinSession::new`, is read from exactly two
places: `CancellableModel::turn`, **before** a turn (slice 013), and `AcpTextSink::wants_more`,
per line of the provider's stream (slice 026). Nothing between the flag and a running child process
exists at all: `AcpPermissionTransport::call` blocks the loop thread on the transport, the transport
blocks on rmcp, rmcp blocks on `proc_run`, `proc_run` blocks in `run::execute`, and `execute` blocks
in `WaitForSingleObject` for up to `RUN_TIMEOUT` — thirty seconds — with no way to be told to stop.

Slice 026 named this residual in its own *Out of scope*: *"Cancelling a **tool call** already in
flight. `AcpPermissionTransport` runs a tool to completion; `session/cancel` during one is still
observed at the next turn boundary."* That is this slice.

**What the editor sees today.** Stop is pressed during a 30-second test run. `session/cancel` sets
the flag. The child keeps running. When it finally exits, its output is fed into the next turn's
messages, `CancellableModel` refuses that turn, and `session/prompt` is answered `Cancelled` — up to
thirty seconds after the button was pressed, having spent the whole of the tool's budget on a result
nobody will read.

---

## 1. Problem

`session/cancel` cannot stop a process Skein launched. The one capability in the product that can
take thirty seconds, burn a core and touch the filesystem is the one capability cancellation does not
reach.

## 2. Approach

### D1 — One `Arc<AtomicBool>`, injected at the composition root and threaded down

The flag stops being minted inside `SkeinSession::new` and becomes an eighth field of
`SessionParts`. `skein-cli`'s `acp.rs` — the session factory, which is the one frame that builds
both the transport and the parts — mints one per session and hands the *same* `Arc` to both:

```rust
let cancelled = Arc::new(AtomicBool::new(false));
Ok(SessionParts {
    transport: tools.transport(run.clone(), cancelled.clone())?,
    cancelled,
    …
})
```

and it flows down, by value, with no new port and no new trait:

`ToolArgs::transport` → `local_connector_with_run` → `EmbeddedServer::with_run` → `run::execute` →
`Sandbox::run` → `launch::wait`.

**Rejected — widen `ToolTransport` with a `cancel()` method.** `ToolTransport` is `skein-core`'s
port and `skein-core` would then name cancellation for a second time, in a second shape, for one
implementation out of four (`NoTools`, `ConfiguredTools`, `RmcpToolTransport`, `LocalConnector`) —
three of which have nothing to cancel. Worse, it does not work: `AcpPermissionTransport::call` holds
`&mut self` for the whole tool call, so there is no `&mut` left for a canceller to reach the
transport through while a call is in flight. A shared flag needs no `&mut` and that is exactly why it
is the right shape here.

**Rejected — a `CancellableTransport` decorator, mirroring `CancellableModel`.** The mirror is
superficial. `CancellableModel` decorates the *outside* of a call and refuses to make it: its check
is meaningful precisely because it happens before delegation. The defect here is *inside* a call
already delegated, past two protocol boundaries and a tokio runtime, in a thread blocked on a Win32
wait. A decorator at the transport boundary can only check before and after — which is what today's
code already effectively does, and it is the bug.

**Rejected — a per-run `Sandbox` field.** `EmbeddedServer` is `Clone` and rmcp hands each request a
clone; a flag reachable only through a `&mut Sandbox` would need a lock that `proc_run`'s `&self`
handler cannot take. And `Sandbox` is deliberately `Send + Sync` **by construction** with no
`unsafe impl` — a stored `AtomicBool` would keep that, but it would also make the cancel channel a
property of the sandbox rather than of the run, and one sandbox serves every call in a session.

The flag is passed as `&AtomicBool` from `run::execute` inward: those frames do not extend its
lifetime and do not need to own it.

### D2 — `launch::wait` becomes a polling loop over the **existing** kill

```rust
const POLL_SLICE: Duration = Duration::from_millis(50);

fn wait(process: HANDLE, timeout: Duration, cancelled: &AtomicBool) -> Result<u32, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if cancelled.load(Ordering::SeqCst) {
            return Err(terminate(process, "the run was cancelled by the client"));
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(terminate(process, &format!("the run exceeded the … limit")));
        }
        match unsafe { WaitForSingleObject(process, slice_millis(left)) } {
            WAIT_OBJECT_0 => break,
            WAIT_TIMEOUT => continue,
            other => return Err(terminate(process, &format!("… unwaitable: {other:?}"))),
        }
    }
    …GetExitCodeProcess…
}
```

**One kill, three reasons.** `terminate` is the `TerminateProcess` call that was already there, moved
into a helper because it now has three callers instead of one; the `drop(job)` in `run` that kills
the rest of the tree is untouched and still runs on every path. **There is no second kill path**: a
cancelled run and a timed-out run die identically, by the same two mechanisms in the same order, and
differ only in the sentence the model is told.

**The deadline is absolute, computed once.** The failure mode a naive slicing loop introduces is a
timeout that never fires because each iteration passes the *full* `timeout` to
`WaitForSingleObject`. S2's control is what catches that.

**The `other` arm is new, and it protects a failure mode the loop itself creates.** Today a
`WAIT_FAILED` falls into the timeout branch and the call returns; under a loop it would spin hot
forever. It is not a second kill path — it is the same `terminate`, with an honest sentence instead
of a timeout the clock did not actually reach.

**50 ms, and the cost is stated.** Worst-case latency from `store` to `TerminateProcess` is one
slice. The cost is one atomic load and one kernel wait per 50 ms of a run's life — 600 for a run that
uses the whole 30-second budget, against a child that is executing instructions the entire time. A
shorter slice buys latency nothing measures as a cost; a longer one starts to be visible to a person
holding a stop button.

**Rejected — a second waitable object and `WaitForMultipleObjects`.** It removes the 50 ms and costs
an `Event` handle per launch, its creation and close on every path including the failure paths, and
a `SetEvent` reachable from the canceller — which is the `Arc` this slice already has, plus a handle
it would have to be given. `skein-sandbox` is the one crate in the workspace holding every `unsafe`
block in the product (Constitution VII, and the crate's own module docstring): 50 ms of latency is
the cheaper side of that trade until something measures it as a real cost.

### D3 — A cancelled `proc_run` is an `Err`, and the model is told which of the two happened

`run::execute` already turns `Sandbox::run`'s `Err` into the tool's `Err`, which rmcp reports as
`isError: true` and `NativeLoop::mediate` survives. Nothing new is built for the cancelled case: it
takes the path the timeout already takes, with a different sentence.

That the two sentences are *different* is the decision. A run the operator stopped and a run that
outlived its budget are different facts about a session, they land on the chain as different
`ToolResult` payloads, and reporting one as the other would be a wrong answer in a right answer's
shape — the same reasoning `RUN_OUTPUT_BYTE_CAP` records for labelling its truncation.

### D4 — No pre-launch check, and the residual is stated rather than papered over

The flag is read for the first time at the top of `wait`, after `CreateProcessW`. A cancel that lands
between the permission answer and the launch therefore starts a process and kills it within one poll
slice rather than never starting it.

Adding a check before `CreateProcessW` would narrow a window that the permission gate already
bounds — `AcpPermissionTransport::call` blocks until a human answers, and the cancel that matters
arrives *after* that — and it would be a second read of the flag with no test able to distinguish it
from the first. Constitution VII: not until something needs it.

### D5 — `skein chat` gets a flag nothing sets, and that is the honest answer

`ToolArgs::transport` is called from `chat.rs` too, and `skein chat` has no cancel channel: it is
non-interactive, it has no `session/cancel`, and slice 026 already recorded that giving it one is a
CLI slice with its own decisions. It passes a freshly-minted `Arc` nobody holds a second reference
to — "nothing can cancel this run" stated in the wiring rather than in a comment, the same way
`ConfiguredTools::None` states "this run has no tools".

**Rejected — an `Option<Arc<AtomicBool>>` down the chain.** Six signatures would each carry a `None`
that means the same thing the never-set flag means, and `launch::wait` would branch on it once per
poll for no behavioural difference.

### D6 — The pairing is expressed in the type system: `EmbeddedServer` holds a `Launcher`

`EmbeddedServer.sandbox: Option<Arc<Sandbox>>` becomes `launcher: Option<Launcher>`, a private
two-field struct holding the `Arc<Sandbox>` and the `Arc<AtomicBool>` together. A sandbox without a
cancel channel, and a cancel channel with nothing to launch, are both unrepresentable — which is the
reasoning `RunAccess` already records for carrying its allowlist *inside* the `Allowed` arm rather
than beside it. Two parallel `Option` fields that must agree would be a second invariant to keep
right by hand.

---

## 3. Steps

- **S0** fast-forward onto `dev` at `ac37966`, measure the control baseline, write
  `specs/027-cancel-tool-call/{spec.md,plan.md}`. *(`tasks.md` is S12's close-out.)*
- **S1** RED — `skein-sandbox/tests/escape.rs`: a flag set from another thread while a genuinely
  long-running sandboxed command runs makes `Sandbox::run` return a refusal naming cancellation,
  in far less than the timeout, and the child is gone. The command is **pinned by measurement**, with
  every rejected candidate named and its measured failure recorded.
- **S2** RED/control — the timeout path. Slice 019 **did** write one
  (`the_job_object_kills_the_tree_when_the_clock_runs_out`); what it did not write, and what D2's
  loop puts at risk, is *"the timeout still fires, still says so, and does **not** say cancelled,
  when the flag was never set"* and *"a run that outlives several poll slices but finishes inside the
  budget still returns its real exit code"*. Both are written here.
- **S3** GREEN — `skein-sandbox`: `POLL_SLICE`, the `terminate` helper, the polling `wait`, and
  `Sandbox::run`'s new parameter on both platform arms.
- **S4** RED-by-revert — remove **only** the cancellation check from the loop and record the red, so
  the red is evidence about this code rather than about its absence. Restore.
- **S5** RED — `skein-connectors/tests/run_server.rs`: an `EmbeddedServer::with_run` whose flag is
  set from another thread mid-`proc_run` returns an `Err` naming cancellation, fast; and a server
  whose flag is never set runs unchanged.
- **S6** GREEN — `skein-connectors`: the `Launcher` struct, `run::execute`'s parameter,
  `EmbeddedServer::with_run`'s parameter, `local_connector_with_run`'s parameter.
- **S7** RED — `skein-acp/tests/acp_session.rs`: `SessionParts` carries the flag, and a session built
  from a caller-supplied flag hands that same flag to the tool side. `a7_…`, `a13_…` and `x1_…` are
  **not modified** beyond the new field — their passing unchanged is the proof the pre-turn and
  mid-stream paths still work.
- **S8** GREEN — `skein-acp`: `SessionParts.cancelled`, `SkeinSession::new` consuming it instead of
  minting one.
- **S9** GREEN — `skein-cli`: `ToolArgs::transport`'s parameter, `chat.rs`'s never-set flag,
  `acp.rs`'s one-per-session mint handed to both sides.
- **S10** RED-by-sabotage — the composition-root wiring test, in
  `skein-cli/tests/cli_acp_agent.rs`: a real ACP client drives the real binary with `--allow-run`,
  approves a `proc_run` of the long-running command, then sends `session/cancel`; the prompt must be
  answered `Cancelled` **in far less than `RUN_TIMEOUT`**. The red is obtained by deliberately wiring
  **two different `Arc`s** in `acp.rs` — the one composition-root mistake nothing else in this slice
  can catch, because with two flags the run still ends `Cancelled`, just thirty seconds later.
  **This step is not optional.** It is the test that proves the wiring test tests the wiring.
- **S11** live hand-verification on this machine, timestamped: `skein acp-agent --allow-run` against
  the real Ollama, a real `proc_run`, a real `session/cancel`, and **real process death confirmed
  through `Get-Process`** — not through Skein's own report of it. Part of this run.
- **S12** close-out: `tasks.md` with the reds verbatim, the live run, the deviations and the
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

- **A poll loop that never times out.** The deadline must be absolute and computed once; recomputing
  `timeout` per iteration makes `RUN_TIMEOUT` unreachable. S2's control is the test that catches it,
  and it asserts on elapsed wall clock, not only on the message.
- **A poll loop that spins.** A `WaitForSingleObject` returning neither `WAIT_OBJECT_0` nor
  `WAIT_TIMEOUT` must leave the loop. D2's third arm.
- **Two `Arc`s at the composition root.** The failure is silent — the run still ends `Cancelled`,
  thirty seconds late — so only an *elapsed-time* assertion catches it. S10 is that assertion and
  S10's sabotage is its proof.
- **A `proc_run` that outlives its session.** Unchanged: `drop(job)` still kills the tree, and
  cancellation reaches it strictly sooner than the timeout did.
- **Rollback** is the revert of one `const`, one helper, one loop, one struct, and one parameter on
  six signatures. No wire format, no chain payload, no `StepKind`, no CLI flag and no dependency
  changes, so a revert leaves no artifact behind.

## 6. Out of scope

- **Cancelling a tool call that is not a process.** `fs_read`, `fs_write`, `fs_list`, `git_status`
  and `git_log` are bounded by their own caps and complete in milliseconds; there is nothing there a
  50 ms poll could interrupt.
- **Cancelling while a permission request is outstanding.** `AcpPermissionTransport::call` blocks on
  an untimed `recv()` for the human's answer. A `session/cancel` arriving then is observed when the
  human answers. A real gap, and a different one: it needs a second channel into that `recv`.
- **A pre-launch check** (D4's stated residual).
- **Any `skein chat` cancellation surface** (D5; slice 026 recorded the same boundary).
- **Non-Windows.** `Sandbox` is uninhabited off Windows and `--allow-run` is a refusal there, so the
  parameter is added to the `#[cfg(not(windows))]` arm for signature parity and is unreachable.
