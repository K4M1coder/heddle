# Tasks: a `ToolResult` capture is scrubbed as wire bytes (v0 slice)

**Spec:** `specs/029-toolresult-redaction/spec.md` · **Plan:**
`specs/029-toolresult-redaction/plan.md` · TDD (red→green), branch `029-toolresult-redaction`,
fast-forwarded onto `dev` at `fe13c73`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)

- **I Headless core** ✅ no command, flag, argument or output change. `heddle ledger show` renders a
  run holding a scrubbed tool result with zero CLI change, because the payload's *shape* is
  identical — only which bytes inside it are `***` differs.
- **II Local-first** ✅ no new dependency, no `Cargo.toml` change, no socket, no process, no thread.
  `redact_wire` was already compiled into this binary and already called twice per turn; this is a
  third call on a path that already ran a `str::replace` per secret.
- **III Test-First** ✅ three reds observed and recorded verbatim below. Red A is the forward red.
  Reds B and C are the **RED-by-revert** at both levels, which is what earns a one-identifier
  change: a forward red proves the test can fail, and only the revert proves it fails for *this*
  line. Red A's first form also caught a wrongly-written assertion in the new test itself — see
  *Deviations* 1.
- **IV Inverted coupling** ✅ nothing crosses a boundary it did not already cross. No port gains a
  method, `ToolTransport` is not widened, `Redactor`'s public surface is unchanged, and
  `heddle-mcp`/`heddle-connectors`/`heddle-acp`/`heddle-cli` have no production change at all. The fix is
  inside the one type that already owned both the transport and the redactor.
- **V Traceability** ✅ no `StepKind`, no payload field, no serialization change. Every chain recorded
  before this slice still deserializes and still verifies, and a run's chain verification is asserted
  in both new tests. FR-004 pins that the scrubbed body is still parseable, so replay is unaffected —
  and D4 makes every assertion in the slice *depend* on that parse rather than merely claim it.
- **VI Security** ✅ NON-NEGOTIABLE, and the slice's whole subject. Strictly narrowing: `redact_wire`
  applies the literal needle first and adds the escaped one only when the two differ, so it can only
  find **more** secrets than `redact` did, never fewer. FR-007 pins the "never fewer" half as a test
  rather than as an argument.
- **VII Neutrality** ✅ one identifier, one comment, four tests, one test-helper extraction. Six
  alternatives are rejected with a reason each in `spec.md`, including the two the shape most invites
  (scrub the downstream copies harder; collapse `redact` and `redact_wire` into one method). D2
  refuses a second change the originating request asked for, on a measurement.
- **VIII Loop discipline** ✅ untouched. The controller, budget, probe and exit conditions are neither
  read nor written. `NativeLoop::mediate` already fed the *capture* back rather than the raw outcome;
  this slice only changes which bytes that capture holds.
- **Cross-platform** ✅ nothing platform-specific. `redact_wire` names no OS API and both new tests
  run on every platform.

## Tasks

- [x] **S0** fast-forwarded onto `dev` at `fe13c73` — **the first action of the run, and load-bearing:**
      `HEAD` was `d364405`, which predates slice 023, and
      `git show d364405:crates/heddle-core/src/tool.rs | grep -c redact_wire` returns `0`. Verified
      after the fast-forward that `redact_wire` is present (`tool.rs:237`) before writing a line.
      Control baseline measured, the leak measured at both levels (`plan.md` §0.3) and reverted,
      `spec.md` and `plan.md` written
- [x] **S1** RED — an awkward secret in a wire-shaped tool result, asserted on the parsed capture
      (red A)
- [x] **S2** GREEN — `redact` → `redact_wire` at `tool.rs:410`, and its comment
- [x] **S3** RED-by-revert at **both** levels (reds B and C), the revert restored and both suites
      re-confirmed green
- [x] **S4** integration — the same property with a real file, the real `EmbeddedServer` and the real
      `NativeLoop`, plus the assertion that the *model* was told the scrubbed body
