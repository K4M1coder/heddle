# Tasks: a read-only `git` connector (v0 slice)

**Spec:** `specs/017-git-connector/spec.md` · **Plan:** `specs/017-git-connector/plan.md` · TDD
(red→green), product code in `crates/heddle-connectors` and `crates/heddle-cli`, branch
`017-git-connector` cut from `dev` at `5d36c1d`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the capability lives in a library crate; `heddle-cli` gains one helper on an
  existing flag's argument group. `heddle-core` is **untouched** — slice 015 already built everything
  it needed · II Local-first ✅ NON-NEGOTIABLE, and stronger here than in any prior slice: the
  connector stays **in-process** over a `tokio::io::duplex`, **no subprocess is spawned anywhere**,
  and `git2` is compiled with `default-features = false` so **no HTTPS and no SSH transport is
  linked in at all** — the same build-property guarantee `heddle-gateway` makes about TLS. A `git`
  subprocess was rejected partly on the measured ground that a repository's own `core.fsmonitor` is
  executed by `git status` and is **not** executed by `git2`
- III Test-First ✅ every step's red observed and recorded in `## Observed red` before its green ·
  IV Inverted coupling ✅ `heddle-core` still names no protocol. `heddle-connectors` remains the only
  crate naming MCP as a **server**, and `src/git.rs` is the only module naming `git2` — so a future
  swap to `gix` touches one file. `heddle-cli` reaches the connector only through `ToolTransport` and
  never sees `git2`
- V Traceability ✅ no new `StepKind` and none needed. A git tool call lands as
  `ToolCall`/`Approval`/`ToolResult` through the gateway 005 built; slice 014's `Redactor` scrubs it
  on the way in, verified against a secret in a **real commit message** rather than assumed
- VI Security ✅ deny-by-default at three layers, each with its own test: `--fs-root` is **opt-in**;
  the **server** disables both git routes when the root is not a repository *and* refuses a
  containment violation as a tool error; and the **CLI allowlist** omits the names in the same case,
  which is not decoration — an allowlisted-but-disabled route produces a `HeddleError::Tool` that
  `NativeLoop::mediate` treats as fatal. Read-only **in scope**, not by policy choice: there is no
  mutating code path to misclassify
- VII Neutrality ✅ two tools, no new flag, no new crate, no second connector, no subprocess. Four
  new shipped packages, measured, against `gix`'s 112 for the same capability
- VIII Loop discipline ✅ NON-NEGOTIABLE and untouched. The exits, the probe, the controller and the
  survivable-refusal rule are unchanged
- Cross-platform ⚠️ **no `#[cfg]` anywhere in this slice's product code.** The containment check
  compares two canonicalized paths, which is exactly why both sides are canonicalized — on Windows
  both are `\\?\`-verbatim. This is also the first slice to add a **C dependency**:
  `vendored-libgit2` pins one libgit2 (1.9.7) on all three OSes rather than linking whatever a runner
  has, and a C toolchain was already required by `rusqlite`'s bundled SQLite. T2 is deliberately an
  early, separate step so a build failure lands before any behaviour is written. The Windows leg is
  observed locally; macOS and Linux remain unobserved until this repository has a remote

## Tasks
- [x] **T0** `specs/017-git-connector/{spec.md,plan.md,tasks.md}`; branch `017-git-connector` cut
      from `dev` at `5d36c1d`
- [x] **T1** control baseline: `cargo test --workspace` before any edit — **165 passed, 2 ignored**
- [x] **T2** manifests: `git2` + `chrono` in `[workspace.dependencies]` and in
      `crates/heddle-connectors`; `git2` as a dev-dependency of both `heddle-connectors` and
      `heddle-cli`. Proves libgit2 compiles **before** any behaviour is written
- [x] **T3** RED→GREEN — containment: `src/git.rs` with `open_contained` and `is_git_repository`,
      driven by `tests/git_root.rs`
- [x] **T4** RED→GREEN — the two tools: `git_status`, `git_log`, `LogParams`, `LOG_COUNT_CAP`,
      `STATUS_ENTRY_CAP`, and the porcelain/log formatting, driven by `tests/git_server.rs`
- [x] **T5** RED→GREEN — the rename (`FsServer` → `EmbeddedServer`, `fs_connector` →
      `local_connector`) and the two `disable_route` calls, driven by two tests appended to
      `tests/connector.rs`
- [x] **T6** RED→GREEN — the headline governed run: `tests/governed_git_run.rs`
- [x] **T7** RED→GREEN — redaction over a **real commit message**
- [x] **T8** RED→GREEN — the injection boundary: the only model-supplied value is a `u32`
- [x] **T9** RED→GREEN — `heddle-cli` wiring: `ToolArgs::git_tools`, both policies, the reworded
      `--fs-root` doc comment and module docstring
- [x] **T10** RED→GREEN — CLI acceptance against the **real binary**, one test per command
- [x] **T11** the `#[ignore]`d live-model test, gated on `HEDDLE_LIVE_MODEL`
- [x] **T12** gates, control diff, dependency drift, close-out
- [ ] **T13** hand-verification against live Ollama — **not part of the implementation run**;
      performed separately and recorded below under `## Live verification`

