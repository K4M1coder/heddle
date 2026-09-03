# Tasks: a Windows-only sandboxed process launcher and one `proc_run` tool (v0 slice)

**Spec:** `specs/019-shell-connector-windows/spec.md` · **Plan:**
`specs/019-shell-connector-windows/plan.md` · TDD (red→green), branch `019-shell-connector-windows`
cut from `dev` at `b82f37a`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the launcher is a library crate with no CLI of its own and the tool is a method
  on the existing embedded server; `skein acp-agent` gains one flag and stays the authoritative
  client · II Local-first ✅ NON-NEGOTIABLE and **strengthened**: the AppContainer profile carries
  zero capability SIDs and the launch passes `CapabilityCount: 0`, so WFP has no permit filter to
  match on and the sandboxed child reaches no network at all — proven hermetically against a
  loopback listener in the test process, with an unsandboxed positive control
- III Test-First ✅ every step's red was observed and recorded verbatim under `## Observed red`
  before its green. T4 is deliberately the earliest behavioural step because it validates the whole
  DACL/traversal model at once, and its red had to be an unwritten-code red rather than an ACL one
  · IV Inverted coupling ✅ `skein-core` gains nothing and depends on nothing new. `skein-sandbox`
  depends on no Skein crate at all — it is a leaf — and `skein-connectors` reaches it the way it
  reaches `git2`: one module, one `#[cfg]`, no type of the dependency in any public signature
- V Traceability ✅ unchanged machinery, newly exercised: a `proc_run` call lands `ToolCall` →
  `Approval` → `ToolResult` on the chain like any other, and both governed end-to-end runs are read
  back **in a second process** through `skein ledger verify` at 12 and 11 steps. No new `StepKind`
- VI Security ✅ **the principle this slice is shaped by.** Deny-by-default is structural and not
  merely policy: two independent opt-ins (`--fs-root` *and* `--allow-run`), a server route disabled
  unless both are present, a CLI allowlist that must agree with it, and then the per-call human
  approval slice 018 proved live. The containment claim is stated exactly as narrow as it is
  provable — cannot *write* outside the root, not cannot *read* anything outside it
- VII Neutrality ✅ one crate, one tool, one flag. `proc_kill`, `proc_status`, streaming output, a
  cross-OS trait, a shell-binary blocklist, a `--run-dir` allowlist and two Job limits were each
  considered and rejected with a reason in `spec.md`
- VIII Loop discipline ✅ NON-NEGOTIABLE and untouched. Every refusal and every timeout is an
  `Err(String)` the model is told about and the run survives; a nonzero exit is an `Ok` because the
  process ran and the model needs the output
- Cross-platform ⚠️ **This slice is intentionally Windows-only.** ADR-0006 authorizes shipping
  `shell` on one OS first; the Constitution's "no OS-specific call without `#[cfg]` + an equivalent"
  is met on the `#[cfg]` and **not** on the equivalent, which is deferred to a Linux (Landlock) and
  a macOS (Seatbelt) slice each. On the macOS and Linux CI legs `skein-sandbox` compiles to a crate
  whose only reachable behaviour is a loud refusal, and `proc_run` is absent from every catalogue —
  verified by a `#[cfg(not(windows))]` test that runs on two of the three legs.

## Tasks
- [x] **T0** `specs/019-shell-connector-windows/{spec.md,plan.md,tasks.md}`; branch
      `019-shell-connector-windows` cut from `dev` at `b82f37a`
- [x] **T1** control baseline: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace`, each re-measured rather than quoted
- [ ] **T2** manifests, before any behaviour: `windows` and `win32job` in `[workspace.dependencies]`,
      `crates/skein-sandbox` with the D6 signatures and `todo!()` Windows bodies
- [ ] **T3** RED→GREEN — the AppContainer profile and the ACL grant (`tests/profile.rs`)
- [ ] **T4** RED→GREEN — the launcher walking skeleton (`tests/launch.rs`)
- [ ] **T5** RED→GREEN — the escape reproductions (`tests/escape.rs`)
- [ ] **T6** RED→GREEN — argv quoting (`tests/argv.rs`)
- [ ] **T7** RED→GREEN — the tool (`skein-connectors`, `tests/run_server.rs`)
- [ ] **T8** RED→GREEN — the absence gates (`tests/connector.rs`)
- [ ] **T9** RED→GREEN — `skein-cli` wiring
- [ ] **T10** RED→GREEN — the governed end-to-end pair (`cli_acp_agent.rs`)
- [ ] **T11** one `#[ignore]`d live-model test gated on `SKEIN_LIVE_MODEL`
- [ ] **T12** gates, dependency drift, control diff, close-out
- [ ] **T13** hand-verification against live Ollama — **not part of the implementation run**

## Control baseline (T1)

On `019-shell-connector-windows` @ `b82f37a`, working tree clean, Windows 11 Pro 10.0.26200,
toolchain 1.97, 2026-09-03, before any edit:

- `cargo fmt --all --check` — clean, no output, exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, `Finished dev profile`, exit 0.
- `cargo test --workspace` — **193 passed, 0 failed, 3 ignored**: `acp_session` 16,
  `cli_acp_agent` 10, `cli_chat` 12, `cli_ledger` 8, `cli_secret` 2, `connector` 6, `fs_root` 10,
  `fs_server` 7, `git_root` 5, `git_server` 13, `governed_fs_run` 4 (+1 ignored), `governed_git_run`
  4 (+1 ignored), `core` 19, `native_loop` 25, `tool_gateway` 14, `governed_run` 2, `openai_compat`
  15 (+1 ignored), `rmcp_gateway` 9, `silo_ledger` 7, `silo_secret` 5. Every `src/lib.rs` and
  `src/main.rs` unit target reports 0.

Slice 018's close records 191 at `4eeea42`; the delta of +2 is slice 018's own two tests, which is
the expected figure and is why the baseline is re-measured rather than quoted.

## Observed red

**T3** — `cargo test -p skein-sandbox --test profile`, both tests:

```
thread 'a_sandbox_derives_an_appcontainer_sid_and_grants_it_the_root' panicked at
crates\skein-sandbox\src\lib.rs:98:9:
not yet implemented: T3
test result: FAILED. 0 passed; 2 failed
```

An **unwritten-code** red — the `todo!("T3")` T2 deliberately left in `Sandbox::create` — and not an
ACL or a Win32 one. That distinction is the whole reason T2 exists as its own step. Getting there
took two compile errors in the *test's* own Win32 helper, both fixed before the red was recorded:
`HLOCAL` implements neither `From<*mut u16>` nor `From<*mut c_void>` in windows 0.61, so the frees
are written `HLOCAL(text.0 as *mut c_void)` rather than `.into()`.
