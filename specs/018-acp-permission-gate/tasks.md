# Tasks: prove the ACP permission gate end to end with an answering client (v0 slice)

**Spec:** `specs/018-acp-permission-gate/spec.md` · **Plan:** `specs/018-acp-permission-gate/plan.md`
· TDD (red→green), **no product code**, branch `018-acp-permission-gate` cut from `dev` at
`4eeea42`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ nothing is added to any layer; the CLI stays the authoritative client and this
  slice proves one of its behaviours through the shipped binary, which is what Principle I says the
  CLI is for · II Local-first ✅ NON-NEGOTIABLE and untouched: the model is a `TcpListener` on
  loopback in the test process, the connector stays in-process, no network egress and no new
  dependency
- III Test-First ✅ the whole slice **is** the test. Each new test's red was observed and recorded
  verbatim under `## Observed red` before its green, and the first one corrected the spec's own
  prediction about how a missing handler fails · IV Inverted coupling ✅ untouched. `skein-cli` is
  the one crate that already depends on both `skein-acp` and `skein-connectors`, which is why the
  proof lives here and not in either of them: a real `fs_write` effect on disk needs
  `skein-connectors`, which `skein-acp` does not depend on and must not
- V Traceability ✅ this slice is where the ACP gate's chain shape first becomes a checked claim.
  Both runs' chains are read back **in a second process** through `skein ledger log` and
  `skein ledger verify`, at 12 and 11 steps; the deny chain differs from the allow chain by the
  absence of `tool_result` and nothing else. No new `StepKind` — the existing deny shape is matched,
  not reinvented
- VI Security ✅ **the principle this slice exists for.** Deny-by-default is proven at the layer
  where the binary, the policy, the connector, the disk and the chain are all real: an `AllowOnce`
  answer lets a real `fs_write` land on disk, and a `RejectOnce` answer under the identical fixture
  leaves no file at all. The two option-id constants `AcpPermissionTransport::call` matches on are
  pinned from the client's side, where a typo in either would otherwise silently turn every Allow
  into a denial
- VII Neutrality ✅ two tests, one copied helper pair, no new dependency, no new machinery. The
  tempting `StepKind` for the client's answer was rejected: the answer is already on the chain twice
  over
- VIII Loop discipline ✅ NON-NEGOTIABLE and untouched. A governed refusal remains history the run
  survives — both runs reach `StopReason::EndTurn` and answer
- Cross-platform ⚠️ **no `#[cfg]` anywhere.** The fixture is `TempDir` plus `Path::join`, and
  `FsRoot` canonicalizes both sides of its containment check already. The tri-OS caveat of slices
  004–017 stands unamended: the Windows leg is observed locally, macOS and Linux remain unobserved
  until this repository has a remote

## Tasks
- [x] **T0** `specs/018-acp-permission-gate/{spec.md,plan.md,tasks.md}`; branch
      `018-acp-permission-gate` cut from `dev` at `4eeea42`
- [x] **T1** control baseline: `cargo test --workspace` before any edit — **191 passed, 0 failed,
      3 ignored**
- [x] **T2** helpers: `tool_call_reply` and `last_message` copied from `cli_chat.rs`,
      `struct Answered`, `fn run_answering`
- [x] **T3** RED→GREEN — the Allow path: `an_acp_client_that_allows_lets_a_real_fs_write_execute`
- [x] **T4** RED→GREEN — the Deny path:
      `an_acp_client_that_rejects_stops_the_fs_write_and_the_run_survives`
- [x] **T5** no pre-existing assertion changed: the diff of `cli_acp_agent.rs` is append-only apart
      from the `use` block
- [x] **T6** gates, control diff, close-out

## Control baseline (T1)