## Control baseline (T1)

`cargo test --workspace` on `017-git-connector` @ `5d36c1d` (identical to `dev`), working tree clean,
2026-09-03, before any code edit: **165 passed, 0 failed, 2 ignored** — `acp_session` 16,
`cli_acp_agent` 7, `cli_chat` 11, `cli_ledger` 8, `cli_secret` 2, `connector` 4, `fs_root` 10,
`fs_server` 7, `governed_fs_run` 4 (+1 ignored), `core` 19, `native_loop` 25, `tool_gateway` 14,
`governed_run` 2, `openai_compat` 15 (+1 ignored), `rmcp_gateway` 9, `silo_ledger` 7, `silo_secret`
5. The six `src/lib.rs`/`src/main.rs` unit-test targets and the six doc-test targets each contribute
`0 passed`. This is slice 016's recorded close figure exactly — `dev` is two documentation-only
commits later — and it is the number T12 diffs against.

## Observed red (Constitution III)

All on 2026-09-03. Recorded verbatim.

**T3** — `cargo test -p heddle-connectors --test git_root` against a `git.rs` that did not exist:

```
error[E0432]: unresolved import `heddle_connectors::is_git_repository`
```

**T4** — `cargo test -p heddle-connectors --test git_server`:

```
error[E0432]: unresolved imports `heddle_connectors::LogParams`, `heddle_connectors::LOG_COUNT_CAP`,
`heddle_connectors::STATUS_ENTRY_CAP`
  --> crates\heddle-connectors\tests\git_server.rs:19:42
error[E0599]: no method named `git_status` found for struct `FsServer` in the current scope
  --> crates\heddle-connectors\tests\git_server.rs:93:21
error[E0599]: no method named `git_log` found for struct `FsServer` in the current scope
  --> crates\heddle-connectors\tests\git_server.rs:97:21
error: could not compile `heddle-connectors` (test "git_server") due to 6 previous errors
```

Then, with the tools written but `Sort::TIME` alone as the plan literally named it — a **second**
red, and a real defect rather than a test accommodation:

```
assertion `left == right` failed: newest first:
  left: ["the third commit", "add the tracked file", "the second commit"]
 right: ["the third commit", "the second commit", "add the tracked file"]
```

`TIME` is a date-ordered priority queue whose tie-break among commits sharing a second is
arbitrary, and three fixture commits are written in the same second — as are rebased or scripted
commits in real life. `Sort::TIME | Sort::TOPOLOGICAL` adds the constraint that a parent never
precedes its child. Recorded under `## Deviations from the plan`.

**T5** — the gate's red arrived twice, and the second half is the one that matters. First the
rename, from `cargo test -p heddle-connectors --test connector`:

```
error[E0432]: unresolved import `heddle_connectors::local_connector`
  --> crates\heddle-connectors\tests\connector.rs:10:24
```

And before it, from T4's own green: the **pre-existing** three-tool catalogue test failed, because
the new tools were advertised unconditionally:

```
---- the_connector_lists_the_three_tools_with_their_derived_schemas stdout ----
assertion `left == right` failed
  left: ["fs_list", "fs_read", "fs_write", "git_log", "git_status"]
 right: ["fs_list", "fs_read", "fs_write"]
```

That is exactly the failure SC-012 makes a stop condition, and the two `disable_route` calls are
what answer it. Its assertions were **not** changed; only its import line moved.

**T6, T7, T8** — `tests/governed_git_run.rs` was **green on arrival**, because T4 and T5 had
already built everything it composes. Recorded as such rather than dressed up: these are
composition guards, and a guard that has never failed is a guard nobody has checked. Each one's
teeth were therefore demonstrated by breaking exactly the thing it claims to protect.

T7, with `SECRET_IN_A_COMMIT` removed from the run's `Redactor` configuration and nothing else
changed:

```
---- a_secret_in_a_commit_message_is_scrubbed_from_every_payload_of_the_run stdout ----
no payload of the run may carry a configured secret: [… "{\"tool\":\"git_log\",\"content\":
\"{\\\"content\\\":[{\\\"type\\\":\\\"text\\\",\\\"text\\\":\\\"595e302\\\\t2026-09-03T20:07:11Z
\\\\tFixture Author\\\\toops, committed sk-from-a-commit-message-SECRET-abc123 in the subject …
```

So a secret in a commit message really does reach the Ledger through `git_log`, and only the
`Redactor` keeps it out. The unconfigured case remains the stated gap slice 016 recorded.

T8, with the crafted `count` replaced by a valid `5`:

```
---- a_crafted_count_is_refused_as_a_tool_error_and_the_run_survives stdout ----
the refusal must arrive flagged as a tool error: [tool_result tool=git_log status=ok]
{"content":[{"type":"text","text":"07fb893\t2026-09-03T20:07:22Z\tFixture Author\tthe only
commit"}],"isError":false}
```

So the `isError: true` the test asserts is genuinely produced by the crafted value being refused at
the typed boundary, and not by anything the harness would have said anyway. Both were restored
verbatim afterwards and both are green.

**T9, T10** — one red for both, because `heddle-cli` has no `lib` target and its policies are only
observable through the shipped binary. From `cargo test -p heddle-cli --test cli_chat`:

```
---- chat_with_an_fs_root_that_is_a_git_repository_advertises_the_git_tools_and_reports_real_status
stdout ----
assertion `left == right` failed
  left: ["fs_read", "fs_list"]
 right: ["fs_read", "fs_list", "git_status", "git_log"]
```

The server was already offering both git tools over that root; the allowlist was what withheld
them. `ToolArgs::git_tools` is what answers it, appended by both `chat_policy` and `agent_policy`.

## Gates (T12)

All four on 2026-09-03, Windows 11, `rustc 1.97.1` — the channel `rust-toolchain.toml` pins.

- `cargo fmt --all --check` — clean, no output.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, no warnings.
- `cargo build --workspace` — succeeds. libgit2 1.9.7 is compiled from the vendored source by
  `cc`; T2 proved this before any behaviour was written, and it has held on every run since.
- `cargo test --workspace` — **191 passed, 0 failed, 3 ignored**, against the T1 baseline of
  165/0/2. Per target: `acp_session` 16, `cli_acp_agent` 8, `cli_chat` 12, `cli_ledger` 8,
  `cli_secret` 2, `connector` 6, `fs_root` 10, `fs_server` 7, `git_root` 5, `git_server` 13,
  `governed_fs_run` 4 (+1 ignored), `governed_git_run` 4 (+1 ignored), `core` 19, `native_loop` 25,
  `tool_gateway` 14, `governed_run` 2, `openai_compat` 15 (+1 ignored), `rmcp_gateway` 9,
  `silo_ledger` 7, `silo_secret` 5.

The +26 is accounted for exactly and adds nothing anywhere else: `git_root` 5, `git_server` 13,
`governed_git_run` 4, `connector` +2 (4 → 6), `cli_chat` +1 (11 → 12), `cli_acp_agent` +1 (7 → 8).
The +1 ignored is `a_live_model_calls_a_real_git_tool`. **Every other target's count is unchanged**,
which is SC-012 stated as a number.

The tri-OS caveat of slices 004–016 stands unamended: the Windows leg is observed locally, and the
macOS and Linux legs remain unobserved until this repository has a remote. This is the first slice
whose unobserved legs must compile C that was not already being compiled — see the correction under
`## Drift`.

## Control diff (T12)

`git diff dev --stat -- crates/heddle-silo/ spikes/ .github/ rust-toolchain.toml` — **empty**
(SC-013). The same command over `crates/heddle-core/ crates/heddle-gateway/ crates/heddle-acp/
crates/heddle-mcp/` is also empty, which is the stronger claim the plan's blast-radius table made:
`heddle-core` needed nothing, because slice 015 already built everything a second tool family
requires.

Everything the slice touched, `git diff dev --stat`:

```
 Cargo.toml                                        |   9 +
 crates/heddle-cli/Cargo.toml                       |   5 +
 crates/heddle-cli/src/main.rs                      |   8 +-
 crates/heddle-cli/src/wiring.rs                    |  59 +-
 crates/heddle-cli/tests/cli_acp_agent.rs           |  88 +++
 crates/heddle-cli/tests/cli_chat.rs                | 136 +++++
 crates/heddle-connectors/Cargo.toml                |  11 +
 crates/heddle-connectors/src/connector.rs          |   8 +-
 crates/heddle-connectors/src/fs.rs                 |   2 +-
 crates/heddle-connectors/src/git.rs                | 355 ++++++++++++
 crates/heddle-connectors/src/lib.rs                |   9 +-
 crates/heddle-connectors/src/server.rs             | 110 +++-
 crates/heddle-connectors/tests/connector.rs        | 120 +++-
 crates/heddle-connectors/tests/fs_server.rs        |  14 +-
 crates/heddle-connectors/tests/git_root.rs         | 216 ++++++++
 crates/heddle-connectors/tests/git_server.rs       | 455 ++++++++++++++++
 crates/heddle-connectors/tests/governed_fs_run.rs  |   6 +-
 crates/heddle-connectors/tests/governed_git_run.rs | 602 ++++++++++++++++++++
 specs/017-git-connector/*.md                      | (this slice's own documents)
 21 files changed, 3261 insertions(+), 42 deletions(-)
```

The four small edits to pre-existing test files — `connector.rs` beyond its two new tests,
`fs_server.rs`, `governed_fs_run.rs`, and the two CLI files beyond their new tests — are **import
lines and docstrings only**. No assertion in the workspace was changed or removed (SC-012).

## Drift (T12)

Re-measured this session rather than quoted from the plan, with `cargo tree -p heddle-cli -e normal`
on both `dev` and this branch.

**140 → 144 normal packages. Exactly four are new:**

```
git2 v0.21.0
libgit2-sys v0.18.8+1.9.7
libz-sys v1.1.29
libc v0.2.186
```

`chrono 0.4.45` is a **new edge to a package that was already in the graph** (it arrives via `rmcp`
and `schemars`), so it costs nothing; `bitflags`, `log` and `num-traits` were likewise already
there. The comparison also surfaced six packages differing only by patch version
(`serde`/`serde_core`/`serde_derive`/`serde_json`/`proc-macro2`/`quote`) — that is resolution noise,
not drift: `Cargo.lock` is gitignored here (`.gitignore:13`), so the `dev` worktree resolved fresh
while this branch used the local lock. It is recorded because it appeared in the measurement, not
because the slice caused it.

**No network transport is linked in.** `grep -iE "openssl|libssh2|ssh2|native-tls|rustls"` over the
whole shipped normal graph returns nothing. git2 0.21's `default` is empty and `https`/`ssh` are
left off, so this is a property of the build rather than of the code's restraint — the same
guarantee `heddle-gateway` makes about TLS (Constitution II, NON-NEGOTIABLE). It matters more here
than anywhere: this repository has no remote, so its own tests **could not** have caught a remote
call by accident.

`cargo tree -e build` shows no additions in the `cc`/`jobserver`/`pkg-config`/`shlex` class,
because they were already build dependencies of `libsqlite3-sys`.

**Correcting slice 016.** `specs/016-fs-connector/spec.md:189` records `git2` as *"libgit2 C
bindings, a tri-OS build burden"*. That judgment is **materially wrong today**, and it is now
load-bearing in the wrong direction, so it is corrected here rather than inherited: `rusqlite` is
pinned `features = ["bundled"]` in the root manifest and `libsqlite3-sys 0.38.2` is in
`heddle-cli`'s shipped graph, which means the bundled SQLite amalgamation is already compiled with
`cc` on all three OSes. A C toolchain was a hard build prerequisite of this workspace before this
slice existed. `vendored-libgit2` adds a second C library to that same existing burden, and pins
one libgit2 (1.9.7) on every OS instead of linking whatever a runner happens to have — mirroring
`rusqlite`'s own `bundled` choice. Slice 016's *conclusion* (do not add a git dependency yet) was
still right for that slice on Principle VII grounds; only this one stated reason was wrong.

Slice 016's spec is left as the historical record of what was believed then. This section is where
the correction lives.

## Deviations from the plan

Four, all recorded rather than quietly absorbed.

1. **`Sort::TIME` became `Sort::TIME | Sort::TOPOLOGICAL`.** The plan's step T4 named
   `set_sorting(Sort::TIME)` literally. Measured: `TIME` alone is a date-ordered priority queue
   whose tie-break among commits sharing a second is arbitrary, and three fixture commits written
   in the same second came back parent-before-child — see the second red under `## Observed red`.
   `TOPOLOGICAL` adds the constraint that a parent never precedes its child, which is what "newest
   first" means to whoever reads the output. Real-world equivalents of the fixture are a rebase or
   a scripted series of commits, so this was a shippable defect and not a test artifact.

