# Feature Specification: a read-only `git` connector — what changed, and what happened lately (v0 slice)

**Feature Branch:** `017-git-connector` · **Created:** 2026-09-03 · **Status:** Implemented (v0
slice) **Input:** `specs/016-fs-connector/tasks.md` "Next slice" — *"a `git` connector, if a model
that can read files turns out not to be enough"* · ADR-0004 D3's sixth v0 item (*"MCP tools
(fs/git/shell)"*), which closed for `fs` in slice 016 and closes for `git` here · Constitution II
(**local-first**, NON-NEGOTIABLE), III (**test-first**), IV (**inverted coupling**), V
(**traceability**), VI (**deny-by-default**), VII (**no capability without a real need**) · design
§4.3.

Slice 016 gave a model the ability to read a file inside one operator-named directory. A coding
agent that can read files but cannot see **what changed**, **what is staged**, or **what the recent
history looks like** is missing the single most common piece of context a coding assistant needs —
and it cannot reconstruct any of it from file contents alone.

This slice adds two read-only MCP tools, `git_status` and `git_log`, to the **same** embedded server
over the **same** `--fs-root`. There is no new flag, no new crate, no second connector, and no
subprocess. `shell` stays deferred and slice 016's reasoning for that deferral stands untouched.

## What this slice changes for a user

**Without `--fs-root`: nothing, observably.** Both commands keep today's exact behaviour.

**With `--fs-root <DIR>` where `DIR` is not a git repository: nothing, observably.** The git tools
are neither advertised nor allowlisted, and the run puts the same three `fs` names on the wire it
put there on `dev`.

**With `--fs-root <DIR>` where `DIR` *is* the root of a git repository:** the run gains two more
tools, named to the model with the schema the server derived from its real parameter types:

| tool | args | `ToolAccess` | `heddle chat` | `heddle acp-agent` |
|---|---|---|---|---|
| `fs_read` | `path` | `ReadOnly` | allowlisted | allowlisted |
| `fs_list` | `path` | `ReadOnly` | allowlisted | allowlisted |
| `fs_write` | `path`, `content` | `Mutating` | **not allowlisted** | allowlisted **and** `approved` |
| `git_status` | *none* | `ReadOnly` | allowlisted | allowlisted |
| `git_log` | `count` | `ReadOnly` | allowlisted | allowlisted |

Both commands allowlist both git tools **identically**. There is no `fs_write`-style asymmetry here,
because neither tool mutates anything and so there is nothing to confirm.

## Six things a reader must know up front

1. **The library is `git2`, and no subprocess is spawned anywhere.** `git2 = { version = "0.21",
   default-features = false, features = ["vendored-libgit2"] }`. `default = []` means **no HTTPS and
   no SSH transport is compiled in at all** — the same *build-property* guarantee `heddle-gateway`
   makes by compiling in no TLS backend, now applied to git. Measured: four new normal packages in
   `heddle-cli`'s shipped graph (`git2`, `libgit2-sys`, `libz-sys`, `libc`), against `gix`'s 112 for
   `status` + `revision`. A C toolchain is **already** a hard prerequisite of this workspace —
   `rusqlite`'s `bundled` SQLite compiles C on all three OSes — so slice 016's recorded *"libgit2 C
   bindings, a tri-OS build burden"* judgment is **materially wrong today** and this spec corrects it
   rather than inheriting it.
