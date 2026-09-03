# Tasks: the `fs` connector (v0 slice)

**Spec:** `specs/016-fs-connector/spec.md` · **Plan:** `specs/016-fs-connector/plan.md` · TDD
(red→green), product code in a new `crates/skein-connectors` plus `crates/skein-mcp` and
`crates/skein-cli`, branch `016-fs-connector` cut from `dev` after slice 015 merged.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the capability lives in a library crate; `skein-cli` gains one flag and the
  wiring that turns it into a transport plus two named policies. `skein-core` is **untouched** by
  this slice: 015 already built everything it needed · II Local-first ✅ NON-NEGOTIABLE. The
  connector is **in-process** — an `rmcp` client and server over a `tokio::io::duplex`, no socket, no
  child process, no Node runtime. A third-party out-of-process server was rejected on exactly this
  ground (plan D1). No new egress path exists to guard
- III Test-First ✅ every step's red observed and recorded in `## Observed red` before its green ·
  IV Inverted coupling ✅ `skein-core` still names no protocol. `skein-mcp` remains the only crate
  naming MCP as a **client** and `skein-connectors` becomes the only one naming it as a **server** —
  the invariant is amended in both crates rather than left stale (FR-002). `skein-cli` reaches the
  connector only through `ToolTransport`
- V Traceability ✅ no new `StepKind` and none needed. A tool call lands as
  `ToolCall`/`Approval`/`ToolResult` through the gateway 005 built, and the advertisement travels
  inside the captured `LlmRequest`. The headline test asserts the whole sequence and `verify_chain`
- VI Security ✅ deny-by-default at three layers, each with its own test: the **policy** refuses an
  unlisted name before the transport is consulted (`fs_write` under `chat_policy` never reaches the
  server); the **server** refuses a path outside its root as a tool error; and `--fs-root` is
  **opt-in**, so absent it no tool exists at all. `fs_write` is `approved` only where a human can be
  asked, and the reason is written at both wiring sites
- VII Neutrality ✅ three tools, one flag, one crate. No glob, no recursion, no rename/delete/mkdir,
  no config hierarchy, no trust registry, no `git`, no `shell`. The server is ~150 lines because
  containment is the only thing it has to be careful about
- VIII Loop discipline ✅ NON-NEGOTIABLE and untouched. `advertise` still runs after the pre-flight
  budget check; a tool refusal is still survivable and a transport failure still fatal; the exits,
  the probe and the controller are unchanged
- Cross-platform ⚠️ **the highest-risk slice yet for this row, and the one place it is load-bearing.**
  The containment code is filesystem semantics: `canonicalize` is `\\?\`-verbatim on Windows and
  symlink creation needs a privilege there that it does not on Unix. There is **no `#[cfg]` in the
  containment code**, so the same bodies must pass everywhere; the symlink test skips cleanly when
  the OS refuses to create one rather than failing. The Windows leg is observed locally; macOS and
  Linux remain unobserved until the repository has a remote

## Tasks
- [x] **T0** `specs/016-fs-connector/{spec.md,plan.md,tasks.md}`; branch `016-fs-connector` cut from
      `dev` with slice 015 merged
- [x] **T1** control baseline: `cargo test --workspace` before any edit — **132 passed, 1 ignored**
- [x] **T2** RED→GREEN — `crates/skein-connectors` with `FsRoot`, and the amended MCP invariant
- [x] **T3** RED→GREEN — `FsServer`: `#[tool_router]` and the three `#[tool]` methods over derived
      schemas
- [x] **T4** RED→GREEN — `RmcpToolTransport::list`, then `LocalConnector` / `fs_connector`
- [x] **T5** RED→GREEN — the headline end-to-end test: advertisement → model tool request → real
      gateway → real connector → real server → real file contents → chain verifies
- [x] **T6** RED→GREEN — the refusal twins: unlisted `fs_write` never reaches the server; an
      out-of-root read is refused by the server and the run survives
- [x] **T7** RED→GREEN — a secret in a file's **contents** is scrubbed from every Ledger payload
- [x] **T8** RED→GREEN — `skein-cli`: `ToolArgs`, `ConfiguredTools`, the two policies, `--fs-root`
- [x] **T9** RED→GREEN — CLI acceptance tests against the real binary
- [x] **T10** the `#[ignore]`d live-model test, gated on `SKEIN_LIVE_MODEL`
- [x] **T11** gates, control diff, dependency drift, close-out
- [ ] **T12** hand-verification against live Ollama — **not part of the implementation run**;
      performed separately and recorded below under `## Live verification`

## Control baseline (T1)

`cargo test --workspace` on `016-fs-connector` @ `c4da8f7` (identical to `dev`), working tree clean,
2026-09-03, before any code edit: **132 passed, 0 failed, 1 ignored** — `acp_session` 16,
`cli_acp_agent` 4, `cli_chat` 8, `cli_ledger` 8, `cli_secret` 2, `core` 19, `native_loop` 25,
`tool_gateway` 14, `governed_run` 2, `openai_compat` 15 (+1 ignored, the optional live-Ollama test),
`rmcp_gateway` 7, `silo_ledger` 7, `silo_secret` 5. The five `src/lib.rs`/`src/main.rs` unit-test
targets and the five doc-test targets each contribute `0 passed`. This is slice 015's recorded close
figure exactly (120 + its 12), and it is the number T11 diffs against.

## Observed red (Constitution III)

To be recorded per step, before each green.

## Gates (T11)

To be recorded.
