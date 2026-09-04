# Tasks: cancelling a tool call in flight (v0 slice)

**Spec:** `specs/027-cancel-tool-call/spec.md` · **Plan:** `specs/027-cancel-tool-call/plan.md` ·
TDD (red→green), branch `027-cancel-tool-call`, fast-forwarded onto `dev` at `ac37966`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)

- **I Headless core** ✅ no new command, no new flag, no new argument, no output change. `skein chat`
  is behaviourally identical — it passes a flag nobody holds a second reference to. `skein ledger
  log`/`show`/`verify` render a cancelled run's chain with zero CLI change, read back from a live
  one below.
- **II Local-first** ✅ no new dependency, no `Cargo.toml` change, no socket, no second process. The
  slice adds one atomic load per 50 ms to a wait that already existed.
- **III Test-First** ✅ five reds observed and recorded verbatim below. Red A is a compile absence;
  reds B, C and E each isolate one decision by reverting exactly it from the finished implementation;
  red D is a second compile absence at the port that had to change. **Red E is the slice's most
  important measurement**: with two `Arc`s at the composition root every assertion but the clock
  still passes, and the run takes 30.80 s instead of 0.26 s.
- **IV Inverted coupling** ✅ `skein-core` is **untouched**. `ToolTransport` is not widened; no port
  gains a method. The flag travels as a parameter through the crates that already depend on one
  another, and `skein-sandbox` learns only that a caller may want its child to stop — it does not
  name ACP, a session, or cancellation-as-a-protocol.
- **V Traceability** ✅ the cancelled tool call lands on the chain as an ordinary `ToolResult` whose
  payload is the refusal the model was shown, verbatim, and the run's chain verifies. No `StepKind`,
  no payload shape and no `WireExchange` field is added.
- **VI Security** ✅ strictly narrowing. The change can only make a sandboxed process's life
  **shorter**; there is no path by which a set flag lets anything run that would not have run. The
  permission gate, the AppContainer, the Job Object and the DACL are untouched, and every existing
  containment test passes unmodified.
- **VII Neutrality** ✅ one `const`, one helper, one loop, one two-field private struct, one
  parameter on six signatures. Nine alternatives are rejected with a reason each in `spec.md`,
  including the two the run order named explicitly (widening `ToolTransport`; a decorator) and the
  one Win32 most invites (`WaitForMultipleObjects`).
- **VIII Loop discipline** ✅ NON-NEGOTIABLE and unchanged. A cancelled tool is an `Err`, which is
  what `NativeLoop::mediate` already adjudicates; the controller, budget, probe and exit conditions
  are neither read nor written. A half-finished side effect is never laundered into a success.
- **Cross-platform** ✅ the parameter is added to the `#[cfg(not(windows))]` arm of `Sandbox::run`
  for signature parity and is unreachable there — `Sandbox` is uninhabited off Windows and
  `--allow-run` is a refusal. Every new test is `#![cfg(windows)]` or `#[cfg(windows)]`, as slice
  019 established.

## Tasks

- [x] **S0** fast-forwarded onto `dev` at `ac37966`, control baseline measured, `spec.md` and
      `plan.md` written
- [x] **S1** RED — a flag set while a sandboxed child runs must end it (red A), with the long-running
      command **pinned by measurement** and three rejected candidates recorded with their failures
- [x] **S2** controls — the timeout still fires, still says so, and does not say cancelled; and a run
      spanning many poll slices still returns its real exit code. See *Deviations* 1
- [x] **S3** GREEN — `skein-sandbox`: `POLL_SLICE`, the `terminate` helper, the polling `wait`,
      `Sandbox::run`'s parameter on both platform arms
- [x] **S4** RED-by-revert — the cancellation check removed from the loop (red B), restored
- [x] **S5** RED — `proc_run` must stop on the flag its server was built with (red C, by sabotage —
      see *Deviations* 2)
- [x] **S6** GREEN — `skein-connectors`: the `Launcher` struct, and the parameter on `run::execute`,
      `EmbeddedServer::with_run` and `local_connector_with_run`
- [x] **S7** RED — a session must obey the flag its **caller** supplied (red D)
- [x] **S8** GREEN — `skein-acp`: `SessionParts.cancelled`; `SkeinSession::new` stops minting one.
      `a7`, `a13` and `x1` **unmodified** beyond the new field
- [x] **S9** GREEN — `skein-cli`: `ToolArgs::transport`'s parameter; `chat.rs`'s never-set flag;
      `acp.rs`'s one-per-session mint handed to both the session and the transport
- [x] **S10** RED-by-sabotage — two different `Arc`s wired in `acp.rs` (red E), restored. **Not
      skipped**: it is the test that proves the wiring test tests the wiring
- [x] **S11** live hand-verification — **part of this run**, against the real Ollama, the real
      binary, and real process death read from outside Skein
- [x] **S12** close-out

## Control baseline (S0)

Measured on this worktree immediately after the fast-forward to `ac37966`, before any edit:

| gate | result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo test --workspace` | all green, 0 failed |

## The command this slice runs, and the three it cannot (S1)

Every candidate was launched in the real fixture under a 2 s bound. These are measurements, not
assumptions, and they are recorded in `crates/skein-sandbox/tests/cancel.rs`'s module docstring:

| candidate | measured |
|---|---|
| `waitfor.exe /t 30 SkeinCancelProbe` | exit 1 in **24.8 ms** — *"impossible d'attendre le signal spécifié"*: it needs a named kernel object an AppContainer with zero capability SIDs cannot create |
| `timeout.exe /t 30` | exit 1 in **28.7 ms** — *"la redirection de l'entrée n'est pas prise en charge"*: every stream here is a pipe |
| `ping.exe -n 30 127.0.0.1` | exit 1 in **28.5 ms** — *"Impossible de contacter le pilote IP"*: ICMP is capability-gated exactly as TCP is |
| `cmd.exe /c cmd.exe /c for /l %i in (1,1,2000000000) do @rem` | **still running at 2.0148 s**, terminated by the bound |

The last is the only survivor, and it is the command `the_job_object_kills_the_tree_when_the_clock_
runs_out` already used — so cancellation and the timeout are proved to stop the same thing. The
middle two reproduce, independently, the two rejections slice 019 recorded in prose.

## The five reds

### Red A — `Sandbox::run` has no cancellation to observe (S1)

```
error[E0061]: this method takes 4 arguments but 5 arguments were supplied
   --> crates\skein-sandbox\tests\cancel.rs:108:10
    |
108 |         .run(&system32("cmd.exe"), &forever(), 16 * 1024, GENEROUS, &cancelled)
    |          ^^^                                                        ---------- unexpected argument #5 of type `&Arc<Atomic<bool>>`
```

### Red B — the check removed from the loop, everything else intact (S4)

The finished implementation with **only** the two-line `cancelled.load(…)` guard deleted from
`wait`'s loop:

```
thread 'a_flag_set_while_a_child_runs_kills_it_long_before_its_timeout' (22052) panicked at crates\skein-sandbox\tests\cancel.rs:99:5:
the refusal must name the cancellation so the model is not told it timed out: the run exceeded the 20s limit and was terminated

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 22.64s
```

**22.64 s against 2.91 s green**, and the other two tests still pass: the revert is isolated to
exactly the decision, and the wall clock is what shows it.

### Red C — `proc_run` reading a flag nobody holds (S5)

`crate::run::execute` handed `&AtomicBool::new(false)` instead of `&launcher.cancelled`:

```
thread 'a_flag_set_while_proc_run_is_executing_ends_it_with_a_named_refusal' (42448) panicked at crates\skein-connectors\tests\run_server.rs:348:5:
the model must be told which of the two bounds stopped it: the run exceeded the 30s limit and was terminated

test result: FAILED. 10 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 30.32s
```

**30.32 s against 0.54 s green**, 10 of 11 still passing.

### Red D — `SessionParts` cannot carry the flag (S7)

```
error[E0560]: struct `SessionParts<ScriptedModel, StaticProbe, CountingTransport>` has no field named `cancelled`
    --> crates\skein-acp\tests\acp_session.rs:1389:13
```

### Red E — two `Arc`s at the composition root (S10)

`acp.rs` handing `tools.transport` a **freshly minted** flag instead of `cancelled.clone()`:

```
thread 'acp_agent_cancelling_a_proc_run_kills_it_without_waiting_for_its_timeout' (48196) panicked at crates\skein-cli\tests\cli_acp_agent.rs:1741:5:
the child must die on the flag and not on its own 30s clock; the prompt took 30.8012074s

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 30.89s
```

**30.80 s against 0.26 s green — and `assert_eq!(stop, StopReason::Cancelled)` passed.** This is the
whole reason S10 exists. A composition root holding two flags is not a broken feature that a
functional assertion catches; it is a feature that still works, thirty seconds late. Only the clock
notices, and only this sabotage proves the clock is watching.

It also settles the green run's suspiciously fast 0.26 s: the tool really does launch a real process
there, because the sabotaged run of the very same test waits for that process's own timeout.

## Live verification (S11)

`2026-09-04T06:36:19Z` — `2026-09-04T06:37:11Z`. The real `target/debug/skein.exe acp-agent
--allow-run` spawned as a subprocess and driven over its actual stdio with newline-delimited
JSON-RPC, against the real `gemma4:latest` on `http://localhost:11434/v1`. The model — not a stub —
chose the tool call. Process liveness read with `Get-CimInstance Win32_Process`, from outside Skein
entirely, so the ground truth for the death is the OS process table and not Skein's report of it:

```
  stderr: serving acp on stdio: silo live027 at http://localhost:11434/v1
[  3.50s] session = skein-1
[ 12.24s] sandboxed children before the prompt: []
[ 12.24s] ==> session/prompt sent
[ 47.88s] <== session/request_permission for proc_run
[ 47.88s] ==> permission answered skein.allow-once
[ 48.01s] LIVE sandboxed children (Get-CimInstance): [70152]
[ 48.01s] ==> session/cancel sent
[ 48.07s] <== session/prompt RESPONSE {'stopReason': 'cancelled'}
[ 51.33s] child PIDs [70152] gone from Get-CimInstance (3.32s after the cancel)
```