`cargo test --workspace` on `018-acp-permission-gate` @ `4eeea42`, working tree clean, 2026-09-03,
before any edit: **191 passed, 0 failed, 3 ignored** — `acp_session` 16, `cli_acp_agent` 8,
`cli_chat` 12, `cli_ledger` 8, `cli_secret` 2, `connector` 6, `fs_root` 10, `fs_server` 7,
`git_root` 5, `git_server` 13, `governed_fs_run` 4 (+1 ignored), `governed_git_run` 4 (+1 ignored),
`core` 19, `native_loop` 25, `tool_gateway` 14, `governed_run` 2, `openai_compat` 15 (+1 ignored),
`rmcp_gateway` 9, `silo_ledger` 7, `silo_secret` 5. The six `src/lib.rs`/`src/main.rs` unit-test
targets and the six doc-test targets each contribute `0 passed`.

This is slice 017's recorded close figure exactly. The plan predicted it would be, and it was
**re-measured rather than quoted** — it is the number T6 diffs against.

## Observed red (Constitution III)

All on 2026-09-03. Recorded verbatim.

**T2/T3, first red — the imports.** `cargo test -p skein-cli --test cli_acp_agent`, against the
harness written but the `use` block untouched:

```
error[E0425]: cannot find type `RequestPermissionRequest` in this scope
   --> crates\skein-cli\tests\cli_acp_agent.rs:723:30
error[E0433]: cannot find type `PermissionOptionKind` in this scope
error[E0433]: cannot find type `ToolCallId` in this scope
error: could not compile `skein-cli` (test "cli_acp_agent") due to 7 previous errors
```

**T3, the red that matters — and it is not the one the spec predicted.** The harness was run once
deliberately **without** registering `on_receive_request`, because that is the state `dev` is in and
the spec claims it is why the residual could sit open through two slices. The spec predicted the
request would go unhandled, `ask` would return `Err`, and the prompt would answer with an internal
error. Measured instead:

```
running 1 test
test an_acp_client_that_allows_lets_a_real_fs_write_execute has been running for over 60 seconds
test an_acp_client_that_allows_lets_a_real_fs_write_execute ... FAILED

---- an_acp_client_that_allows_lets_a_real_fs_write_execute stdout ----
thread 'an_acp_client_that_allows_lets_a_real_fs_write_execute' (38280) panicked at
crates\skein-cli\tests\cli_acp_agent.rs:163:10:
the ACP client finished within 60s: Timeout

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 8 filtered out; finished in 60.01s
```

A client with no handler registered does **not** auto-reply "method not found". The request is
simply never answered, and `AcpPermissionTransport::ask`'s untimed
`std::sync::mpsc::Receiver::recv()` blocks the child's loop thread **forever**. What turned that
into a failure rather than a hung CI job on three operating systems is the file's pre-existing
`run_with_timeout`, which the plan's risk table listed only as "belt and braces". It was the
load-bearing mitigation. Recorded under `## Deviations from the plan`, and the spec's
untimed-`recv` residual is upgraded from a note to a measured behaviour.

Registering the handler — which only records and responds, and never calls `block_task()` — is the
green.

**T4 — green on arrival**, because T3's harness had already built everything the deny test
composes. Recorded as such rather than dressed up: a guard that has never failed is a guard nobody
has checked, so its teeth were demonstrated by breaking exactly the two things it claims to protect.

First, the answer flipped to `AllowOnce` and nothing else changed:

```
---- an_acp_client_that_rejects_stops_the_fs_write_and_the_run_survives stdout ----
thread '...' (41700) panicked at crates\skein-cli\tests\cli_acp_agent.rs:954:5:
a client's refusal must have had no effect whatsoever
```

So the file's absence is genuinely produced by the client's refusal, and not by anything the fixture
would have arranged anyway. Second, with the answer restored and a `tool_result` step added to the
expected chain:

```
---- an_acp_client_that_rejects_stops_the_fs_write_and_the_run_survives stdout ----
thread '...' (21396) panicked at crates\skein-cli\tests\cli_acp_agent.rs:973:5:
assertion `left == right` failed
  left: ["iteration_boundary", "llm_request", "llm_response", "budget_spent", "tool_call",
"approval", "iteration_boundary", "llm_request", "llm_response", "budget_spent", "exit"]
 right: ["iteration_boundary", "llm_request", "llm_response", "budget_spent", "tool_call",
"approval", "tool_result", "iteration_boundary", "llm_request", "llm_response", "budget_spent",
"exit"]
```