2. **A `git` subprocess was rejected on three measured grounds, not on the one slice 016 gave.** A
   fixed `Command::new("git").args([…])` with no shell genuinely *is* a different hazard class from an
   arbitrary shell tool, and slice 016's plan conflated the two. It is rejected because: (a) `git -C
   <root> …` **discovers upward**, measured — a root inside a repository silently reports on the
   *enclosing* repository, whose failure mode is silent success on the wrong repository; (b) a
   repository's own `.git/config` can make a "read-only" `git status` **execute an arbitrary
   program** — `core.fsmonitor`, measured to run under `git` and measured **not** to run under
   `git2`, which reads the key and does not execute it; (c) it makes a shipped capability depend on
   an unpinned external binary's output format, exit codes and locale on three OSes.
3. **`Repository::open` is the containment primitive, and `discover` is the hole.** Measured:
   `open("<repo>/sub")` fails with `NotFound`; `discover("<repo>/sub")` succeeds and reports the
   *parent* repository's worktree. So `open_contained` uses `Repository::open` **only** — never
   `discover`, never `open_ext`, never `open_from_env` — and the containment test asserts
   `discover`'s contrasting behaviour explicitly, so a helpful future refactor to `discover` becomes
   a failing test rather than a review miss.
4. **A repository's own `core.worktree` can point its worktree outside the root, and that is a real
   escape.** Measured: a repository at `<tmp>/root` whose `.git/config` sets `core.worktree =
   <tmp>/outside` opens fine at `<tmp>/root`, reports `workdir() == <tmp>/outside/`, and `statuses()`
   lists a file outside the root. This is why containment compares the **canonicalized
   `repo.workdir()`** against the canonicalized root rather than trusting the path passed to `open`.
   It is the single most likely defect in the slice and it has its own test built from the measured
   reproduction (SC-002).
5. **`git_status` truncates and says so; `git_log` refuses. The asymmetry is deliberate.** `fs_read`
   refuses an oversized file because the model can act on the refusal by reading a smaller one.
   `git_status` takes **no arguments**, so there is no smaller call to make — a refusal would leave a
   dirty repository permanently unreadable. A labelled `# <n> more entries not shown` is not a wrong
   answer in a right answer's shape; a silent truncation would be. `git_log` has a `count` the model
   can lower, so it refuses like `fs_read` does.
6. **The capability gate is two layers, and the second is not decoration.** When the root is not a
   repository the server calls `ToolRouter::disable_route` for both names *and* `wiring::ToolArgs`
   omits them from the CLI allowlist. Both are required:
   `RmcpToolTransport` maps rmcp's `invalid_params("tool not found")` onto `HeddleError::Tool`, and
   `NativeLoop::mediate` survives **only** `HeddleError::ToolDenied` — so an allowlisted name whose
   route is disabled would turn a model's invented `git_status` into a **dead run** instead of a
   refusal it is told about. `wiring::ToolArgs::policy`'s existing docstring already states exactly
   this rule; this slice obeys it.

## Functional requirements

- **FR-001** `crates/heddle-connectors` gains `git2` (`default-features = false`, feature
  `vendored-libgit2`) and `chrono` (`default-features = false`, feature `std`) as product
  dependencies, both declared in the root `[workspace.dependencies]`. `git2` is additionally a
  **dev**-dependency of `heddle-connectors` and of `heddle-cli`, because an integration test in
  `tests/` does not inherit the crate's own product dependencies and the fixtures build their
  repositories with `git2`.
- **FR-002** `crates/heddle-connectors/src/git.rs` is the **only** module in the workspace that names
  `git2`. That boundary is deliberate: if `gix`'s footprint ever stops mattering, the swap touches
  one file.
- **FR-003** `open_contained(&FsRoot) -> Result<Repository, String>` is `pub(crate)` and every git
  tool starts there. In order: `Repository::open(root.path())` — never `discover`, `open_ext` or
  `open_from_env`; refuse `is_bare()`; refuse unless `canonicalize(repo.workdir())` equals
  `root.path()`. Every refusal is an `Err(String)`, which rmcp turns into `isError: true` — a tool
  error the model is told about, never a transport failure that ends the run.
- **FR-004** `is_git_repository(&FsRoot) -> bool` is public and is exactly "`open_contained`
  succeeded". `EmbeddedServer::new` and `heddle-cli`'s `wiring::ToolArgs` both need it and **neither
  may see `git2`**.
- **FR-005** The repository is opened **per call**, never held in the server. `git2::Repository` is
  not `Sync` and rmcp's handler must be `Clone + Send + Sync + 'static`; opening per call sidesteps
  that entirely, keeps `EmbeddedServer`'s fields as they are, and re-verifies containment on every
  call rather than caching a handle across a config change.
- **FR-006** `FsServer` is renamed `EmbeddedServer` and `fs_connector` is renamed `local_connector`.
  A name that lies is the defect this codebase's commentary exists to prevent, and the two docstrings
  asserting an `fs`-only server are rewritten in the same step rather than left stale — slice 016's
  own idiom (FR-002 there) applied to itself.
- **FR-007** `git_status` is a `#[tool]` method taking **no parameters at all**, whose derived schema
  is therefore `{"type":"object","properties":{}}`. `StatusOptions` is set explicitly:
  `include_untracked(true)`, `recurse_untracked_dirs(false)`, `include_ignored(false)`,
  `include_unmodified(false)`. Output is porcelain-v1 shaped: a `## <branch>` header, then
  `XY<TAB><path>` lines sorted by path, capped at `STATUS_ENTRY_CAP`, then a labelled
  `# <n> more entries not shown` when it truncated, or `# working tree clean` when there is nothing.
  An unborn branch is named (`## (unborn branch <name>)`) rather than failing; a detached HEAD is
  named too.
