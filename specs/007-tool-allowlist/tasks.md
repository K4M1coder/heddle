# Tasks: deny-by-default for tool identity (v0 strict-local)

**Spec:** `specs/007-tool-allowlist/spec.md` · TDD (red→green), product code in
`crates/skein-core`, branch `007-tool-allowlist` cut from `dev`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ (library API; no UI, no bin) · II Local-first ✅ (no network, no new deps,
  no `Cargo.toml` touched)
- III Test-First ✅ (T2 red before T3 green) · IV Inverted coupling ✅ (the transport seam is
  untouched; `ToolPolicy` names no protocol and no server)
- V Traceability ✅ (an unlisted tool is refused through the *existing* `[ToolCall, Approval]`
  shape and the existing `SkeinError::ToolDenied` — no new `StepKind`, no second denial path;
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
- [ ] **T5** control diff: `native_loop.rs`, `loop_ctl.rs`, `ledger.rs` and `error.rs` unchanged;
      no pre-existing test *body* changed (SC-004, FR-006)
- [ ] **T6** no drift: `git diff dev` empty on every `Cargo.toml`, on `spikes/` and on
      `.github/` (SC-003, SC-005)
- [ ] **T7** tick the allowlist bullet in `specs/006-loop-tool-wiring/tasks.md`; set this spec's
      Status

## Control baseline (T1)

`cargo test --workspace` on `dev` / `31051cb`, working tree clean, 2026-09-03: **35 passing** —
`tests/core.rs` 6, `tests/native_loop.rs` 17, `tests/tool_gateway.rs` 6,
`skein-mcp/tests/rmcp_gateway.rs` 6; 0 failed, 0 ignored. This is the number T4 diffs against:
35 pre-existing + 5 new = 40.

## Observed red (Constitution III)

- **T2** `cargo build --workspace --all-targets`, 2026-09-03:
  - `error[E0432]: unresolved import skein_core::ToolAccess`
    (`crates/skein-core/tests/tool_gateway.rs`,
    `crates/skein-core/tests/native_loop.rs`,
    `crates/skein-mcp/tests/rmcp_gateway.rs:16:52`)
  - `error: could not compile skein-core (test "tool_gateway") due to 1 previous error`
  - `error: could not compile skein-core (test "native_loop") due to 1 previous error`
  - `error: could not compile skein-mcp (test "rmcp_gateway") due to 1 previous error`
  - The `ToolPolicy::new` argument-type errors the plan also expected are *not* in this
    output: rustc abandons a crate once import resolution fails, so the unresolved
    `ToolAccess` import is the only diagnostic each test crate reaches. Same red, one
    diagnostic per crate instead of two.

## Next slice (not this feature)
- [ ] ACP client facade over the native loop + gateway
- [ ] a typed `Content::ToolResult` variant and real prompt-injection defense; redacting the
      tool *name* on its way into the Ledger
- [ ] tool advertisement on `TurnRequest`, which needs tool discovery (`tools/list`)
- [ ] cost / wall-clock budgets and `Exit::Error`; the `ts`/`principal`/`silo` fields design
      §4.11 sketches on `Step`
- [ ] silo-backed durable Ledger (SQLite) + `SecretProvider` (OS keychain)
- [ ] `skein-cli` reference client
