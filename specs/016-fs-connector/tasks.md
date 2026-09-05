# Tasks: the `fs` connector (v0 slice)

**Spec:** `specs/016-fs-connector/spec.md` · **Plan:** `specs/016-fs-connector/plan.md` · TDD
(red→green), product code in a new `crates/heddle-connectors` plus `crates/heddle-mcp` and
`crates/heddle-cli`, branch `016-fs-connector` cut from `dev` after slice 015 merged.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the capability lives in a library crate; `heddle-cli` gains one flag and the
  wiring that turns it into a transport plus two named policies. `heddle-core` is **untouched** by
  this slice: 015 already built everything it needed · II Local-first ✅ NON-NEGOTIABLE. The
  connector is **in-process** — an `rmcp` client and server over a `tokio::io::duplex`, no socket, no
  child process, no Node runtime. A third-party out-of-process server was rejected on exactly this
  ground (plan D1). No new egress path exists to guard
- III Test-First ✅ every step's red observed and recorded in `## Observed red` before its green ·
  IV Inverted coupling ✅ `heddle-core` still names no protocol. `heddle-mcp` remains the only crate
  naming MCP as a **client** and `heddle-connectors` becomes the only one naming it as a **server** —
  the invariant is amended in both crates rather than left stale (FR-002). `heddle-cli` reaches the
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
- [x] **T2** RED→GREEN — `crates/heddle-connectors` with `FsRoot`, and the amended MCP invariant
- [x] **T3** RED→GREEN — `FsServer`: `#[tool_router]` and the three `#[tool]` methods over derived
      schemas
- [x] **T4** RED→GREEN — `RmcpToolTransport::list`, then `LocalConnector` / `fs_connector`
- [x] **T5** RED→GREEN — the headline end-to-end test: advertisement → model tool request → real
      gateway → real connector → real server → real file contents → chain verifies
- [x] **T6** RED→GREEN — the refusal twins: unlisted `fs_write` never reaches the server; an
      out-of-root read is refused by the server and the run survives
- [x] **T7** RED→GREEN — a secret in a file's **contents** is scrubbed from every Ledger payload
- [x] **T8** RED→GREEN — `heddle-cli`: `ToolArgs`, `ConfiguredTools`, the two policies, `--fs-root`
- [x] **T9** RED→GREEN — CLI acceptance tests against the real binary
- [x] **T10** the `#[ignore]`d live-model test, gated on `HEDDLE_LIVE_MODEL`
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

All on 2026-09-03.

- **T2** `cargo test -p heddle-connectors --test fs_root` with ten containment tests written against a
  crate whose `lib.rs` held only its docstring — **1 compile error**:
  - `error[E0432]: unresolved import heddle_connectors::FsRoot` at
    `crates/heddle-connectors/tests/fs_root.rs:10:5` — `no FsRoot in the root`
  - Green: **10 passed**. One follow-on red between the two, worth recording because it changed the
    product: `error[E0277]: FsRoot doesn't implement Debug`, from the two `expect_err` calls on
    `FsRoot::new`. The derive was added rather than the assertions weakened.

- **T3** `cargo test -p heddle-connectors --test fs_server` with seven tool tests written against a
  crate that had no server — **1 compile error** naming all five missing items:
  - `error[E0432]: unresolved imports heddle_connectors::FsServer, ::ListParams, ::ReadParams,
    ::WriteParams, ::READ_BYTE_CAP` at `crates/heddle-connectors/tests/fs_server.rs:10:32`
  - Green: **7 passed**.

- **T4a** `cargo test -p heddle-mcp --test rmcp_gateway` with two new tests against the **existing**
  client and the **existing** live-server fixture — **2 failures, and nothing failed to compile.**
  This is the slice's most valuable red:
  - `assertion left == right failed: the catalogue is the server's own, not a hand-written list`
    — `left: []`, `right: ["fs_write", "read_secret"]`
  - `assertion left == right failed: the server offers fs_write too; only the operator's allowlist
    may reach the model` — `left: []`, `right: ["read_secret"]`
  - Exactly the failure the plan's D4 predicted: `RmcpToolTransport` inherited slice 015's defaulted
    `list` and advertised **nothing** against a real MCP server offering two tools, with no compile
    error and no runtime error to say so. Green: **9 passed** where 7 had passed, the seven unchanged.