- **FR-008** Rename detection stays **off** (libgit2's default), so a rename appears as a delete plus
  an add. Documented in the tool description rather than configured on.
- **FR-009** `git_log` takes `Parameters<LogParams>` over `pub struct LogParams { pub count: u32 }`,
  required. It refuses `count == 0` and refuses `count > LOG_COUNT_CAP` with a message naming the
  cap, exactly as `fs_read` does. On an unborn HEAD it returns a plain tool error rather than
  propagating libgit2's `reference 'refs/heads/master' not found`.
- **FR-010** `git_log` emits one line per commit,
  `<7 hex><TAB><YYYY-MM-DDTHH:MM:SSZ><TAB><author name><TAB><summary>`, newest first. **The summary
  only, never the body**: it is the first line by definition, which bounds the output to `count`
  short lines and keeps a long commit body out of the prompt and off the chain. **The author's name
  only, never the email**: the model does not need it and it is needless personal data on an
  append-only chain. The date is UTC via `chrono`.
- **FR-011** `EmbeddedServer::new` stays infallible and calls `tool_router.disable_route` for
  `git_status` and `git_log` when `is_git_repository` is false. A root that is not a repository
  advertises no git tools and cannot be made to run one.
- **FR-012** `wiring::ToolArgs` gains `git_tools()`, appended to **both** `chat_policy` and
  `agent_policy` as `ToolAccess::ReadOnly`, returning the two names only when the configured root is
  a repository. Required for the reason FR-011's server gate is not sufficient (point 6 above).
- **FR-013** **No new flag.** `--fs-root` stays the one flag; its clap doc comment and `main.rs`'s
  module docstring are reworded to name the one directory an agent may work in — the filesystem tools
  always, and the git tools when that directory is a git repository.

## Success criteria

- **SC-001** `open_contained` refuses a directory that is not a repository, and a **subdirectory** of
  one — never walking up. `Repository::discover`'s contrasting behaviour is asserted in the same test.
- **SC-002** `open_contained` refuses a repository whose `core.worktree` points outside the root, and
  no file outside the root appears in any output.
- **SC-003** `open_contained` refuses a bare repository.
- **SC-004** `is_git_repository` and `open_contained` agree on every case above.
- **SC-005** `git_status` returns the branch header and correct porcelain `XY` codes for a staged
  addition, a worktree modification and an untracked file; says so plainly when clean; names an
  unborn branch; and caps its entries while reporting how many it dropped.
- **SC-006** `git_log` returns the newest commits first with short oid, UTC date, author name and
  **summary only**; refuses `0`; refuses a count over `LOG_COUNT_CAP` naming the cap; and is a tool
  error, not a panic, on a repository with no commits.
- **SC-007** `LocalConnector::list` returns three tools over a plain directory and five over a
  repository, `git_status`'s advertised schema has **empty** `properties`, and a disabled git tool is
  not callable by name.
- **SC-008** A secret configured on the `Redactor` and committed into a **real commit message**
  appears in **no** Ledger payload of a governed run, and at least one payload contains `***`.
- **SC-009** The only model-supplied value in the slice is a `u32`; a non-numeric `count` is refused
  at the typed boundary, reaches the model as `isError: true`, and the run survives. **No subprocess
  is spawned and no argument vector is constructed anywhere in the slice.**
- **SC-010** `heddle chat --fs-root <a real repository>` (the **real binary**) advertises the five
  tools and reports the repository's real status; `ledger verify` passes.
- **SC-011** `heddle acp-agent --fs-root <a real repository>` advertises the five tools.
- **SC-012** Every pre-existing test passes with **no assertion changed or removed**. The only edits
  to pre-existing test files are import lines touched by FR-006's rename. In particular
  `connector.rs`'s three-tool catalogue test, `cli_chat.rs`'s `["fs_read","fs_list"]` assertion,
  `cli_chat.rs`'s no-`tools`-key control and `cli_acp_agent.rs`'s
  `["fs_read","fs_list","fs_write"]` assertion all keep their bodies, because each uses a
  non-repository root. **If one of them needs an assertion changed, the gate is wrong; stop and fix
  the gate.**
- **SC-013** `git diff dev -- crates/heddle-silo/ spikes/ .github/ rust-toolchain.toml` is **empty**.

## Assumptions and residuals

- **The connector trusts the system and global git configuration the way `git` itself does.** A
  global `core.excludesFile` affects what `git_status` reports. Recorded rather than hidden.
- **`.git/` remains readable through `fs_read`** exactly as on `dev`. This slice neither widens nor
  narrows that.
- **Slice 016's TOCTOU residual is unchanged and still open**, and `open_contained` inherits it: a
  directory swapped between `canonicalize` and `Repository::open` escapes the root.
- **libgit2's owner-validation** refuses a repository owned by another user. It surfaces as a
  tool-level refusal the model is told, like any other refusal.
  `git2::opts::set_verify_owner_validation` is deliberately **not** called — silencing a safety check
  to make a tool convenient is the wrong trade.
- **The entry cap bounds the output, not the walk.** `recurse_untracked_dirs(false)` and
  `include_ignored(false)` bound the walk the way `git status --porcelain` bounds its own; a huge
  worktree can still make `git_status` slow. Accepted and stated — a timeout belongs to a slice that
  has timeout machinery.
- **A live model may not call a zero-argument tool.** That is a model-selection finding, not a code
  defect, and if it recurs the follow-up is a documented note here, **not** an invented dummy
  parameter.
- **The Windows leg is observed locally; the macOS and Linux legs are unobserved** until this
  repository has a remote — the standing caveat of slices 004–016. Its bite is real here because this
  is the first slice to add a C dependency; `vendored-libgit2` is what makes it one pinned libgit2 on
  all three OSes rather than whatever a runner happens to have.
- **ADR-0004 D3 closes for `fs` and `git` and remains open for `shell`.** Said here rather than
  claiming the item done.

## Out of scope

Deliberately not done, so nobody helpfully does it:

- **`git diff`, `git blame`, `git show`, branch/tag/remote listing, `git stash list`.** Two tools,
  per Principle VII. `diff` in particular is an unbounded-output tool and would need its own cap
  design.
- **Any git operation that mutates repository state** — `add`, `commit`, `checkout`, `reset`,
  `restore`, `stash`. Not classified `Mutating`-and-deferred: **not built**. This slice is read-only
  in scope, not read-only by policy choice, so there is no code path to misclassify.
- **Any operation that could reach a remote** — `fetch`, `pull`, `push`, `clone`, `ls-remote`,
  `submodule update`. Constitution II is NON-NEGOTIABLE. Beyond not calling them, `git2` is compiled
  with `default = []`, so no HTTPS or SSH transport is linked in at all. This repository has no
  remote, so its own tests could not have caught a remote call by accident; the guarantee is
  therefore made at the code and build level rather than left to the fixture.
- **An arbitrary `git` subcommand passthrough tool.** Shell execution in git clothing. Not built, and
  a fixed argv would not have made it safe either — see point 2.
- **Any subprocess at all.** No `Command`, no `std::process`, in product code.
- **A `shell` connector.** Still deferred, and slice 016's reasoning stands unamended: shell's blast
  radius is bounded by nothing this tree has, and an allowlist of command *names* is not an allowlist
  of *effects*. Rejecting a `git` subprocess is a narrower judgment about one fixed argv and does
  **not** reopen that deferral — if anything the `core.fsmonitor` measurement strengthens it.
- **A `--git-root` flag, or any second root.**
- **Design §5.4/§5.5's connector configuration hierarchy**, the scope-owner resolver and
  `AccessScope::{Project,Folder,FullComputer}` — still absent from `crates/`, verified again this
  slice. One operator-named root; nothing hierarchical.
- **Rename detection in `git_status`**, submodule status recursion, `-uall`-style untracked
  recursion, ignored-file reporting, pathspec filtering.
- **Deriving `ToolAccess` from MCP tool annotations.** Classification stays operator configuration.
- **The ACP permission gate exercised end to end**, the TOCTOU residual, `role: "tool"` /
  `tool_call_id` replay, raw wire-byte capture, streaming (SSE), provider authentication, a config
  file, `--json` output, and the slices-008-vs-014 `serde_json/preserve_order` reconciliation — all
  carried unchanged from slice 016's `## Next slice`.
- **`crates/heddle-silo/`, `spikes/`** (ADR-0004 D2), **`.github/`, `rust-toolchain.toml`.**
