# Tasks: the provider's literal bytes as a `WireExchange` step (v0 slice)

**Spec:** `specs/023-raw-wire-capture/spec.md` · **Plan:** `specs/023-raw-wire-capture/plan.md` ·
TDD (red→green), branch `023-raw-wire-capture` cut from `dev` at `12c14f5`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)

- **I Headless core** ✅ no CLI of its own, no new flag, no new command, no new subcommand argument.
  `skein ledger log` and `skein ledger show` render the new kind with **zero** CLI change, because
  `skein-cli/src/ledger.rs`'s `kind_name` derives the column from `serde_json::to_value(kind)`
  rather than matching the enum. Verified by running both against a live chain (see *Live
  verification*).
- **II Local-first** ✅ no new dependency, no `Cargo.toml` change, no new network-capable code. The
  slice captures bytes from the one HTTP call that already existed and adds no second one. `ureq`
  stays declared with `default-features = false`, so no TLS backend and no content-encoding decode.
- **III Test-First** ✅ three reds observed and recorded verbatim below, each isolating a different
  failure mode. Red A is the plan's T1–T4 compile red; reds B and C are the two *silent* defects the
  plan singled out as the slice's real risks, reproduced deliberately by reverting one line each.
- **IV Inverted coupling** ✅ `skein-core` gains a `WireExchange` struct that names **no protocol**:
  a url, a status, and two opaque strings. It does not know the bodies are JSON, or OpenAI-shaped,
  or HTTP-framed. Every fact about the chat-completions format stays in `skein-gateway`, which
  remains the one crate that names it. The port grows a defaulted method, so no other crate reasons
  about a client that has a wire.
- **V Traceability** ✅ this slice exists *for* Principle V. It closes the gap design §4.11 named as
  the Ledger's first capture obligation and that Spike 1 made its deciding criterion C1. Capture is
  unconditional and unbypassable by construction (D5 — no flag, no config key). The new variant is
  proven not to perturb any existing step's id by an upgrade-shape test that persists a pre-023
  chain, reopens it, and compares ids (`s8`).
- **VI Security** ✅ NON-NEGOTIABLE. Two independent halves. **Redaction:** the naive implementation
  leaks, and the slice proves it rather than asserting it — red B below shows a quote-bearing secret
  reaching the chain in cleartext under a `redact`-only implementation, which is why
  `Redactor::redact_wire` exists and matches both needle forms. **Credentials:** bodies only, never
  headers (D6), so no `Authorization` header can become a chain payload before the slice that
  designs for one.
- **VII Neutrality** ✅ one `StepKind` variant, one payload struct, one defaulted trait method, one
  `Redactor` method, one field on `OpenAiCompatClient`, one forwarding override on the ACP
  decorator. No new tool, flag, crate, dependency, config key or CLI surface. Five alternatives were
  considered and rejected with a reason each in `spec.md`'s register — including three
  (`WireSink`, an observer closure, bytes on `TurnResponse`) that would each have been more
  machinery for the same outcome.
- **VIII Loop discipline** ✅ NON-NEGOTIABLE and untouched. The controller, the budget, the probe and
  the exit conditions are neither read nor written by this slice. The one control-flow change in
  `NativeLoop::run` defers an existing `?` past a new append; it changes *when* the error
  propagates within a single iteration, never *whether* it does, and both failure-path tests assert
  the run still ends in `Err(SkeinError::Model(_))`.
- **Cross-platform** ✅ no `#[cfg]`, no platform API, no filesystem or process work. The stub
  provider is a `std::net::TcpListener` on loopback, as it already was. The one `#[ignore]`d live
  test is gated on an environment variable, as slices 019–022 established.

## Tasks

- [x] **T0** branch verified against `dev` at `12c14f5`, control baseline measured, and
      `specs/023-raw-wire-capture/{spec.md,plan.md,tasks.md}` written (see *Deviations* 1)
- [x] **T1** RED — the headline claim: the exchange bytes equal the stub's observed bytes, both
      directions
- [x] **T2** RED — redaction with a quote-bearing secret, plus a plain-secret sibling
- [x] **T3** RED — backward compatibility from a persisted pre-023 chain, ids unchanged
- [x] **T4** RED — the two negative properties: (a) an unreachable provider appends nothing,
      (b) a non-2xx and an unparseable body each still record the exchange
- [x] **T5** GREEN — `skein-core`: the variant (`ledger.rs`), the struct and the defaulted port
      method (`model.rs`), the re-export (`lib.rs`), `redact_wire` (`tool.rs`), the
      turn/take/append/`?` sequence (`native_loop.rs`)