- **T4b** `cargo test -p heddle-connectors --test connector` — **1 compile error**:
  - `error[E0432]: unresolved imports heddle_connectors::fs_connector, heddle_connectors::LocalConnector`
    at `crates/heddle-connectors/tests/connector.rs:8:24`
  - Green: **4 passed**, including the `Send` bound `HeddleAgent` requires.

- **T5** `cargo test -p heddle-connectors --test governed_fs_run` — **1 compile error**
  (`error[E0432]: unresolved import heddle_gateway`, before the dev-dependency existed), then **1
  assertion failure** once it compiled:
  - `the file's actual contents must reach the model: [tool_result tool=fs_read status=ok]
    {"content":[{"type":"text","text":"the first line of notes\nand a second one"}],"isError":false}`
  - **The assertion was wrong, not the product**, and the correction is recorded rather than
    absorbed: `RmcpToolTransport::call` deliberately hands back the whole `CallToolResult`, so the
    file's bytes arrive JSON-escaped inside it. The test now asserts the escaped form — the same move
    `rmcp_gateway.rs`'s `c4` already makes — plus `"isError":false`. **No product code changed for
    T5 at all**, which is the point of the step: the parts were built to compose, and this is the
    first test in a position to see whether they do.

- **T6** Both refusal twins were written together. `an_unlisted_write_never_reaches_the_server` was
  **green on arrival** — it exercises slice 005's governor, not anything new — and is labelled as
  guarding the composition rather than driving it.
  `an_out_of_root_read_is_refused_by_the_server_and_the_run_survives` failed first on an
  over-escaped `isError` needle in the test; the product was not touched.

- **T7** **Green on arrival**, and said so plainly rather than presented as driven: it verifies that
  `Redactor` composes with a secret arriving from disk, which the request asked to be *verified, not
  assumed*. To show the assertion has teeth, the secret was removed from the run's configuration and
  the test re-run: it failed with
  `no payload of the run may carry a configured secret: [… "content":"{\"content\":[{\"type\":\"text\",\"text\":\"api_key=sk-from-disk-SECRET-abc123\\nendpoint=…` —
  the value in cleartext in both the `ToolResult` payload and the next `LlmRequest`. Restored, green.

- **T8** `heddle chat … --fs-root <dir>` against the real binary — the flag did not exist:
  - `error: unexpected argument '--fs-root' found`, clap exit **2** where the test expects 0
  - Green after `wiring::ToolArgs`, `ConfiguredTools` and the two commands' wiring: **9 passed** in
    `cli_chat` where 8 had passed.

- **T9** Additive tests only; each was written before the assertion it makes was known to hold, and
  `chat_without_an_fs_root_sends_no_tools_key_at_all` is the control that pins slice 015's
  byte-identical promise now that a caller exists.

- **T10** The `#[ignore]`d live test was run twice with `-- --ignored --nocapture` and no
  `HEDDLE_LIVE_MODEL`: it prints `HEDDLE_LIVE_MODEL is unset; skipping the live model tool-call test`
  and passes, which is the behaviour that keeps `cargo test --workspace` green on a machine without
  Ollama.

## Gates (T11)

All four run on `016-fs-connector` @ `b7c9918`, 2026-09-03, Windows 11, Rust 1.97:

- `cargo fmt --all --check` — **clean**
- `cargo clippy --workspace --all-targets -- -D warnings` — **clean**, including
  `clippy::large_enum_variant`, which `Box<LocalConnector>` was chosen up front to avoid
- `cargo test --workspace` — **165 passed, 0 failed, 2 ignored**
- `cargo build --workspace` — **clean**

**132 → 165 is 33 new tests, where the plan predicted ~14.** Reconciled rather than explained away:
the plan named nine claims for this slice, and each turned out to want more than one test to state
honestly. `fs_root` is 10 tests because the absolute-argument refusal is asserted on the read path
*and* the write path, and because `FsRoot::new`'s two failure modes (missing, not-a-directory) are
different bugs. `fs_server` is 7 because each of three tools has a happy path and a refusal.
`governed_fs_run` is 4 plus the ignored live test. The three CLI tests the plan predicted became six,
because `--fs-root` needed a refusal test on **both** commands and the `--help` check is what makes
an opt-in capability discoverable. Per file: `connector` 4, `fs_root` 10, `fs_server` 7,
`governed_fs_run` 4 (+1 ignored), `rmcp_gateway` 7 → 9, `cli_chat` 8 → 11, `cli_acp_agent` 4 → 7.

## Control diff (T11)

