# Tasks: SSE streaming from the local provider, with live ACP `AgentMessageChunk`s (v0 slice)

**Spec:** `specs/025-streaming-sse/spec.md` · **Plan:** `specs/025-streaming-sse/plan.md` ·
TDD (red→green), branch `025-streaming-sse`, fast-forwarded onto `dev` at `9002f73`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)

*Filled at close (S12).*

## Tasks

- [x] **S0** fast-forward onto `origin/dev` at `9002f73`, measure the control baseline, and write
      `specs/025-streaming-sse/{spec.md,plan.md,tasks.md}`
- [ ] **S1** RED — the accumulator against Ollama's real shape: multi-delta text, `usage` event
- [ ] **S2** RED — tool calls in both shapes, `reasoning` discarded, a stream with no metering refused
- [ ] **S3** RED — the wire capture: literal SSE bytes, `streamed`, the request fields, redaction of a
      quote-bearing secret inside a `data:` payload, and the non-2xx exchange
- [ ] **S4** GREEN — `skein-core`: `TextSink`, the defaulted `set_text_sink`, `WireExchange.streamed`,
      the re-export, and the one carried field in `native_loop.rs`
- [ ] **S5** GREEN — `skein-gateway`: `stream_options`, the chunk types, the bounded byte-oriented
      reader, the accumulator, and the sink push
- [ ] **S6** RED — the ACP transcript: one chunk per delta in order, no duplicate, and a redacted
      secret
- [ ] **S7** GREEN — `skein-acp`: `stream.rs`'s `AcpTextSink`, the `CancellableModel` forward, and the
      session's install and counter
- [ ] **S8** GREEN — suppress the duplicate at the one `project_updates` call site
- [ ] **S9** GREEN — re-frame the two CLI stub providers to SSE
- [ ] **S10** RED then GREEN — the end-to-end proof that a chunk arrives *while `session/prompt` is
      still outstanding*
- [ ] **S11** the live provider tests — **part of this run**
- [ ] **S12** close-out: the reds verbatim, the live verification, the deviations and the residuals

## Control baseline (S0)

Measured on this worktree immediately after the fast-forward to `9002f73`, before any edit:

| gate | result |
|---|---|
| `cargo test --workspace` | **258 passed, 0 failed, 7 ignored** |

## Observed red

*Filled as each red is observed.*

## Live verification (S11)

*Filled at S11.*

## Deviations from the plan, stated

*Filled at close.*

## Close (S12)

*Filled at close.*