- [x] **T6** GREEN — `skein-gateway`: `last_exchange`, the capture in `turn`, the override
- [x] **T7** the existing assertions that genuinely change — four test files (see *Deviations* 2)
- [x] **T8** live verification against a real local model — **part of this run**
- [x] **T9** close-out: the residual carried since slice 011 drops off, two new residuals recorded

## Control baseline (T0)

Measured on this worktree at `12c14f5` with the slice's changes fully reverted
(`git checkout -- .`), before any edit:

| gate | result |
|---|---|
| `cargo fmt --all --check` | pass, no output |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass, `Finished dev profile` |
| `cargo test --workspace` | **240 passed, 0 failed, 6 ignored** |

At close, on the same worktree with the slice applied: fmt pass, clippy pass,
**246 passed, 0 failed, 7 ignored**. The delta is **+6 passed** (five new tests in
`skein-gateway/tests/governed_run.rs`, one new `s8` in `skein-silo/tests/silo_ledger.rs`) and
**+1 ignored** (the live test in `skein-gateway/tests/openai_compat.rs`). No test was deleted,
renamed or disabled; nothing moved from passed to ignored.

## Observed red

Three reds, each isolating a distinct failure mode. Red A is the absence of the feature. Reds B and
C are the two defects `plan.md`'s risk table named as the slice's real hazards — both **silent**,
in the precise sense that the headline test stays green while they are present. Each was reproduced
by reverting exactly one decision from the finished implementation, so the red is evidence about
*this* code rather than about a sketch of it.

### Red A — T1/T2/T3/T4, the feature does not exist

Tests applied, sources at `12c14f5`. `cargo test --workspace`:

```
error[E0599]: no variant, associated function, or constant named `WireExchange` found for enum `StepKind` in the current scope   (x6)
error[E0432]: unresolved import `skein_core::WireExchange`                                                                       (x2)
error[E0599]: no method named `take_wire_exchange` found for struct `OpenAiCompatClient` in the current scope                     (x2)
error: could not compile `skein-gateway` (test "governed_run") due to 6 previous errors
error: could not compile `skein-gateway` (test "openai_compat") due to 2 previous errors
error: could not compile `skein-silo` (test "silo_ledger") due to 2 previous errors
```

Exactly the three absences the slice adds: the variant, the payload type, the port method.

### Red B — T2, the redaction hole, with the full implementation otherwise present

The one change: `native_loop.rs` scrubs the two bodies with the **existing** `redact` instead of
`redact_wire` — i.e. the implementation a reader who trusted "the existing Redactor already covers
`LlmRequest`/`LlmResponse`" would have written.

```
running 7 tests
test a_plain_secret_is_scrubbed_from_the_exchange ... ok
test the_chain_records_the_literal_bytes_of_the_exchange ... ok
test an_unparseable_reply_still_leaves_the_bytes_that_caused_it_on_the_chain ... ok
test a_provider_error_status_still_leaves_the_bytes_that_caused_it_on_the_chain ... ok
test an_end_to_end_run_against_a_stub_provider_lands_on_the_chain ... ok
test a_provider_failure_ends_the_run_with_the_request_already_on_the_chain ... ok
test a_quote_bearing_secret_is_scrubbed_from_the_exchange_it_escaped_into ... FAILED

---- a_quote_bearing_secret_is_scrubbed_from_the_exchange_it_escaped_into stdout ----
panicked at crates\skein-gateway\tests\governed_run.rs:403:5:
the escaped secret reached the chain: {"model":"llama3.1","messages":[{"role":"user","content":"the password is pa\"ss-w0rd"}],"stream":false}

test result: FAILED. 6 passed; 1 failed
```

**This is the slice's most important red.** The secret is on the chain in cleartext, and note what
stayed green: the byte-equality test, both failure-path tests, and — decisively — the
*plain*-secret sibling. A suite with only one redaction test would have certified this leak.
`redact_wire` is therefore load-bearing, not decoration.

### Red C — T4(b), the failure-path capture, with the full implementation otherwise present

The one change: `let resp = self.client.turn(&req)?;` written on a single line, the deferred `?`
removed — i.e. the idiomatic Rust that `plan.md`'s risk table predicted an implementer would write.