`git diff dev --stat -- crates/heddle-silo/ spikes/ .github/ rust-toolchain.toml` is **empty**.
`spikes/` is untouched (ADR-0004 D2), and so are `.github/` and `rust-toolchain.toml` — the workspace
is `members = ["crates/*"]` and `core.yml`'s `paths:` already reads `crates/**`, so the new member is
picked up with no CI edit.

Over the whole branch, **2680 insertions and 39 deletions** across 21 files, three of them this
slice's spec artifacts. Every deletion in a **pre-existing test file** is accounted for, and **not
one is an assertion** — nine lines in total:

- two `StubProvider` docstring lines, in `cli_chat.rs` and `cli_acp_agent.rs`, which became false the
  moment the stub started reporting request bodies;
- six lines of the same two files' `read_request` plumbing, which now returns the body it already
  read instead of discarding it (`Option<()>` → `Option<String>`);
- one `use heddle_core::{…}` line in `rmcp_gateway.rs`, rewrapped after gaining `ToolTransport`.

The remaining deletions are in product code this slice exists to change: `heddle-mcp`'s docstring
invariant, and `heddle-cli`'s `chat.rs`/`acp.rs` `NoTools` + empty-`ToolPolicy` literals.

## Drift (T11)

**This slice adds packages to the shipped binary, and the honest number is twelve — but zero are new
to the workspace.** Recorded the way slices 012 and 014 recorded their own cases, rather than
claiming the zero slice 015 could truthfully claim.

- **One new workspace member**, `crates/heddle-connectors`, and **one new dependency edge**,
  `heddle-cli → heddle-connectors`. Those are the whole manifest diff: `git diff dev --
  Cargo.toml crates/*/Cargo.toml` touches exactly two files, adding 27 lines and removing none. The
  root `Cargo.toml` is **unchanged** — every dependency the new crate needed (`rmcp`, `tokio`,
  `schemars`, `serde`, `serde_json`, `tempfile`) was already in `[workspace.dependencies]`.
- **Twelve package names are newly reachable from the `heddle` binary**: `heddle-connectors`,
  `heddle-mcp`, `rmcp`, `rmcp-macros`, `tokio`, `tokio-macros`, `tokio-stream`, `tokio-util`,
  `async-trait`, `chrono`, `num-traits`, `pastey`. Measured, not estimated: `cargo tree -p heddle-cli
  -e normal` compared name-by-name against the same command in a throwaway worktree of `dev`.
- **All twelve were already resolved for the workspace on `dev`** — as `heddle-mcp`'s own product
  dependencies and the transitive closure of `rmcp` — so the workspace's package set is unchanged and
  no new build prerequisite appears. What changed is that they now **ship**. Nothing was removed.
- **`schemars` and `rmcp`'s `server` / `macros` / `transport-async-rw` features become product
  dependencies** where they were `heddle-mcp` dev-dependencies. That promotion is the whole reason
  plan D5 put the server in a new crate instead of in `heddle-mcp`, whose rmcp features are
  deliberately client-only.
- No toolchain change: `rust-toolchain.toml` and `workspace.package.rust-version` are untouched, and
  no package entered the graph that could have raised the MSRV.

`Cargo.lock` is **not tracked** in this repository (`.gitignore:13`), so there is no lockfile diff to
show either way. Recorded because slice 015's close-out cites an empty `Cargo.lock` diff as evidence,
and that diff is empty for every slice regardless of what it changed.

## Deviations from the plan

Six, all recorded rather than absorbed:

1. **`RmcpToolTransport::list` had to be written, and the plan did not say so.** The plan's T4 says
   `LocalConnector` implements `ToolTransport::{call, list}` *"by delegation"* — but slice 015
   deliberately left `RmcpToolTransport` inheriting the defaulted empty `list`, so delegation
   returned nothing. This is recorded as plan D4 and driven by T4a's red. It is the single change
   without which the whole slice would have compiled, run, and advertised nothing.
2. **`resolve_new`, not `resolve_parent`.** The plan's D8.3 describes the behaviour and the operator's
   request calls it *"a parent-canonicalization special case"*; the method is named for what a caller
   wants (a path that may not exist yet) rather than for how it works.
3. **No invocation counter in `FsServer`.** The plan and the request both name *"the server's own
   invocation counter"* as the ground truth for T6, on `rmcp_gateway.rs`'s precedent. That precedent
   is a **test fixture**; `FsServer` is product code, and a counter in it would be machinery with no
   caller outside a test. The proof used instead is **stronger**: `fs_write` would have created the
   file, so its absence on disk is an effect the server would have had, not bookkeeping about whether
   it ran.
