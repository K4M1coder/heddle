# Tasks: deny-by-default for tool identity (v0 strict-local)

**Spec:** `specs/007-tool-allowlist/spec.md` · TDD (red→green), product code in
`crates/heddle-core`, branch `007-tool-allowlist` cut from `dev`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ (library API; no UI, no bin) · II Local-first ✅ (no network, no new deps,
  no `Cargo.toml` touched)
- III Test-First ✅ (T2 red before T3 green) · IV Inverted coupling ✅ (the transport seam is
  untouched; `ToolPolicy` names no protocol and no server)
- V Traceability ✅ (an unlisted tool is refused through the *existing* `[ToolCall, Approval]`
  shape and the existing `HeddleError::ToolDenied` — no new `StepKind`, no second denial path;
  `verify_chain` asserted on the new refusal)
- VI Security ✅ (this slice restores the principle's opening clause: deny-by-default now
  governs tool *identity*, not only mutation, and fails closed — `ToolPolicy::new(Vec::new(),
  Vec::new())` allows nothing) · VII Neutrality ✅ (no new module, crate, `StepKind`, dependency
  or builder; the superseded `mutating` field is deleted rather than kept alongside)
- VIII Loop discipline ✅ **(a)** `loop_ctl.rs` and `native_loop.rs` byte-identical; a denied
  tool burns exactly the iteration budget the mutating-denial path already burns. **(b)** the
  probe still runs after the turn's tools, unchanged. **(c)** per-step capture of the attempt
  and the decision is unchanged; the refusal reason is the only new text. **(d)** HITL
  escalation still deferred — approval stays a separate configured list, deliberately not folded
  into `ToolAccess`, so that seam survives.
- Cross-platform ✅ (pure Rust, no `#[cfg]`; `core.yml`'s `paths:` already covers `crates/**` and
  its toolchain pin is already 1.97 — confirmed, not edited).

## Tasks
- [x] **T0** `specs/007-tool-allowlist/{spec.md,plan.md,tasks.md}`; branch `007-tool-allowlist`
      cut from `dev`
- [x] **T1** record the control baseline: `cargo test --workspace` on `dev` before any edit
- [x] **T2** RED — the five new tests, and all four `ToolPolicy::new` construction sites
      migrated in the same commit (the workspace cannot compile between the two); compile
      failure observed and recorded below
- [x] **T3** GREEN — `ToolAccess`, `ToolPolicy.allowed`, the rewritten `decide`, and the
      `lib.rs` re-export (FR-001/002/003/004/005)
- [x] **T4** gates: `cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets
      -- -D warnings` clean (no `type_complexity` objection to the tuple vector, so the
      `AllowedTools` alias the plan held in reserve was not needed); `cargo test --workspace`
      **40/40** — 35 pre-existing + 5 new, 2026-09-03. Windows leg observed; macOS and Linux
      unobserved until the repository has a remote (SC-001)
- [x] **T5** control diff: `git diff dev` empty on `src/native_loop.rs`, `src/loop_ctl.rs`,
      `src/ledger.rs`, `src/error.rs`, `src/model.rs`, `src/content.rs`,
      `heddle-mcp/src/lib.rs` and `tests/core.rs`. In the three test files every *removed*
      line is an import line, a line inside `fn gateway` / `fn live_server`, or the `no_tools`
      doc comment — no pre-existing `#[test]` body changed, so specs 004/005/006 stay
      controls (SC-004, FR-006)
- [x] **T6** no drift: `git diff dev` empty on every `Cargo.toml`, on `spikes/` and on
      `.github/`. `core.yml` already runs the three gates on `crates/**` at toolchain 1.97 —
      confirmed, unedited (SC-003, SC-005)
- [x] **T7** ticked the allowlist bullet in `specs/006-loop-tool-wiring/tasks.md`; this spec's
      Status set to `Implemented (v0 slice)` at **40/40**, 2026-09-03

## Control baseline (T1)

`cargo test --workspace` on `dev` / `31051cb`, working tree clean, 2026-09-03: **35 passing** —
`tests/core.rs` 6, `tests/native_loop.rs` 17, `tests/tool_gateway.rs` 6,
`heddle-mcp/tests/rmcp_gateway.rs` 6; 0 failed, 0 ignored. This is the number T4 diffs against:
35 pre-existing + 5 new = 40.

## Observed red (Constitution III)

- **T2** `cargo build --workspace --all-targets`, 2026-09-03:
  - `error[E0432]: unresolved import heddle_core::ToolAccess`
    (`crates/heddle-core/tests/tool_gateway.rs`,
    `crates/heddle-core/tests/native_loop.rs`,
    `crates/heddle-mcp/tests/rmcp_gateway.rs:16:52`)
  - `error: could not compile heddle-core (test "tool_gateway") due to 1 previous error`
  - `error: could not compile heddle-core (test "native_loop") due to 1 previous error`
  - `error: could not compile heddle-mcp (test "rmcp_gateway") due to 1 previous error`
  - The `ToolPolicy::new` argument-type errors the plan also expected are *not* in this
    output: rustc abandons a crate once import resolution fails, so the unresolved
    `ToolAccess` import is the only diagnostic each test crate reaches. Same red, one
    diagnostic per crate instead of two.

## Next slice (not this feature)
- [x] ACP client facade over the native loop + gateway — spec 008, `crates/heddle-acp`
- [ ] a typed `Content::ToolResult` variant and real prompt-injection defense; redacting the
      tool *name* on its way into the Ledger
- [ ] tool advertisement on `TurnRequest`, which needs tool discovery (`tools/list`)
- [ ] cost / wall-clock budgets and `Exit::Error`; the `ts`/`principal`/`silo` fields design
      §4.11 sketches on `Step`
- [ ] silo-backed durable Ledger (SQLite) + `SecretProvider` (OS keychain)
- [ ] `heddle-cli` reference client