So the ACP-denied chain really is the allowed chain minus `tool_result`, measured rather than
asserted into existence. Both edits were reverted verbatim afterwards and the target is green.

## Append-only check (T5)

`git diff dev -- crates/skein-cli/tests/cli_acp_agent.rs`, filtered to removed lines, is exactly the
two-line `use` block that was rewritten to add six imports:

```
-    ContentBlock, InitializeRequest, NewSessionRequest, PromptRequest, SessionId,
-    SessionNotification, SessionUpdate, StopReason, TextContent,
```

Nothing else in the workspace lost a line. **No assertion anywhere was changed or removed**
(FR-007).

## Gates (T6)

All four on 2026-09-03, Windows 11, the channel `rust-toolchain.toml` pins.

- `cargo fmt --all --check` — clean, no output.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, no warnings.
- `cargo build --workspace` — succeeds.
- `cargo test --workspace` — **193 passed, 0 failed, 3 ignored**, against the T1 baseline of
  191/0/3. The +2 is `cli_acp_agent` 8 → 10 and nothing else: `acp_session` 16, `cli_chat` 12,
  `cli_ledger` 8, `cli_secret` 2, `connector` 6, `fs_root` 10, `fs_server` 7, `git_root` 5,
  `git_server` 13, `governed_fs_run` 4 (+1 ignored), `governed_git_run` 4 (+1 ignored), `core` 19,
  `native_loop` 25, `tool_gateway` 14, `governed_run` 2, `openai_compat` 15 (+1 ignored),
  `rmcp_gateway` 9, `silo_ledger` 7, `silo_secret` 5 — **every one identical to T1**. That is
  SC-006 stated as a number.

The tri-OS caveat of slices 004–017 stands unamended: the Windows leg is observed locally, and the
macOS and Linux legs remain unobserved until this repository has a remote. This slice adds no
`#[cfg]`, no dependency and no OS-specific call, so it does not widen the unobserved surface.

## Control diff (T6)

`git diff 4eeea42 --stat -- crates/skein-core/ crates/skein-acp/ crates/skein-connectors/
crates/skein-gateway/ crates/skein-mcp/ crates/skein-silo/ spikes/ .github/ rust-toolchain.toml
Cargo.toml Cargo.lock 'crates/*/src/'` — **empty**. The `crates/*/src/` half is the stronger claim
and it is the one FR-001 makes: **no product code changed anywhere in the workspace.**

Everything the slice touched, `git diff 4eeea42 --stat`:

```
 crates/skein-cli/tests/cli_acp_agent.rs | 374 +++++++++++++++++++++++++++++-
 specs/018-acp-permission-gate/plan.md   | 388 ++++++++++++++++++++++++++++++++
 specs/018-acp-permission-gate/spec.md   | 166 ++++++++++++++
 specs/018-acp-permission-gate/tasks.md  |  69 ++++++
 4 files changed, 995 insertions(+), 2 deletions(-)
```

One test file. The `-2` is T5's `use` block.

**Why the stat is taken against `4eeea42` rather than `dev`.** The plan and the request both name
`4eeea42` as the branch point, and it was `dev`'s tip when the branch was cut. `dev` has since
advanced two commits — `e3c9b08` (ADR-0006, documentation) and `4e9bf3b` (`chore: track the
archon-cli agent skill files`) — neither of which touches `crates/`. Diffing the full tree against
`dev` would therefore report those two commits' contents as this slice's deletions, which is noise
about where `dev` moved rather than a fact about this slice. The control-path diff is **empty
against `dev` too**, verified both ways; only the full stat needed the branch point to be readable.
No merge or rebase was performed.

## Deviations from the plan

Three, all recorded rather than quietly absorbed.

1. **The T3 red was a hang, not an internal error** — see `## Observed red`. The spec's own point 3
   predicted `ask` would return `Err` and the prompt would answer with an internal error; measured,
   the unanswered request blocks the child forever on an untimed `recv()`. The prediction is left in
   the spec as the reasoning that was available before the measurement, and corrected here, which is
   this project's own idiom for a premise that did not survive contact. The practical consequence is
   that the plan's risk table under-rated `run_with_timeout`: it is not belt-and-braces, it is the
   only thing standing between a missing handler and a hung CI job on three operating systems.