4. **The parameter structs and `READ_BYTE_CAP` are public.** The plan does not say. They are the
   tool contract — the schema `schemars` derives from them is what the model is shown and what the
   server validates against — and a test cannot reach a private type from `tests/`.
5. **One `#[cfg]` appears in `fs_root.rs`, in the symlink *helper* only.** The plan says *"No
   `#[cfg]`"* for T2. Creating a symlink is a platform API (`std::os::windows::fs::symlink_dir` vs
   `std::os::unix::fs::symlink`), so a body with no `#[cfg]` cannot create one. The containment code
   itself has none, the assertions either side of the helper are the same on every OS, and the test
   skips cleanly when the machine refuses to create a link — Windows needs a privilege for it that
   Unix does not.
6. **The new crate's manifest grew across three commits rather than landing whole at T2.** The plan's
   T2 declares the full dependency set up front. Each dependency was instead added in the commit that
   first used it, so no commit declares an edge with no caller.

Also worth stating, since Constitution III is the point: **T5, T7 and one of T6's two tests needed no
product change**, and they are labelled as such in `## Observed red` rather than presented as driven.
They are composition and regression guards over machinery slices 005–015 already built. The steps
that genuinely drove product code are T2, T3, T4a, T4b and T8.

## Out of scope

Deliberately not done, so no one helpfully does it. Identical to the spec's list, and in particular:

- **`git` and `shell` connectors.** ADR-0004 D3's sixth item **closes for `fs` and remains open for
  `git` and `shell`** — said here rather than marking the item done. Shell only after an access-scope
  boundary exists, for the reasons the spec gives at length.
- **Design §5.4/§5.5's connector configuration hierarchy**, the scope-owner resolver and
  `AccessScope`. One operator-named root; nothing hierarchical.
- **Closing the TOCTOU residual** between `canonicalize` and open. It needs `cap-std`-style
  directory-handle-relative opens and is recorded in `FsRoot`'s own docstring.
- **Automatic secret detection in file contents.** T7 proves the configured case; the unconfigured
  case is a stated gap, asserted as such in the test itself.
- **Filesystem breadth** — no glob, recursion, rename, delete, mkdir, diff, MIME sniffing, ZIP or
  watch. **Deriving `ToolAccess` from MCP annotations.** **`role: "tool"` replay**, `strict`,
  `tool_choice`, parallel tool calls, streaming.
- **`crates/heddle-silo/`, `spikes/`, `.github/`, `rust-toolchain.toml`** — all verified empty in the
  control diff above.

## Next slice (not this feature)
- [ ] **the ACP permission gate, exercised.** `AcpPermissionTransport` is finally reachable —
      `fs_write` is allowlisted and approved for `heddle acp-agent`, so the client is asked. What this
      slice does **not** have is a test where a real ACP client *answers* a permission request:
      `cli_acp_agent.rs`'s client registers no permission handler, and building one is a slice of its
      own. Until then the gate is wired and unproven end to end, which is exactly the shape slice 015
      left advertisement in.
- [ ] **an access-scope boundary**, and then a `shell` connector behind it. Not before.
- [ ] **a `git` connector**, if a model that can read files turns out not to be enough.
- [ ] **the TOCTOU residual**, with `cap-std` or an equivalent directory-handle API.
- [ ] **raw-wire-byte capture** — carried unchanged from slices 011–015.
- [ ] **`role: "tool"` / `tool_call_id` conversation replay**, which would reopen
      `native_loop.rs`'s anti-injection decision deliberately rather than by accident.
- [ ] **reconcile slices 008 and 014 on `serde_json/preserve_order`.** Carried from 015: 008 is right
      and 014 is wrong, and the tree should not carry both.
- [ ] **streaming (SSE)**, **provider authentication**, a config file, the egress-policy layer.

## Live verification (T12)

Not performed in the implementation run. `a_live_model_calls_a_real_fs_tool` in
`crates/heddle-connectors/tests/governed_fs_run.rs` is the repeatable form of it:

```text
$env:HEDDLE_LIVE_MODEL = "<a tool-capable model>"
cargo test -p heddle-connectors --test governed_fs_run -- --ignored --nocapture
```

The by-hand transcript against a live Ollama — `heddle chat --fs-root …`, then `heddle ledger log`,
`show` and `verify`, then a second run asking for a write and confirming the `Approval` step records
`denied` / `tool is not in the allowlist` with no file created — belongs under this heading and is
still to be recorded.
