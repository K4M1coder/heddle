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
  verbatim under `## Observed red` before its green, and both reds are real — the Allow path failed
  on the missing permission handler exactly as the spec predicted · IV Inverted coupling ✅
  untouched. `skein-cli` is the one crate that already depends on both `skein-acp` and
  `skein-connectors`, which is why the proof lives here and not in either of them: a real `fs_write`
  effect on disk needs `skein-connectors`, which `skein-acp` does not depend on and must not
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
- [x] **T2** helpers, no assertions yet: `tool_call_reply` and `last_message` copied from
      `cli_chat.rs`, `struct Answered`, `fn run_answering`
- [x] **T3** RED→GREEN — the Allow path: `an_acp_client_that_allows_lets_a_real_fs_write_execute`
- [x] **T4** RED→GREEN — the Deny path:
      `an_acp_client_that_rejects_stops_the_fs_write_and_the_run_survives`
- [x] **T5** no pre-existing assertion changed: `git diff dev -- crates/skein-cli/tests/cli_acp_agent.rs`
      is append-only apart from the `use` block
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

*(filled in as each step's red is observed)*