2. **The crafted-`count` refusal names the expected *type*, not the field.** The plan's test
   sketch asserted the refusal names `count`. Measured, serde says
   `invalid type: string "…", expected u32` and does not mention the field. The assertion pins
   `expected u32` instead, because naming the type is what lets a model correct itself. The claim
   SC-009 makes is unchanged.

3. **Non-UTF-8 paths, branch names and commit summaries render lossily rather than being dropped.**
   The plan's fact 13 listed `StatusEntry::path -> Result<&str, Error>`; git2 0.21 actually has
   `path() -> Option<&str>` alongside `path_bytes() -> &[u8]`, and `Reference::shorthand() ->
   Option<&str>` alongside `shorthand_bytes()`. Using the `Option`-returning forms would have
   meant either a `filter_map` that silently omits a changed file from a status report — a wrong
   answer in a right answer's shape — or an invented fallback string. The `_bytes` forms with
   `String::from_utf8_lossy` say a file changed and say roughly which, with no fallback branch.

4. **`git_status_names_a_detached_head` is a test the plan did not list.** The plan's output
   specification for `git_status` includes a `## (detached HEAD at <7 hex>)` header, and nothing in
   its test list exercised it. It is one test, over behaviour the plan already required.

Two of the plan's steps landed **green on arrival** rather than red-then-green: T6/T7/T8's
`governed_git_run.rs`, because T4 and T5 had already built everything it composes. They are
labelled composition guards and each one's teeth were demonstrated by breaking exactly what it
protects, with both failures recorded verbatim above. T1's baseline and T2's manifests were
completed in earlier sessions of this branch; T3's red was recorded in its commit message and is
backfilled above.

## Out of scope

Deliberately not done, so no one helpfully does it. Identical to the spec's list, and in particular:

- **`git diff`, `git blame`, `git show`, branch/tag/remote listing.** Two tools.
- **Any mutating git operation, and any operation that could reach a remote.** Not built, and no
  HTTPS/SSH transport is compiled in either.
- **Any subprocess at all**, and no arbitrary `git` subcommand passthrough.
- **A `shell` connector.** Still deferred; ADR-0004 D3's sixth item **closes for `fs` and `git` and
  remains open for `shell`**.
- **A `--git-root` flag, or any second root.**
- **`crates/heddle-silo/`, `spikes/`, `.github/`, `rust-toolchain.toml`** — verified empty in the
  control diff above.

## Next slice (not this feature)

- **A `shell` connector**, still deferred, and slice 016's reasoning stands unamended: shell's
  blast radius is bounded by nothing this tree has, and an allowlist of command *names* is not an
  allowlist of *effects*. This slice's rejection of a `git` subprocess is a narrower judgment about
  one fixed argv and does **not** reopen that deferral — if anything the measured `core.fsmonitor`
  finding strengthens it. ADR-0004 D3's connector item now closes for `fs` and `git` and remains
  open for `shell` alone.
- **`git diff`**, if it is ever wanted, needs its own output-cap design before it needs code: it is
  the first unbounded-output git tool, and neither `git_status`'s labelled truncation nor
  `git_log`'s refusal transfers to it unexamined.
- Carried unchanged from slice 016: the ACP permission gate exercised end to end, the
  `canonicalize`-to-open TOCTOU residual, `role: "tool"` / `tool_call_id` replay, raw wire-byte
  capture, streaming (SSE), provider authentication, a config file, `--json` output, and the
  slices-008-vs-014 `serde_json/preserve_order` reconciliation.
- **Residuals this slice adds**, recorded rather than hidden: the connector trusts the system and
  global git configuration the way `git` itself does, so a global `core.excludesFile` affects what
  `git_status` reports; `.git/` stays readable through `fs_read` exactly as on `dev`, neither
  widened nor narrowed; libgit2's owner-validation refuses a repository owned by another user and
  that surfaces as an ordinary tool-level refusal, with
  `git2::opts::set_verify_owner_validation` deliberately never called; and the entry cap bounds
  `git_status`'s **output**, not its walk — `recurse_untracked_dirs(false)` and
  `include_ignored(false)` bound the walk the way `git status --porcelain` bounds its own, and a
  timeout belongs to a slice that has timeout machinery.

## Live verification (T13)

*(not performed in the implementation run)*