2. **`Answered::updates` is an `Arc<Mutex<Vec<SessionUpdate>>>`, not a `Vec<SessionUpdate>`.** The
   plan named the plain `Vec`. The file's pre-existing `chunks(&Mutex<Vec<SessionUpdate>>)` helper
   is what reads them, and T5 makes changing its signature a stop condition — so the `Arc<Mutex<…>>`
   is what lets the new tests reuse the existing helper instead of duplicating it. `asked` is the
   plain `Vec` the plan specified, because nothing pre-existing reads it.

3. **Each test reads the stub's first request body and discards it.** The plan's validation section
   asserts on "the **second** request", and `StubProvider::request_body()` is a queue read rather
   than an index, so the first read is a positional necessity. It is bound to `_first` rather than
   dropped anonymously, so the reason it exists is visible.

The plan's T3 instruction — *"green needs no `src/` change; if it does, the plan is wrong and that
is a stop condition"* — held. No `src/` change was needed or made.

## Out of scope

Deliberately not done, so no one helpfully does it. Identical to the spec's list, and in particular:

- **Any change under `crates/*/src/`** — verified empty in the control diff above.
- **A new `StepKind` or a second `Approval` step** for the client's answer. The existing deny shape
  is matched; the answer is already on the chain twice over, as the absence of `ToolResult` and
  verbatim inside the next `LlmRequest` payload.
- **The `Cancelled` outcome**, already covered at unit level by
  `p3_a_cancelled_answer_denies_without_reaching_the_transport`.
- **`AllowAlways` / `RejectAlways` or any answer persistence**, **new mutating tools**, **any UI**,
  and **a live-model test** — a live model cannot be made to answer Deny.
- **`crates/skein-silo/`, `spikes/`, `.github/`, `rust-toolchain.toml`, `Cargo.toml`,
  `Cargo.lock`** — verified empty in the control diff.

## Next slice (not this feature)

- **The ACP permission gate exercised end to end is CLOSED by this slice.** It is struck from the
  carried-forward list rather than repeated: an `AllowOnce` answer from a real client over the real
  protocol to the real binary lets a real `fs_write` land on disk, a `RejectOnce` answer under the
  identical fixture leaves no file, and both chains verify in a second process at 12 and 11 steps.
- **Residuals this slice adds**, recorded rather than hidden:
  - **A permission request cannot be correlated to its tool call by a client.**
    `AcpPermissionTransport::ask` uses `ToolCallId::new(tool)` — the tool *name* — while
    `skein_acp::project_updates` uses `step.id`, the chain hash. The two ids never match, so an
    editor cannot join the prompt it showed to the tool call it later sees. Fixing it needs the
    chain step id inside the transport, which the transport does not have; that is a design change.
    `project_updates`' own docstring — *"the ACP tool-call id **is** the chain id of the `ToolCall`
    step, so a client's correlation key is the chain's own identity"* — is true of the projection
    and silent about the permission request, which is how the mismatch went unnoticed.
  - **An ACP-denied call is projected as `Pending` forever.** `project_updates` maps
    `Approval.decision == "allowed"` → `Pending` and only a `ToolResult` step → `Completed`. On the
    ACP-deny path the `Approval` says `allowed` (the *policy* allowed it) and no `ToolResult` is
    written, so the client's last word on that tool call is `Pending`. The deny test asserts only
    the **absence** of `Completed`, so fixing this will not fight a frozen assertion.
  - **`AcpPermissionTransport::ask` blocks on an untimed `recv()`**, now measured: a client that
    receives the request and never answers hangs the child indefinitely. A timeout belongs to a
    slice that has timeout machinery.
- **A `shell` connector**, still deferred and now scoped by ADR-0006 to Windows-first.
- Carried unchanged from slice 017: the `canonicalize`-to-open TOCTOU residual, `role: "tool"` /
  `tool_call_id` replay, raw wire-byte capture, streaming (SSE), provider authentication, a config
  file, `--json` output, and the slices-008-vs-014 `serde_json/preserve_order` reconciliation.