A real AppContainer child, **PID 70152**, existed in the machine's process table and then did not.

**The prompt was answered 60 ms after the cancel**, and that number is itself a proof of death rather
than a report of it: `Sandbox::run` returns only after joining both pipe readers, and those see EOF
only once every write end is closed — which requires the whole tree gone. The 3.32 s
`Get-CimInstance` figure is bounded by the **sampling cost**, not by the kill: each sample spawns a
PowerShell, ~1 s apiece. Recorded as measured rather than rounded down to the number that flatters
the slice.

Without this slice the same run would have answered `cancelled` at `78.01s` — the cancel plus
`RUN_TIMEOUT` — after the child had spent thirty more seconds burning a core on a result nobody
would read.

The chain that run left, read back by a second process:

```
> skein ledger log --root … --silo live027 --run "skein-1#1"
skein-1#1  0  iteration_boundary  1d470fb7…
skein-1#1  1  llm_request         d555fc1c…
skein-1#1  2  wire_exchange       f82c7cbf…
skein-1#1  3  llm_response        7353d34d…
skein-1#1  4  budget_spent        63b40694…
skein-1#1  5  tool_call           3a950aa0…
skein-1#1  6  approval            5e909e57…
skein-1#1  7  tool_result         34fb4dbf…
skein-1#1  8  iteration_boundary  af2e359b…
skein-1#1  9  llm_request         37985103…

> skein ledger verify --root … --silo live027
skein-1#1  ok  10 steps
```

Ten steps, verifying, and the shape is the whole story: the tool was called, approved and produced a
result; the loop went round; the next `llm_request` was appended and there is **no `wire_exchange`
after it and no `Exit`** — `CancellableModel` refused that turn before it left the process. The two
cancellation readers are visible in one chain.

The `tool_result` payload is the refusal the model was actually shown:

```json
{"tool":"proc_run","content":"{\"content\":[{\"type\":\"text\",\"text\":\"the run was cancelled by the client and was terminated\"}],\"isError\":true}"}
```

Not a timeout, not a truncated success: the cancellation, in the model's own transcript, as an error
the run survived long enough to record.

The AppContainer profile the live run created was pruned afterwards
(`skein sandbox prune --profile skein-4cdf2719554d0231` → `revoked … / deleted profile`), leaving no
machine state behind.

## Final gates

| gate | result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo test --workspace` | 279 passed, **0 failed**, across 45 test binaries |

## Deviations

1. **The run order said slice 019 never wrote a timeout-path control test. It did.**
   `the_job_object_kills_the_tree_when_the_clock_runs_out` (`skein-sandbox/tests/escape.rs:158`)
   proves the tree dies on the clock, and it names two of the three rejected long-running commands in
   prose. The code decided: it was kept, unmodified apart from the new argument, and S2 wrote the two
   controls that genuinely were missing and that **D2's loop is what puts at risk** — that the
   timeout still says *timeout* and not *cancelled*, that a sliced wait does not expire on its first
   slice, and that a run outliving many slices still returns its real exit code through the loop's
   normal exit.
2. **Red C was obtained by sabotage rather than as a compile absence.** `skein-connectors`' library
   had to compile before its test target could be built at all, so a pure absence there would have
   been the same error red A already recorded, one crate along. Sabotaging the wiring inside
   `proc_run` produces a *behavioural* red at that layer instead, which is strictly more informative
   — and it is the same technique S10 uses one layer up.
3. **`CancellableModel`'s module docstring was corrected**, which the plan did not name. It claimed
   there were two readers of the flag; there are now three, and leaving it would have been a false
   statement in the file most likely to be read by someone looking for how cancellation works.
4. **The fast-forward was 36 commits, not the 32 the run order stated.** `d364405..ac37966` measured
   on the tree. No consequence beyond the number.
5. **`RunAccess` was not changed.** The plan's D1 lists the flag flowing through `with_run`; an
   earlier reading would have put it inside `RunAccess::Allowed`. That was rejected during S6 on the
   code rather than in the plan: `RunAccess` derives `PartialEq`/`Eq`, which an `AtomicBool` cannot,
   and `RunArgs::resolve` builds one in a frame that has no session and therefore no flag to give it.
   The `Launcher` struct (D6) expresses the same pairing invariant one layer lower, where both halves
   actually exist.

## Residuals

- **A cancel arriving while a permission request is outstanding** is observed when the human answers.
  `AcpPermissionTransport::call` blocks on an untimed `recv()`, and reaching into it needs a second
  channel. Named in `spec.md`'s *Out of scope*; the next slice if anything.
- **No pre-launch check** (D4). A cancel landing between the permission answer and `CreateProcessW`
  starts a process and kills it one poll slice later.
- **50 ms of latency** between the `store` and the `TerminateProcess`, by construction. D2 records
  the rejected `WaitForMultipleObjects` that would remove it and what it would cost.
- **`skein chat` has no cancellation surface**, unchanged from slice 026's boundary.
- **The five non-process tools cannot be cancelled**, and nothing has been built as if they could.