- [x] **S5** controls — a secret with nothing to escape captures byte-for-byte as before (FR-007);
      an awkward secret in a call's *arguments* still scrubs by the literal needle (FR-008, D2). Both
      **stay green under S3's revert**, which is what makes them controls rather than more of the
      same test
- [x] **S6** close-out

## Control baseline (S0)

Measured on this worktree immediately after the fast-forward to `fe13c73`, before any edit:

| gate | result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo test --workspace` | **283 passed, 0 failed** |

## What was leaking, measured before the fix (S0)

`plan.md` §0.3 in full. The short version, from one governed run with `sk-"awkward"-SECRET-abc123`
configured and read off a real file by the real `fs_read` tool:

| # | where | leaked? |
|---|---|---|
| 1 | the `ToolResult` step | **yes** |
| 2 | the next turn's `LlmRequest` step | **yes** |
| 3 | that turn's `WireExchange` step | **yes** |
| 4 | the HTTP request body the provider received | **yes — the secret left the machine** |

Rows 2–4 exist because `NativeLoop::mediate` feeds the capture back into the conversation. They are
closed by fixing row 1 and could not have been closed downstream: each copy escapes the secret one
level deeper, and by row 4 neither of `redact_wire`'s two needles matches.

**And both assertions a test would reach for first were `false` on that run.** The literal needle,
and the `escaped()` helper `governed_fs_run.rs` already has. That is not a near miss — it is the
reason this slice needed a plan, and it is why D4 makes the *parsed* capture the only assertion
surface here.

## The reds

### Red A (S1) — the forward red

`an_awkward_secret_is_redacted_from_a_wire_shaped_result`, against the unmodified `call_captured`:

```
running 15 tests
test an_awkward_secret_is_redacted_from_a_wire_shaped_result ... FAILED

---- an_awkward_secret_is_redacted_from_a_wire_shaped_result stdout ----
panicked at crates\heddle-core\tests\tool_gateway.rs:323:5:
assertion `left == right` failed: a secret containing a quote, a backslash or a newline is on an
already-serialized result in escaped form, so finding it needs the wire premise
  left: "api_key=pa\"ss\\wo\nrd\nendpoint=localhost"
 right: "api_key=***\nendpoint=localhost"

test result: FAILED. 14 passed; 1 failed; … finished in 0.00s
```

The secret arrives **whole** in the parsed capture — quote, backslash and newline. The other 14
tests, including slice 014's own `secret_is_redacted_from_args_and_result_before_capture`, pass: that
test's secret has no escapable character, so it is green either way. Fourteen green redaction-adjacent
tests over a chain carrying a secret in cleartext is the state this red ended.

### Red B (S3) — the revert, at unit level

`redact_wire` → `redact` at `tool.rs:410`, nothing else touched:

```
test an_awkward_secret_is_redacted_from_a_wire_shaped_result ... FAILED
test a_secret_with_nothing_to_escape_is_captured_byte_for_byte_as_before ... ok
test an_awkward_secret_in_the_arguments_is_still_scrubbed_by_the_literal_needle ... ok

panicked at crates\heddle-core\tests\tool_gateway.rs:330:5:
  left: "api_key=pa\"ss\\wo\nrd\nendpoint=localhost"
 right: "api_key=***\nendpoint=localhost"

test result: FAILED. 16 passed; 1 failed; … finished in 0.00s
```

**S5's two controls pass under the revert**, which is exactly their job: they pin behaviour the fix
must *not* change, so a control that went red with the fix would mean the fix was too broad.

### Red C (S3) — the same revert, at integration level

```
test a_secret_with_a_quote_in_it_is_scrubbed_from_a_real_tool_result ... FAILED

panicked at crates\heddle-connectors\tests\governed_fs_run.rs:759:5:
assertion `left == right` failed: the ToolResult capture must not carry a configured secret in
escaped form
  left: "api_key=sk-\"awkward\"-SECRET-abc123\nendpoint=http://localhost:11434"
 right: "api_key=***\nendpoint=http://localhost:11434"

