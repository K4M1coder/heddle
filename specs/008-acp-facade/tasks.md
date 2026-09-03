# Tasks: an ACP facade over the native loop (v0 slice)

**Spec:** `specs/008-acp-facade/spec.md` · TDD (red→green), product code in `crates/skein-acp`,
branch `008-acp-facade` cut from `dev`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ (this slice is the first real client boundary; still library-only, no bin,
  no UI) · II Local-first ✅ (byte-stream transport, in-process duplex in tests; no network
  egress, no listening socket, no process spawned)
- III Test-First ✅ (T1's smoke test pins the SDK's structural assumptions before any product
  code; T3 red before T4/T5 green) · IV Inverted coupling ✅ (no `skein-core` file changes at
  all; ACP is adapted through decorators over `ToolTransport` and `ModelClient`)
- V Traceability ✅ (updates are computed from `Ledger::log(run_id)` — no second record; run ids
  `{session_id}#{n}` keep one `Exit` per chain and `verify_chain` per-run semantics intact)
- VI Security ✅ (`ToolPolicy::decide` is gate 1 and runs first, structurally: the permission
  decorator sits behind `ToolGateway`'s transport call, so an unlisted tool never becomes a
  permission request; the request carries the tool name only, never the arguments)
- VII Neutrality ✅ (one crate, one protocol version, one transport shape, five ACP methods;
  everything else named as deferred below)
- VIII Loop discipline ✅ **(a)** `LoopController` unchanged and still the sole authority on
  termination — ACP cannot extend a budget. **(b)** `ProgressProbe` untouched; the client cannot
  feed progress. **(c)** per-step capture unchanged. **(d)** advanced, not closed:
  `session/request_permission` is the first real human-escalation surface, while
  `Exit::HumanReject` stays unreached — a declined permission is a tool denial, not a run
  rejection.
- Cross-platform ✅ (pure Rust, no `#[cfg]` in our code; the SDK carries `rustix`/`windows-sys`
  itself. `core.yml`'s `paths:` already covers `crates/**` at 1.97 and the SDK's MSRV is 1.88 —
  confirmed, not edited).

## Tasks
- [ ] **T0** `specs/008-acp-facade/{spec.md,plan.md,tasks.md}`; branch `008-acp-facade` cut from
      `dev`
- [ ] **T1** pin the SDK's API surface against the vendored 2.0.0 source *before* writing product
      code, and prove the two structural assumptions with a throwaway smoke test
- [ ] **T2** control baseline: `cargo test --workspace` on `dev` before any edit
- [ ] **T3** RED — the whole of `crates/skein-acp/tests/acp_session.rs` against the
      not-yet-existing `skein_acp` API; compiler errors recorded below
- [ ] **T4** GREEN part 1 — `AcpPermissionTransport` and `CancellableModel`
- [ ] **T5** GREEN part 2 — `SkeinAgent`/`SkeinSession`, the `Agent.builder()` wiring, and
      `project_updates`
- [ ] **T6** gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace`
- [ ] **T7** control diff: `git diff dev` empty on `crates/skein-core/`, `crates/skein-mcp/`,
      `spikes/`, `.github/` and `rust-toolchain.toml`; the root `Cargo.toml` diff confined to
      `[workspace.dependencies]`
- [ ] **T8** drift check: dependency count added to `Cargo.lock`, MSRV confirmation, and the
      `serde_json/preserve_order` feature-unification investigation
- [ ] **T9** close out: tick the ACP bullet in `specs/007-tool-allowlist/tasks.md`, set this
      spec's Status, and populate the "Next slice" list

## Control baseline (T2)

`cargo test --workspace` on `dev` / `0b0102e`, working tree clean, 2026-09-03: **40 passing** —
`skein-core/tests/core.rs` 6, `tests/native_loop.rs` 18, `tests/tool_gateway.rs` 9,
`skein-mcp/tests/rmcp_gateway.rs` 7; 0 failed, 0 ignored. This is the number T6 diffs against.

## Pinned SDK surface (T1)

## Observed red (Constitution III)

## Next slice (not this feature)
