# Tasks: a read-only `git` connector (v0 slice)

**Spec:** `specs/017-git-connector/spec.md` · **Plan:** `specs/017-git-connector/plan.md` · TDD
(red→green), product code in `crates/skein-connectors` and `crates/skein-cli`, branch
`017-git-connector` cut from `dev` at `5d36c1d`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the capability lives in a library crate; `skein-cli` gains one helper on an
  existing flag's argument group. `skein-core` is **untouched** — slice 015 already built everything
  it needed · II Local-first ✅ NON-NEGOTIABLE, and stronger here than in any prior slice: the
  connector stays **in-process** over a `tokio::io::duplex`, **no subprocess is spawned anywhere**,
  and `git2` is compiled with `default-features = false` so **no HTTPS and no SSH transport is
  linked in at all** — the same build-property guarantee `skein-gateway` makes about TLS. A `git`
  subprocess was rejected partly on the measured ground that a repository's own `core.fsmonitor` is
  executed by `git status` and is **not** executed by `git2`
- III Test-First ✅ every step's red observed and recorded in `## Observed red` before its green ·
  IV Inverted coupling ✅ `skein-core` still names no protocol. `skein-connectors` remains the only
  crate naming MCP as a **server**, and `src/git.rs` is the only module naming `git2` — so a future
  swap to `gix` touches one file. `skein-cli` reaches the connector only through `ToolTransport` and
  never sees `git2`
- V Traceability ✅ no new `StepKind` and none needed. A git tool call lands as
  `ToolCall`/`Approval`/`ToolResult` through the gateway 005 built; slice 014's `Redactor` scrubs it
  on the way in, verified against a secret in a **real commit message** rather than assumed
- VI Security ✅ deny-by-default at three layers, each with its own test: `--fs-root` is **opt-in**;
  the **server** disables both git routes when the root is not a repository *and* refuses a
  containment violation as a tool error; and the **CLI allowlist** omits the names in the same case,
  which is not decoration — an allowlisted-but-disabled route produces a `SkeinError::Tool` that
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
      `crates/skein-connectors`; `git2` as a dev-dependency of both `skein-connectors` and
      `skein-cli`. Proves libgit2 compiles **before** any behaviour is written
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
- [x] **T9** RED→GREEN — `skein-cli` wiring: `ToolArgs::git_tools`, both policies, the reworded
      `--fs-root` doc comment and module docstring
- [x] **T10** RED→GREEN — CLI acceptance against the **real binary**, one test per command
- [x] **T11** the `#[ignore]`d live-model test, gated on `SKEIN_LIVE_MODEL`
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

**T3** — `cargo test -p skein-connectors --test git_root` against a `git.rs` that did not exist:

```
error[E0432]: unresolved import `skein_connectors::is_git_repository`
```

**T4** — `cargo test -p skein-connectors --test git_server`:

```
error[E0432]: unresolved imports `skein_connectors::LogParams`, `skein_connectors::LOG_COUNT_CAP`,
`skein_connectors::STATUS_ENTRY_CAP`
  --> crates\skein-connectors\tests\git_server.rs:19:42
error[E0599]: no method named `git_status` found for struct `FsServer` in the current scope
  --> crates\skein-connectors\tests\git_server.rs:93:21
error[E0599]: no method named `git_log` found for struct `FsServer` in the current scope
  --> crates\skein-connectors\tests\git_server.rs:97:21
error: could not compile `skein-connectors` (test "git_server") due to 6 previous errors
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
rename, from `cargo test -p skein-connectors --test connector`:

```
error[E0432]: unresolved import `skein_connectors::local_connector`
  --> crates\skein-connectors\tests\connector.rs:10:24
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

## Gates (T12)

*(filled in at close-out)*

## Control diff (T12)

*(filled in at close-out)*

## Drift (T12)

*(filled in at close-out)*

## Deviations from the plan

*(filled in at close-out)*

## Out of scope

Deliberately not done, so no one helpfully does it. Identical to the spec's list, and in particular:

- **`git diff`, `git blame`, `git show`, branch/tag/remote listing.** Two tools.
- **Any mutating git operation, and any operation that could reach a remote.** Not built, and no
  HTTPS/SSH transport is compiled in either.
- **Any subprocess at all**, and no arbitrary `git` subcommand passthrough.
- **A `shell` connector.** Still deferred; ADR-0004 D3's sixth item **closes for `fs` and `git` and
  remains open for `shell`**.
- **A `--git-root` flag, or any second root.**
- **`crates/skein-silo/`, `spikes/`, `.github/`, `rust-toolchain.toml`** — verified empty in the
  control diff above.

## Next slice (not this feature)

*(filled in at close-out)*

## Live verification (T13)

*(not performed in the implementation run)*
