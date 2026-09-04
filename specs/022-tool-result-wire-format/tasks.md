# Tasks: tool results reply as `role:"tool"` with a real `tool_call_id` (v0 slice)

**Spec:** `specs/022-tool-result-wire-format/spec.md` · **Plan:**
`specs/022-tool-result-wire-format/plan.md` · TDD (red→green), branch `022-tool-result-wire-format`
cut from `dev` at `6873137`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)

- **I Headless core** ✅ no CLI of its own, no new flag, no new command. `skein chat` and
  `skein acp-agent` are unchanged and stay the authoritative clients; what changes is the shape of
  the conversation the core replays, which no caller names.
- **II Local-first** ✅ untouched, and it is *why* one of this slice's motivations was refused. The
  originating "a stricter hosted provider may reject this" premise is refuted precisely because
  `ureq` is compiled with no TLS backend, so hosted providers are unreachable at the transport. No
  new network-capable code, no new dependency, no `Cargo.toml` change.
- **III Test-First** ✅ each step's outcome is recorded verbatim under `## Observed red`, and where a
  step had **no** red the entry says so and why rather than dressing one up.
- **IV Inverted coupling** ✅ `skein-core` gains nothing that names a protocol. `Message` grows two
  fields that describe a *conversation*, not a wire; the OpenAI spelling of them
  (`{"role":"tool","tool_call_id":…}`, `arguments` as a JSON string) exists only in
  `skein-gateway`, which stays the one crate naming the chat-completions format. The `Option<String>`
  id fallback lives at that port too (D3), so no other crate reasons about a provider that omits one.
- **V Traceability** ✅ no new `StepKind` and no change to `Ledger`, `ToolGateway` or `Approval`. The
  `LlmRequest`/`LlmResponse` payloads gain fields that are `#[serde(default)]` +
  `skip_serializing_if`, so a tool-free turn is byte-identical to before and a pre-022 chain still
  deserializes. `verify_chain` is asserted, not assumed, on a tool-bearing redacted run.
- **VI Security** ✅ NON-NEGOTIABLE, and this slice is the one that finally discharges the *structural*
  half of design §7 item 5. Tool output stops being user-role text under a **forgeable** label and
  becomes a role no prompt, model output or tool body can produce. The echoed calls are redacted
  with the loop's own `Redactor` (D4). The *obedience* half stays open and `spec.md` says so rather
  than claiming it closed — measured, not assumed (finding 2).
- **VII Neutrality** ✅ two `Message` fields, one `ToolCall` field, two constructors, one combinator,
  one `Redactor` method extracted from an existing duplication, and **one deleted function**. No new
  tool, flag, crate, dependency or `StepKind`. A `Content::ToolResult` variant, a legacy `"name"`
  field on the tool message, echoing calls while keeping user-role labels, and a raw-wire-byte
  `StepKind` were each considered and rejected with a reason in `spec.md`'s register.
- **VIII Loop discipline** ✅ NON-NEGOTIABLE and untouched. The controller, the budget, the probe and
  the exit conditions are not read or written by this slice. A refusal is still history the run
  survives; only its wording and its envelope moved.
- **Cross-platform** ✅ no `#[cfg]`, no platform API, no filesystem or process work. Every test here
  is deterministic and runs identically on all three platforms; the one `#[ignore]`d live test is
  gated on an environment variable, as slices 019–021 established.

## Tasks

- [x] **T0** `specs/022-tool-result-wire-format/{spec.md,plan.md,tasks.md}`; branch rebased onto
      `dev` at `6873137` and control baseline re-measured
- [x] **T1** RED — the defect: two calls in one turn, echoed with distinct ids and answered in order
- [x] **T2** RED — the safety property: a forged label in a *prompt*, and the no-dangling-id
      invariant over the wire
- [x] **T3** RED — redaction: a secret in echoed **arguments**, still-parseable payloads, verifying
      chain
- [x] **T4** RED — the wire: exact bytes for `role:"tool"` and an echoed assistant, the parsed id,
      and D3's synthesized fallback
- [x] **T5** GREEN — `skein-core`: D1 (`content.rs`), D2 + `Redactor::redact_call` (`tool.rs`), D4
      (`native_loop.rs`)
- [x] **T6** GREEN — `skein-gateway`: D3 and D5, and slice 015's now-false residual entries
      corrected (see *Deviations* 1)
- [x] **T7** the existing assertions that genuinely change — seven test files, eighteen sites
- [x] **T8** live verification against a real local model — **part of this run**
- [x] **T9** close-out: the residual carried since slice 015 is dropped from *Next slice*

## Control baseline (T0)

On `022-tool-result-wire-format` rebased onto `dev` @ `6873137`, working tree clean, Windows 11 Pro
10.0.26200, toolchain 1.97, 2026-09-04, before any edit:

- `cargo fmt --all --check` — clean, no output, exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, `Finished dev profile`, exit 0.
- `cargo test --workspace` — **234 passed, 0 failed, 5 ignored**, matching slice 021's recorded
  close exactly.

The rebase was a **fast-forward**: the branch carried no commits of its own, so nothing conflicted
and nothing was rewritten. `git log --oneline -1` = `6873137`.

## Research findings

Carried in full, with sample sizes and the two findings that argue *against* changing, in
`spec.md`'s *The rejected-alternatives register*. Not duplicated here.

## Observed red

Every red in this slice is a **compile** red rather than a behavioural one, and that is worth saying
plainly rather than dressing up. The slice's subject is a type that did not exist: there was no
`Role::Tool` for a test to assert the wrong value of, so the honest red is "this does not build".
The behavioural evidence that the old shape was *wrong* is not in this suite at all — it is the live
0/6-vs-6/6 measurement recorded in `spec.md`'s finding 4, which no deterministic test can carry
without becoming a model benchmark.

**T1 — the multi-call round-trip.** `cargo test -p skein-core --test native_loop two_calls_in_one_turn`:

```
error[E0599]: no associated function or constant named `with_id` found for struct `skein_core::ToolCall`
   --> crates\skein-core\tests\native_loop.rs:633:27
error[E0609]: no field `tool_calls` on type `&Message`
   --> crates\skein-core\tests\native_loop.rs:658:14
error[E0599]: no variant, associated function, or constant named `Tool` found for enum `Role`
   --> crates\skein-core\tests\native_loop.rs:667:39
error[E0609]: no field `tool_call_id` on type `&Message`
   --> crates\skein-core\tests\native_loop.rs:669:20
error: could not compile `skein-core` (test "native_loop") due to 6 previous errors
```

**T2 — the safety property.** The forged-label test adds three more of the same on `native_loop.rs`;
the wire invariant is red in the other crate too:

```
error[E0599]: no method named `with_tool_calls` found for struct `Message`
   --> crates\skein-gateway\tests\openai_compat.rs:312:41
error[E0599]: no associated function or constant named `tool_result` found for struct `Message`
   --> crates\skein-gateway\tests\openai_compat.rs:324:22
error: could not compile `skein-gateway` (test "openai_compat") due to 5 previous errors
```

**T3 — redaction across the echo.** `error: could not compile skein-core (test "native_loop") due to
15 previous errors`, the new ones being `no field tool_calls on type Message` and `no variant … Tool`
at the new test's assertions. This test's red is the most clearly structural of the four: before this
slice the arguments never re-entered a request at all, so there was no echo for a redactor to have
missed. It does not prove a fixed leak; it pins that the new path cannot open one.

**T4 — the wire.** `error: could not compile skein-gateway (test "openai_compat") due to 12 previous
errors`, including `error[E0560]: struct Message has no field named tool_call_id` and
`error[E0609]: no field id on type &skein_core::ToolCall`.

**T7 — the pre-existing assertions, as a red before they were touched.** After T5/T6 landed and
before any existing test was reworded, `cargo test --workspace --no-fail-fast` reported **19 failing
tests across six files** — the eighteen `[tool_result …]` label sites plus
`two_tool_calls_in_one_turn_run_in_declaration_order`, which asserted the fed-back tool *name* as a
substring and so depended on the label without naming it. That is the blast radius the plan
predicted, measured rather than estimated, and every one of the nineteen was a site where the label
was load-bearing.

## Live verification (T8)

Run on 2026-09-04 against **`gemma4:latest`** on this machine's Ollama, through the real
`OpenAiCompatClient`, the real `EmbeddedServer` and a real filesystem root holding `alpha.txt` = `7`
and `gamma.txt` = `4`. Nothing below the model is a double.

```
$env:SKEIN_LIVE_MODEL = "gemma4:latest"
cargo test -p skein-connectors --test governed_fs_run a_live_model_reads_two_files -- --ignored --nocapture
```

The model emitted **two calls in one turn**, with its own ids, and the second request replayed them
with each answer naming the call it belongs to. The chain's second `LlmRequest`, verbatim, with the
tool catalogue elided:

```json
{"messages":[
 {"parts":[{"text":"Read the files alpha.txt and gamma.txt. Reply with one line per file, in the form <name>=<contents>.","type":"text"}],"role":"user"},
 {"parts":[{"text":"","type":"text"}],"role":"assistant","tool_calls":[
   {"args":{"path":"alpha.txt"},"id":"call_oceu0gno","tool":"fs_read"},
   {"args":{"path":"gamma.txt"},"id":"call_eu7wcyft","tool":"fs_read"}]},
 {"parts":[{"text":"{\"content\":[{\"type\":\"text\",\"text\":\"7\"}],\"isError\":false}","type":"text"}],"role":"tool","tool_call_id":"call_oceu0gno"},
 {"parts":[{"text":"{\"content\":[{\"type\":\"text\",\"text\":\"4\"}],\"isError\":false}","type":"text"}],"role":"tool","tool_call_id":"call_eu7wcyft"}],
 "run_id":"run-live","tools":[…]}
```