```
test an_unparseable_reply_still_leaves_the_bytes_that_caused_it_on_the_chain ... FAILED
test a_provider_error_status_still_leaves_the_bytes_that_caused_it_on_the_chain ... FAILED
test the_chain_records_the_literal_bytes_of_the_exchange ... ok
test a_quote_bearing_secret_is_scrubbed_from_the_exchange_it_escaped_into ... ok
test a_plain_secret_is_scrubbed_from_the_exchange ... ok
test an_end_to_end_run_against_a_stub_provider_lands_on_the_chain ... ok
test a_provider_failure_ends_the_run_with_the_request_already_on_the_chain ... ok

---- an_unparseable_reply_still_leaves_the_bytes_that_caused_it_on_the_chain stdout ----
assertion `left == right` failed
  left: [IterationBoundary, LlmRequest]
 right: [IterationBoundary, LlmRequest, WireExchange]

---- a_provider_error_status_still_leaves_the_bytes_that_caused_it_on_the_chain stdout ----
assertion `left == right` failed
  left: [IterationBoundary, LlmRequest]
 right: [IterationBoundary, LlmRequest, WireExchange]

test result: FAILED. 5 passed; 2 failed
```

The chain stops at the translated request and the bytes that caused the failure are gone — the
exact case the slice exists to capture, lost, while every happy-path test passes. This is why the
`?` in `native_loop.rs` is deferred past the append and why that line carries a comment.

## Live verification (T8)

Against Ollama at `http://localhost:11434/v1`, model `gemma4:latest`, on Windows 11.

**The `#[ignore]`d test**, run with `$env:SKEIN_LIVE_MODEL = "gemma4:latest"`:

```
running 2 tests
live wire gemma4:latest @ http://localhost:11434/v1
  url      = http://localhost:11434/v1/chat/completions
  status   = 200
  request  = {"model":"gemma4:latest","messages":[{"role":"user","content":"Reply with exactly the word: pong"}],"stream":false}
  response = {"id":"chatcmpl-260","object":"chat.completion","created":1788487553,"model":"gemma4:latest","system_fingerprint":"fp_ollama","choices":[{"index":0,"message":{"role":"assistant","content":"pong"},"finish_reason":"stop"}],"usage":{"prompt_tokens":23,"prompt_tokens_details":{"cached_tokens":0},"completion_tokens":2,"total_tokens":25}}

test a_live_local_provider_exchange_is_captured_with_its_own_metering ... ok
test a_live_local_provider_answers ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 17 filtered out
```

**Worth recording:** the captured response carries `system_fingerprint` and
`prompt_tokens_details`, **neither of which `ChatResponse` has a field for**. They were being
silently discarded, and before this slice no artifact of the product contained them. That is
precisely the class of divergence the chain previously could not express.

**The hand-verification**, per the plan's acceptance criteria. `skein chat --root <tmp> --silo
live023 --base-url http://localhost:11434/v1 --model gemma4:latest --run-id t8-live --prompt "What
is 6 times 7? Answer with just the number."` answered `42`, then:

```
> skein ledger log --run t8-live
t8-live 0  iteration_boundary  94a3b30a082aabd21dd63e5b11a319010d95d45ee4c22f682ced091527430c25
t8-live 1  llm_request         33c1cb540629ad3444f03f52b87e2c6f0b5466d8d20c2f5fa17a6d54c93ff61a
t8-live 2  wire_exchange       905828cc407ff58ca98e0b13c6d7c3103581ea353b174b6dc6f61bc647a4a2eb
t8-live 3  llm_response        0cfa2716f9f12faca00b3dda47dc04d4025490d5acdc7710c466d104ae3a887e
t8-live 4  budget_spent        770b0b7b26cb202d0f328f2eee4ec4610ec830a1328a4cfd627d2444b78da5c5
t8-live 5  exit                5e916146c6227359e2e543646dad340a3052bd4cc14d1bce3cf85b4ca763b09e
```

`skein ledger show 9058…` printed `parent 33c1…` — the `llm_request` step, confirming the placement
D1 specifies — and the payload:

```json
{"url":"http://localhost:11434/v1/chat/completions","status":200,
 "request":"{\"model\":\"gemma4:latest\",\"messages\":[{\"role\":\"user\",\"content\":\"What is 6 times 7? Answer with just the number.\"}],\"stream\":false}",
 "response":"{\"id\":\"chatcmpl-505\",…,\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"42\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":30,\"prompt_tokens_details\":{\"cached_tokens\":0},\"completion_tokens\":3,\"total_tokens\":33}}\n"}
```

The three acceptance checks, each cross-read against an **independently produced** step:

| check | evidence |
|---|---|
| (i) captured `request.messages` matches the `LlmRequest` step | both carry exactly `What is 6 times 7? Answer with just the number.`; the `llm_request` payload is `{"run_id":"t8-live","messages":[{"role":"user","parts":[{"type":"text","text":"What is 6 times 7? Answer with just the number."}]}]}` |
| (ii) captured `response.usage.total_tokens` equals `BudgetSpent` | `33` in the captured bytes; the `budget_spent` payload is exactly `33` |
| (iii) the chain verifies | `skein ledger verify --run t8-live` → `t8-live	ok	6 steps` |