test result: FAILED. 0 passed; 1 failed; 8 filtered out; finished in 0.02s
```

The escaping here is produced by a **real** MCP server, not by a double's `format!`, which is the
premise the one-line fix rests on. The revert was restored immediately and both suites re-run green.

**Why the revert is the load-bearing step of this slice.** The whole production diff is one
identifier. A reviewer reading `redact` against `redact_wire` has no way to tell which of them the
new test actually depends on — the forward red proves the test *can* fail, and only the revert proves
it fails for this line. It is also the cheapest possible check that the fix is not a no-op the
compiler happened to accept.

## Deviations from the plan

1. **The new test's own FR-006 assertion was written in the leaking form, and the fix exposed it.**
   S1's first draft asserted `out.content.contains(AWKWARD)` — "the raw secret must still reach the
   trusted caller". After S2 that assertion **failed**, and it was the test that was wrong, not the
   code: `out.content` is the transport's serialized body, so the secret is escaped there too and the
   literal needle misses a secret that is genuinely present. Repaired to `body_text(&out.content)`,
   the same parse every other assertion uses. Recorded rather than quietly fixed, because it is D4
   demonstrated in the *opposite* direction, in a test written by an author who had just finished
   measuring the trap. `body_text`'s doc comment says so in the code.
2. **`plan.md` §0.3's "unit-level measurement" also pinned D2 in the same run**, which was not
   planned as a measurement — the temporary test printed every step, and the `ToolCall` step's
   `{"token":"***"}` was simply visible in the output. It turned D2 from a reading of the code into
   an observation, and S5 turned it into `an_awkward_secret_in_the_arguments_is_still_scrubbed_by_the_literal_needle`.
3. **A small extraction in the test file that the plan did not name.** `tool_gateway.rs`'s `gateway`
   helper hard-coded `Redactor::new(vec![SECRET.into()])`, and three of this slice's tests need the
   material to vary. The policy construction was lifted into `policy(approved)` and `gateway` now
   sits alongside `gateway_scrubbing(transport, secret)`; the eight existing call sites are
   unchanged. No production code was involved.
4. **No live hand-verification step.** Slices 025–028 each ended with one, and this slice has none,
   deliberately: the property is "a byte is absent from a payload", which the integration test
   observes exactly and a human reading a terminal observes worse. The path *to* that payload — a
   real file, a real MCP server, the real loop, the real socket — is the shipped article in S4's
   test, and the live model tests that already cover it (`a_live_model_calls_a_real_fs_tool`) are
   untouched and still `#[ignore]`d.

## Residuals

- **A secret the operator never configured is still captured in cleartext.** FR-009, unchanged from
  slice 014, and stated here because this slice makes the *configured* case airtight and could
  otherwise read as closing more than it does. `governed_fs_run.rs`'s existing `endpoint=` assertion
  is the control that keeps it visible.
- **A secret split across two ACP deltas can still reach a client's transcript.** Slice 025's own
  recorded residual, on the `stream.rs` path this slice audited (D3) and did not change. The chain is
  unaffected.
- **`redact_wire` matches two forms, not all forms.** It handles a secret escaped once, which is what
  a serialized body contains. A *doubly* escaped copy is out of its reach, and the slice's answer to
  that is architectural rather than a third needle: scrub at the source, where only one level of
  escaping exists. If a fourth already-serialized body appears in the product, it needs `redact_wire`
  at its own source — `plan.md`'s D3 audit table is where the five call sites are recorded so that
  question is answerable without re-deriving it.
- **Nothing enforces the choice at the type level.** `redact` and `redact_wire` have the same
  signature, so picking the wrong one is a comment-and-review matter rather than a compile error.
  A `WireBody` newtype threaded through `ToolOutcome.content` and `WireExchange` would make it
  structural; it changes a core port's shape for a two-caller invariant, and it is recorded here as a
  candidate rather than taken in a one-identifier slice.

## Gates (S6)

| gate | command | result |
|---|---|---|
| format | `cargo fmt --all -- --check` | clean |
| lint | `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| tests | `cargo test --workspace` | **287 passed, 0 failed** (283 at baseline, plus this slice's four) |