```
exit = FinalOutput
answer = "alpha.txt=7\ngamma.txt=4"
test a_live_model_reads_two_files_and_is_answered_by_id ... ok
```

The answer is **correct**, and it is the pairing the old shape's own measurement got wrong 0/6 times.
The `"content":null` the provider sent for that turn still yields an empty `text` part — unchanged
and right, because the model said nothing; what it *did* is now beside it rather than discarded.

## Close (T9)

`cargo test --workspace` — **240 passed, 0 failed, 6 ignored**, from a baseline of 234/0/5. The delta
of **+6 passed** is exactly this slice's new deterministic tests, and **+1 ignored** is the live one:

| target | baseline | close | added |
|---|---|---|---|
| `native_loop` | 25 | 28 | multi-call round-trip, forged-label prompt, secret in echoed args |
| `openai_compat` | 15 (+1 ign) | 17 (+1 ign) | no-dangling-id invariant, missing-id fallback |
| `governed_fs_run` | 4 (+1 ign) | 6 (+2 ign) | two reads answered by id through the real chain; the live two-file test |

Every other target is unchanged in count. `cargo fmt --all --check` and
`cargo clippy --workspace --all-targets -- -D warnings` are clean.

## Deviations from the plan, stated

1. **The gateway module doc never carried the claim T6 said to correct.** The plan directed a fix to
   `skein-gateway`'s module doc for *"the tool-result feedback path is deliberately left as it is"*.
   That sentence is not there and never was: it lives only in `specs/015-tool-advertisement/plan.md`
   and, in a variant wording, in that slice's `spec.md`. Verified by grep. The three `specs/015-…`
   entries now point at this slice; the gateway source needed no doc change beyond the new types'
   own comments.
2. **The plan's "exactly seven files assert the old label" is right; its own bullet list has eight
   entries.** Seven test files assert the label (eighteen sites). The eighth, `openai_compat.rs`,
   changes because its `Message { role, parts }` struct literal gains the new fields, not because it
   asserted the label. Recorded so the count is not read as an error in either direction.
3. **T3 was written as a new sibling test rather than by extending
   `the_redacted_llm_payloads_are_still_parseable_turn_request_and_response`.** That existing test's
   value is precisely that its turn involves **no** tool, which is the FR-009 byte-identity path;
   extending it to a tool-bearing turn would have spent it. The new test carries the tool-bearing
   parseability and the `verify_chain` assertion the plan asked for.
4. **`captured_requests` in `governed_fs_run.rs` took a run id.** It had `"run-fs"` hard-coded, so
   the new live test — which runs as `run-live` — silently read an empty list rather than failing.
   The live run caught it (`asked [], answered []`) and the helper was parameterized; its five
   existing callers pass `"run-fs"` and assert exactly what they did before.
5. **One commit, not several.** `Role::Tool` and `skein-gateway`'s exhaustive `match` on `Role` are
   inseparable: any split puts a non-compiling or test-red commit in the history. The migration is
   one commit, which is what `plan.md`'s rollback paragraph already assumes.

## Inherited failure, not introduced

`a_live_model_calls_a_real_fs_tool` — slice 016's `#[ignore]`d live test — **fails, and failed before
this slice.** It searches the `StepKind::ToolResult` *ledger payload* for `escaped(FILE_CONTENTS)`,
but that payload nests the tool's `CallToolResult` JSON as a string inside the `CapturedResult` JSON,
so the file's newline is double-escaped there while `escaped()` produces it once. Because the test is
`#[ignore]`d it has never run under `cargo test --workspace`, so the defect has been invisible since
it was written.

Proven inherited rather than asserted: the same command in a scratch worktree detached at `dev`
`6873137`, before any commit of this slice, fails identically with a byte-identical payload. And
`git diff dev -- crates/skein-core/src/tool.rs` shows no change to the `CapturedResult` construction
or to the `StepKind::ToolResult` append, so this slice cannot have altered that payload's shape. Left
unfixed deliberately: it is outside this slice's scope and belongs to whoever owns that test next.

## Next slice

Carried in `plan.md`'s *Next slice*. The `role:"tool"` / `tool_call_id` replay item, carried since
slice 015 and repeated in 016, 017, 018 and 019, is **closed by this slice** and drops off the list.