A further byte-exactness signal the stubs cannot give: the captured response **ends in `\n`**,
Ollama's own trailing newline, preserved rather than normalized away. A re-serialization would have
dropped it.

## Deviations from the plan, stated

1. **The plan's §0 first correction was already stale, in the slice's favour.** `plan.md` states the
   worktree "is checked out at `d364405`, which is behind `dev` (`12c14f5`)" and makes rebasing the
   first action of the run. It was **not** behind: `git log` and `git merge-base HEAD dev` both
   reported `12c14f5`, identical to `dev`, with zero commits of the branch's own. No rebase, merge
   or fast-forward was performed or needed, and the baseline was measured directly at `12c14f5` as
   the plan intends. The plan's substantive point — that every anchor in it is a `dev` anchor and
   slices 021/022 are present — holds and was relied on.
2. **T7's counts were stale and the plan said to recount rather than trust them.** Recounted against
   `dev`: the affected set is **four** files, not three. `skein-cli/tests/cli_chat.rs` (4 kind
   vectors, 4 step counts: 5→6, 9→11, 12→14, 12→14) and `skein-cli/tests/cli_acp_agent.rs` as named,
   plus **`skein-connectors/tests/governed_fs_run.rs` and `governed_git_run.rs`**, which the plan
   asserted were stub-driven and would not change. They are not stubs — both drive the real
   `OpenAiCompatClient` against an in-test HTTP provider, so both genuinely gain two `WireExchange`
   entries. This is a correction to the plan's inventory, not to its design: the rule it stated
   ("only tests that drive the real client are affected") is exactly what the tree shows.
3. **One source file outside the plan's stated blast radius changed:
   `skein-acp/src/cancel.rs`.** `CancellableModel` is a **decorator** over an arbitrary
   `ModelClient`. Inheriting the defaulted `take_wire_exchange` would have silently dropped the
   exchange of every ACP-driven run — a traceability gap with nothing to notice it, since the
   default returns `None` without erroring. It forwards to the inner client in three lines. The
   plan's rule was right and its file list simply missed a decorator; the deviation is recorded here
   rather than folded in silently.
4. **`OpenAiCompatClient::post` gained a `url` parameter.** `turn` needs the URL for the
   `WireExchange` and `post` was computing it internally, so it is computed once in `turn` and
   passed down. This keeps one call to `chat_completions_url()` per turn and makes the captured URL
   provably the one that was posted to, rather than a second derivation of it.
5. **The implementation was already present, uncommitted, at the start of this run**, left by an
   earlier session; no `spec.md`, `tasks.md` or commits existed. Rather than accept it on trust, the
   reds above were reproduced against it — that is why reds B and C revert one decision each from
   finished code instead of building up from nothing. The TDD claim this file makes is therefore
   about observed behaviour of the shipped implementation, and is stronger evidence than a
   build-up ordering would have been, not weaker.

## Close (T9)

`cargo fmt --all --check` pass · `cargo clippy --workspace --all-targets -- -D warnings` pass ·
`cargo test --workspace` **246 passed, 0 failed, 7 ignored**, against a baseline of 240/0/6. The
delta is accounted for line by line under *Control baseline* above.

## Next slice

**Closed by this slice and dropping off the list:** **raw-wire-byte capture** — *"a `StepKind` for
the provider's literal request and response bytes"* — carried in every *Next slice* list since slice
011 and named in `specs/012-model-gateway/tasks.md`, 013, 014, 015, 016 and referenced in 017–022.

**Carried forward unchanged:** the `canonicalize`-to-open TOCTOU residual, model obedience to
instructions found in tool content, streaming (SSE), provider authentication, a config file, the
egress-policy layer.

**New, created or discovered by this slice:**

- [ ] **HTTP header capture on `WireExchange`** — the request line and headers, to be taken **in the
      same slice as provider authentication** and not before, because the moment headers are
      captured an `Authorization: Bearer` becomes a chain payload and inherits that item's entire
      Principle VI design. `plan.md` D6.
- [ ] **The `ToolResult` JSON-escape redaction hole** — `ToolGateway::call_captured` scrubs a tool's
      `content` with `redact`, so a quote-bearing secret inside a tool result that is *itself* JSON
      text survives in escaped form, exactly as red B demonstrates for the wire bodies. The fix is
      to point that call site at `Redactor::redact_wire`, which this slice adds; it is a one-line
      change with a different payload and different tests, so it is a slice of its own.
