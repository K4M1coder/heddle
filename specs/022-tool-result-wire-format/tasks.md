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
- [x] **T6** GREEN — `skein-gateway`: D3 and D5, and the module doc's now-false claim corrected
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

*(recorded per step below)*

## Close (T9)

*(recorded at close)*

## Deviations from the plan, stated

*(recorded as they occur)*

## Live verification (T8)

*(recorded at T8)*

## Next slice

Carried in `plan.md`'s *Next slice*. The `role:"tool"` / `tool_call_id` replay item, carried since
slice 015 and repeated in 016, 017, 018 and 019, is **closed by this slice** and drops off the list.
