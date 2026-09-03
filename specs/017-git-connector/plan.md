# Implementation Plan: a read-only `git` connector (`git_status`, `git_log`) — slice 017

**Spec:** `specs/017-git-connector/spec.md` · **Tasks:** `specs/017-git-connector/tasks.md` ·
**Branch:** `017-git-connector`, cut from `dev` at `5d36c1d` · **No PR** (this repository has no
remote).

Everything named below was read in the working tree at `5d36c1d` unless it is explicitly marked
**new**. Where this plan contradicts the request's own assumptions, it says so and gives the
measurement.

---

## Problem

ADR-0004 D3 (`docs/superpowers/adr/0004-solo-v0-calibration.md`) names three connector families as
v0 scope — *"MCP tools (fs/git/shell)"*. Slice 016 closed `fs` and left `git` and `shell` open,
saying so in `specs/016-fs-connector/tasks.md` under **Out of scope** and **Next slice**. A coding
agent that can read files but cannot see what changed, what is staged, or what the recent history
looks like is missing the single most common piece of context a coding assistant needs. This slice
closes the `git` portion: two read-only MCP tools, `git_status` and `git_log`, bounded to the same
one operator-named directory `--fs-root` already bounds.

`shell` stays deferred, and slice 016's reasoning for that deferral stands untouched — see
**Out of scope**.

---

## What was verified before planning (and where the request's premises need correcting)

Load-bearing facts, each measured this session rather than assumed. The probes live under this run's
artifact directory (`probe-git2/`, `probe-gix/`, `gitprobe/`, `wtprobe/`) and touched nothing in the
repository; `git status --short` in `D:\claudecode\skein` is empty.

1. **`crates/skein-connectors` structure.** `src/{lib.rs,connector.rs,fs.rs,server.rs}` and
   `tests/{connector.rs,fs_root.rs,fs_server.rs,governed_fs_run.rs}`. `fs.rs` holds `FsRoot`
   (containment only); `server.rs` holds `FsServer` — one `#[tool_router]` impl with `fs_read`,
   `fs_list`, `fs_write`, each returning `Result<String, String>`; `connector.rs` holds
   `fs_connector(FsRoot) -> Result<LocalConnector>` wiring one `FsServer` to one
   `RmcpToolTransport` over a `tokio::io::duplex`. **It generalizes to a second family with one
   caveat:** rmcp serves exactly one `ServerHandler` per connection, so a second family means either
   more `#[tool]` methods on the *same* handler or a second connector plus a fan-out transport. See
   **D3**.
2. **A `#[tool]` method with no `Parameters<T>` argument is legal**, and gets the schema
   `{"type":"object","properties":{}}` — `rmcp-macros-2.2.0/src/tool.rs` (`find_parameters_type_impl`
   returning `None` → `schema_for_empty_input()`), and `rmcp-2.2.0/src/handler/server/common.rs:99`.
   So `git_status` can genuinely take **no arguments at all**.
3. **`ToolRouter::disable_route(name)` exists and is a real capability gate.**
   `rmcp-2.2.0/src/handler/server/router/tool.rs`: `list_all` filters disabled names,
   `get` returns `None` for them, and `call` returns `invalid_params("tool not found")` for them.
   This is how the git tools stay invisible when the configured root is not a repository.
4. **`RmcpToolTransport` maps any rmcp protocol error to `SkeinError::Tool`**
   (`crates/skein-mcp/src/lib.rs`, `call`), and `NativeLoop::mediate`
   (`crates/skein-core/src/native_loop.rs`) survives **only** `SkeinError::ToolDenied` — everything
   else ends the run. So "disabled route" alone is not enough: the CLI allowlist must also omit the
   git names when the root is not a repository, or a model inventing `git_status` would kill the run
   instead of being told "not in the allowlist". `wiring::ToolArgs::policy`'s existing docstring
   already states exactly this rule; this slice obeys it.
5. **An argument *deserialization* failure is a tool-level error, not a protocol error.**
   `into_tool_argument_error` (same file as (3), line 146) converts an `INVALID_PARAMS` error whose
   message carries the deserialization prefix into `CallToolResult::error(...)` — i.e. `isError:
   true`, told to the model, run survives. This is what makes the "crafted `count`" acceptance test
   assert a *survivable* refusal.
6. **`git2::Repository::open` does not walk up; `Repository::discover` does.** Measured
   (`probe-git2`): `open("<repo>/sub")` → `ErrorCode::NotFound`, *"could not find repository at
   …/repo/sub"*; `discover("<repo>/sub")` → succeeded with `workdir = …/repo/`. `open` is therefore
   the containment primitive, and `discover` / `open_ext` / `open_from_env` must never appear in this
   slice.
7. **The system `git` binary *does* walk up, silently.** Measured with git 2.53.0.windows.1:
   `git -C <repo>/sub status --porcelain=v1 -b` printed `## master` with exit 0, and
   `git -C <repo>/sub rev-parse --show-toplevel` printed the **parent** repository's path. So
   `git -C <root> status` is **not** root-bounded — exactly the doubt the request raised, confirmed.
8. **A repository's own `.git/config` can make `git status` execute an arbitrary program.**
   Measured: with `core.fsmonitor` set to a script in `.git/config`, `git status` ran it (the script
   wrote `marker.txt`, which appeared). Re-run against the same repository through `git2`, the marker
   did **not** appear, and `config.get_string("core.fsmonitor")` returned the value — libgit2 reads
   the key and does not execute it. A fixed argv with no shell is therefore *not* the whole risk
   surface of the subprocess route; this is a measured, code-execution-class difference.
9. **A repository's own `core.worktree` can point its worktree outside the configured root.**
   Measured (`wtprobe`): a repo at `<tmp>/root` with `core.worktree = <tmp>/outside` opens fine at
   `<tmp>/root`, reports `workdir() == <tmp>/outside/`, and `statuses()` listed `outside.txt` — a
   file outside the root. **This is a real escape and it is why the containment check below compares
   `repo.workdir()` against the root**, rather than trusting the path passed to `open`.
10. **Dependency footprint, measured with `cargo tree -e normal` on this machine (Windows,
    rustc 1.97.1):**
    - `gix 0.87.1`, `default-features = false, features = ["status", "revision"]` → **112 unique
      packages**. (`status` transitively enables `dirwalk`, `index`, `blob-diff`, `attributes`,
      `excludes`, `command` → `gix-filter`, `gix-pathspec`, `gix-attributes`, `gix-submodule`,
      `gix-dir`, `gix-ignore`, `gix-worktree`, `gix-index`, `gix-diff`, `gix-status`.)
    - `git2 0.21.0`, `default-features = false` → **7 packages total** (`git2`, `libgit2-sys`,
      `libz-sys`, `libc`, `bitflags`, `log`). git2 0.21's `default = []`: `https` and `ssh` are
      **opt-in**, and with them off the tree contains no `openssl-sys` and no `libssh2-sys`.
    - Against `skein-cli`'s current graph (136 normal packages), git2 adds exactly **four** new
      normal packages: `git2`, `libgit2-sys`, `libz-sys`, `libc`. `bitflags`, `log`, `chrono` and
      `num-traits` are already there.
    - Maintenance, from the crates.io API: `git2` 0.21.0, 111.5M total / 15.6M recent downloads,
      published 2026-05-18, `rust-lang/git2-rs`. `gix` 0.87.1, 44.9M total / 9.7M recent, published
      2026-08-24, `GitoxideLabs/gitoxide`. **Both are proven and actively maintained** — this is not
      the `mcp-server-filesystem` situation slice 016 rejected, and the decision below turns on
      footprint and containment semantics, not on maintenance.
11. **A C toolchain is already a hard build prerequisite of this workspace.**
    `libsqlite3-sys 0.38.2` is in `skein-cli`'s shipped normal graph, and the root `Cargo.toml`
    pins `rusqlite = { version = "0.40", default-features = false, features = ["bundled"] }` — the
    bundled SQLite amalgamation is compiled with `cc` on all three OSes. **Slice 016's
    "libgit2 C bindings, a tri-OS build burden" judgment is therefore materially wrong today**, and
    this plan records the conflict rather than inheriting it.
12. **git2 0.21 + vendored libgit2 1.9.7 compiles and runs on the pinned toolchain.** The probe was
    built and run with `rustc 1.97.1 (8bab26f4f 2026-07-14)` — the channel in
    `rust-toolchain.toml` — on Windows 11.
13. **Exact git2 0.21 signatures** (registry source + probe output), because the plan names them:
    `Repository::{open, is_bare, workdir -> Option<&Path>, is_empty -> Result<bool>, head ->
    Result<Reference>, head_detached -> Result<bool>, statuses, revwalk, find_commit, config}`;
    `Reference::shorthand -> Result<&str, Error>`; `StatusEntry::{status -> Status, path ->
    Result<&str, Error>, path_bytes -> &[u8]}`; `Statuses::{len, is_empty, iter}`;
    `Commit::summary -> Result<Option<&str>, Error>`; `Commit::{author, time, id}`;
    `Revwalk::{push_head, set_sorting}`; `Sort::TIME`; `Status` flags `CURRENT`,
    `INDEX_{NEW,MODIFIED,DELETED,RENAMED,TYPECHANGE}`,
    `WT_{NEW,MODIFIED,DELETED,TYPECHANGE,RENAMED,UNREADABLE}`, `IGNORED`, `CONFLICTED`.
    On an unborn HEAD: `repo.head()` → `ErrorCode::UnbornBranch`, `revwalk.push_head()` →
    `GenericError` *"reference 'refs/heads/master' not found"*, `repo.is_empty()` → `Ok(true)`.
14. **`chrono 0.4.45` is already in the shipped graph** (via `rmcp` and `schemars`), and
    `DateTime::from_timestamp(secs, 0).format("%Y-%m-%dT%H:%M:%SZ")` compiles and runs with
    `default-features = false, features = ["std"]` — measured, printing
    `2026-09-03T13:03:18Z`. A direct edge adds zero packages.
15. **The two pre-existing advertisement assertions use non-repository roots.**
    `cli_chat.rs`'s `fs_root_holding` and `cli_acp_agent.rs`'s
    `acp_agent_accepts_an_fs_root_and_still_serves_a_session` both build a plain `TempDir`, and
    `connector.rs`'s `fn connector()` does too. Under the design below (git tools disabled when the
    root is not a repository) **all three keep passing with no edit** — `["fs_read","fs_list"]`,
    `["fs_read","fs_list","fs_write"]`, and the three-tool catalogue respectively. This is a
    designed consequence, not luck, and SC-012 pins it.

---

## Approach

### D1 — Use `git2` (libgit2 bindings) in-process. Not `gix`. Not a `git` subprocess.

**Chosen: `git2 = { version = "0.21", default-features = false, features = ["vendored-libgit2"] }`.**

Why, against the two real alternatives:

- **A `git` subprocess with a fixed argv — rejected, and *not* for the reason slice 016 gave.**
  The request is right that a fixed `Command::new("git").args([...])` with no shell and no
  operator-controlled command name is a different hazard class from an arbitrary shell tool; slice
  016's plan did conflate the two. It is rejected on three *measured* grounds instead:
  1. **It is not root-bounded.** Fact (7): `git -C <root> …` discovers upward, so a root that is a
     subdirectory of a repository silently reports on the enclosing repository. Containing it needs
     `--git-dir` + `--work-tree` and/or `GIT_CEILING_DIRECTORIES` discipline whose failure mode is
     *silent success on the wrong repository*. `Repository::open` refuses that case by construction
     (fact 6).
  2. **A "read-only" call can execute code the operator never named.** Fact (8): `core.fsmonitor`
     in the target repository's own `.git/config` is executed by `git status`. The argv is fixed and
     the shell is absent, and the program still runs. libgit2 does not implement fsmonitor and did
     not run it. Constitution II makes local-only a property of the *build* in `skein-gateway`
     (no TLS backend compiled in); handing an arbitrary repository-config-selected executable a seat
     on that path gives the guarantee back at runtime, which is the same objection slice 016 made to
     an out-of-process Node MCP server.
  3. **It makes a shipped capability depend on an unpinned external binary.** Output format, exit
     codes and locale come from whatever `git` is on `PATH`, on three OSes, with the tri-OS matrix
     required green before merge. `git` is not in `scripts/bootstrap.ps1`'s contract as a *runtime*
     dependency of the product.
- **`gix` — rejected on footprint, not on quality.** Fact (10): 112 packages for `status` +
  `revision` against a binary whose entire normal graph is 136 packages today. Nearly doubling the
  shipped package graph to add two read-only tools fails Principle VII's "start simple" as squarely
  as reimplementing status would. Its pure-Rust, no-C property would be the decisive advantage if a
  C toolchain were a new burden — fact (11) says it is not, because `rusqlite`'s bundled SQLite
  already compiles C on every OS in this workspace. Recorded honestly: `gix` is the ecosystem's
  direction and is memory-safe where libgit2 is 200k lines of C. If the footprint stops mattering —
  or if `gix`'s `status` stabilizes into a thinner feature set — revisiting is cheap, because
  nothing outside `crates/skein-connectors/src/git.rs` will name either library.
- **`git2`, chosen.** Four new shipped packages (fact 10). No network transport compiled in
  (`default = []`; `https`/`ssh` off ⇒ no `openssl-sys`, no `libssh2-sys` in the tree) — the same
  *build-property* argument `skein-gateway` already makes, now applied to git. `Repository::open`
  is the containment primitive (fact 6). API stability: `statuses`/`revwalk` are libgit2's oldest
  and most-used surfaces, from `rust-lang/git2-rs`, at 15.6M recent downloads. Verified to build and
  run on the pinned toolchain (fact 12).

`vendored-libgit2` is deliberate: it pins libgit2 1.9.7 identically on all three OSes instead of
linking whatever a runner happens to have, mirroring `rusqlite`'s `bundled` choice already in the
root manifest. It adds no packages — it is a feature of `libgit2-sys`.

**No subprocess is spawned anywhere in this slice**, so the request's fixed-argv invariant is
satisfied vacuously rather than by discipline: there is no command line, no argument vector, and no
place where a model-supplied value could become command structure. That is the strongest available
form of the guarantee, and it is the reason the injection acceptance test (SC-009) reduces to a
typed-boundary test.

### D2 — Containment: reuse `FsRoot`, and require the repository's *worktree* to be the root

The git tools take **no path arguments whatsoever**, so containment is not "resolve this path
safely" — it is "open exactly one repository and refuse anything else". New in
`crates/skein-connectors/src/git.rs`:

```rust
// pub(crate); every git tool starts here.
fn open_contained(root: &FsRoot) -> std::result::Result<Repository, String>;
/// True when `open_contained` succeeds. Public: `EmbeddedServer::new` and
/// `skein-cli`'s `wiring::ToolArgs` both need it, and neither may see `git2`.
pub fn is_git_repository(root: &FsRoot) -> bool;
```

`open_contained`, in order:

1. `Repository::open(root.path())` — never `discover`, never `open_ext`, never `open_from_env`.
   `root.path()` is already canonicalized by `FsRoot::new`. A non-repository, or a subdirectory of
   one, is refused here (fact 6).
2. Refuse `repo.is_bare()` — a bare repository has no worktree, so `git_status` would be
   meaningless, and `workdir()` is `None`.
3. **Refuse unless `std::fs::canonicalize(repo.workdir())` equals `root.path()`.** Fact (9): without
   this, a repository whose own `.git/config` sets `core.worktree` reports on files outside the root.
   Both sides are canonicalized, so the comparison is verbatim-vs-verbatim on Windows — the same
   reasoning `FsRoot::new`'s docstring already records.
4. Every refusal is an `Err(String)`, which rmcp turns into `isError: true` — a tool error the model
   is told about, never a transport failure that ends the run (slice 016's D3, and fact 5's sibling).

**The repository is opened per call, not held in the server.** `git2::Repository` is not `Sync`,
and rmcp's handler must be `Clone + Send + Sync + 'static`; opening per call sidesteps that
entirely, keeps `EmbeddedServer`'s fields exactly as they are today (`Arc<FsRoot>` + `ToolRouter`),
and re-verifies containment on every call rather than caching a handle across a config change.

*Rejected: a new `GitRoot` type.* `FsRoot` is already precisely "one canonicalized directory that
exists", which is all a repository root needs; a second type would duplicate the canonicalization
rule that is the whole safety argument of slice 016. `FsRoot`'s `resolve`/`resolve_new` simply go
unused by the git tools — because the git tools accept no paths.

### D3 — One embedded server, two families; rename `FsServer` → `EmbeddedServer`

rmcp serves one `ServerHandler` per connection, so the two git tools become two more `#[tool]`
methods on the existing router. `FsServer` and `fs_connector` then name something they are not, so
they are renamed:

| today | after |
|---|---|
| `FsServer` (`src/server.rs`) | `EmbeddedServer` |
| `fs_connector(FsRoot)` (`src/connector.rs`) | `local_connector(FsRoot)` |
| `FsRoot`, `READ_BYTE_CAP`, `*Params` | unchanged |

Roughly twelve mechanical lines across `src/lib.rs`, `src/server.rs`, `src/connector.rs`,
`tests/{connector.rs,fs_server.rs,governed_fs_run.rs}` and `crates/skein-cli/src/wiring.rs`. This
is slice 016's own idiom applied to itself — it amended `skein-mcp`'s docstring invariant rather
than leaving it stale, on the grounds that a stale invariant is worse than a restated one. The two
docstrings that assert "the embedded `fs` MCP server: three tools over one `FsRoot`" and
`get_info`'s `with_instructions` text are rewritten in the same step.

*Rejected: a second connector plus a fan-out `ToolTransport`.* It would double the tokio runtimes
per ACP session (already one per session, an accepted v0 cost) and add a multiplexer with exactly
one caller. *Rejected: keeping the name `FsServer` while it hosts `git_status`.* A name that lies is
the defect this codebase's commentary exists to prevent.

### D4 — Capability gating, at two layers, each for its own reason

`EmbeddedServer::new(root)` (still infallible) builds the router and then:

```rust
if !git::is_git_repository(&root) {
    tool_router.disable_route("git_status");
    tool_router.disable_route("git_log");
}
```

- **Server layer.** A root that is not a repository advertises no git tools and cannot be made to
  run one (fact 3: `list_all`, `get` and `call` all honour `disabled`). This is the server reporting
  what it can actually do — the same thing `tools/list` already is — and it is what keeps the three
  pre-existing advertisement assertions green untouched (fact 15).
- **CLI layer.** `wiring::ToolArgs`'s allowlist gains `git_status`/`git_log` as
  `ToolAccess::ReadOnly` **only when the root is a repository**. This is not decoration: fact (4)
  shows that an allowlisted name whose route is disabled produces a `SkeinError::Tool` from the
  transport, and `NativeLoop::mediate` ends the run on anything but `ToolDenied`. Omitting the names
  from the allowlist turns a model's invented `git_status` into a survivable `denied` with a reason —
  which is exactly what `wiring::ToolArgs::policy`'s existing docstring demands.

Both classifications are `ReadOnly` because neither tool mutates anything, following `read_only()`'s
existing comment verbatim in spirit: classification is operator configuration, never read from the
server's own annotations. Both commands allowlist both tools identically — there is no `fs_write`-
style asymmetry, because there is nothing here to confirm.

**No new flag.** `--fs-root` stays the one flag, and its clap doc comment plus `main.rs`'s module
docstring are reworded from "the fs tools" to "the one directory an agent may work in — the
filesystem tools always, and the git tools when that directory is a git repository". Adding
`--git-root` would either duplicate the root or create the two-connector composition D3 rejected,
and renaming a shipped flag for cosmetics is worse than a slightly narrow name.

### D5 — The two tools, and what bounds their output

Both are `#[tool]` methods on `EmbeddedServer` returning `Result<String, String>`, per slice 016's
D3. New constants in `src/server.rs` alongside `READ_BYTE_CAP`:
`pub const LOG_COUNT_CAP: u32 = 50;` and `pub const STATUS_ENTRY_CAP: usize = 200;`.

**`git_status` — no parameters** (fact 2). `StatusOptions` set explicitly:
`include_untracked(true)`, `recurse_untracked_dirs(false)`, `include_ignored(false)`,
`include_unmodified(false)`. `recurse_untracked_dirs(false)` matches `git status --porcelain`'s own
default — an untracked directory collapses to one `dir/` entry — which bounds both the output and
the walk. Rename detection stays off (libgit2's default), so a rename appears as a delete plus an
add; documented in the tool description rather than configured on.

Output, entries sorted by path so the same worktree reads the same way twice (`fs_list`'s
precedent):

```
## <branch>                        # or "## (detached HEAD at <7 hex>)" / "## (unborn branch <name>)"
XY<TAB><path>                      # 0..=STATUS_ENTRY_CAP lines
# working tree clean               # only when there are no entries
# <n> more entries not shown, over the <cap>-entry cap    # only when truncated
```

`XY` is git's porcelain-v1 two-character code, derived from `Status` flags by an explicit documented
mapping: `X` from `INDEX_NEW`→`A`, `INDEX_MODIFIED`→`M`, `INDEX_DELETED`→`D`, `INDEX_RENAMED`→`R`,
`INDEX_TYPECHANGE`→`T`, else space; `Y` from `WT_MODIFIED`→`M`, `WT_DELETED`→`D`,
`WT_TYPECHANGE`→`T`, `WT_RENAMED`→`R`, else space; `WT_NEW` alone → `??`; `CONFLICTED` → `UU`.
Porcelain codes rather than prose because a model has seen millions of lines of them, and the
`## <branch>` header is porcelain's own `-b` line — a coding agent wants the branch.

**Status truncates and says so; `git_log` refuses.** The asymmetry is deliberate and stated:
`fs_read` refuses an oversized file because the model can act on the refusal by reading a smaller
one. `git_status` takes no arguments, so there is no smaller call to make — a refusal would leave a
dirty repository permanently unreadable. A labelled `# <n> more entries not shown` is not a wrong
answer in a right answer's shape; a silent truncation would be.

**`git_log` — `Parameters<LogParams>` over `pub struct LogParams { pub count: u32 }`**, required.
Refuses `count == 0` ("name at least one commit") and `count > LOG_COUNT_CAP` with a message naming
the cap, exactly as `fs_read` does. On an unborn HEAD it returns
`Err("the repository has no commits yet")` rather than propagating libgit2's
`reference 'refs/heads/master' not found` (fact 13). Walk: `revwalk()`, `push_head()`,
`set_sorting(Sort::TIME)`, `take(count)`.

One line per commit:

```
<7 hex><TAB><YYYY-MM-DDTHH:MM:SSZ><TAB><author name><TAB><summary>
```

The date is the commit time rendered in **UTC** via `chrono` (fact 14) — unambiguous, and zero new
packages. **`Commit::summary` only**, never the body: it is the first line by definition, which
bounds the output to `count` short lines and keeps a long commit body out of the prompt and off the
chain. **Author name only, never the email** — the model does not need it and it is needless
personal data on an append-only chain.

### D6 — Redaction and traceability compose unchanged

No new `StepKind` and none needed. A git tool call lands as `ToolCall`/`Approval`/`ToolResult`
through the gateway slice 005 built, and slice 014's `Redactor` scrubs `ToolResult` content on its
way into the chain (`ToolGateway::call_captured`, `crates/skein-core/src/tool.rs`). The request asks
this be verified rather than assumed for git output, because a commit message can itself carry a
secret — SC-008 does exactly that with a configured secret committed into a real commit message,
mirroring slice 016's T7. The unconfigured case remains the same stated gap slice 016 recorded.

---

## Steps

Strict TDD (Constitution III): each step's red is observed and recorded verbatim in `tasks.md`
under `## Observed red` before its green. Steps are ordered so each is independently verifiable.

- **T0** `specs/017-git-connector/{spec.md,plan.md,tasks.md}`, mirroring slice 016's format and its
  `## Constitution Check` table shape (the eight principles plus the `Cross-platform` row).
  Branch `017-git-connector` cut from `dev`.
- **T1** Control baseline: `cargo test --workspace` before any edit, recorded verbatim per target.
  `specs/016-fs-connector/tasks.md` records **165 passed, 2 ignored** at `b7c9918`; `dev` is now
  `5d36c1d`, two documentation-only commits later, so the figure is expected to be identical and
  must be re-measured rather than quoted.
- **T2 · manifests.** Add to root `[workspace.dependencies]`:
  `git2 = { version = "0.21", default-features = false, features = ["vendored-libgit2"] }` and
  `chrono = { version = "0.4", default-features = false, features = ["std"] }`. Add
  `git2.workspace = true` and `chrono.workspace = true` to `crates/skein-connectors/Cargo.toml`
  `[dependencies]`, **and `git2.workspace = true` to its `[dev-dependencies]` as well** — an
  integration test in `tests/` does not inherit the crate's own product dependencies, and the
  fixtures build their repositories with `git2`. Same dev-dependency line in
  `crates/skein-cli/Cargo.toml`. Verify `cargo build --workspace` still succeeds (this is the step
  that proves libgit2 compiles in CI's toolchain, not a later one).
- **T3 · RED→GREEN — containment.** New `crates/skein-connectors/tests/git_root.rs` against a
  `git.rs` that does not exist yet. Fixtures build **real repositories with real commits** using
  `git2` (`Repository::init`, index add, `commit`) rather than shelling out to a `git` binary: no
  `PATH` assumption, deterministic on three OSes, and still a real on-disk repository — libgit2
  writes real objects. Then `src/git.rs` with `open_contained` and `is_git_repository`, exported
  from `lib.rs`.
- **T4 · RED→GREEN — the two tools.** New `crates/skein-connectors/tests/git_server.rs` calling the
  `#[tool]` methods directly, which is the level that sees an `Err(String)` before rmcp wraps it
  (`tests/fs_server.rs`'s precedent). Then `git_status`, `git_log`, `LogParams`, `LOG_COUNT_CAP`,
  `STATUS_ENTRY_CAP`, and the porcelain/log formatting in `git.rs`.
- **T5 · RED→GREEN — the rename and the capability gate.** `FsServer` → `EmbeddedServer`,
  `fs_connector` → `local_connector` (D3), the two `disable_route` calls in `EmbeddedServer::new`,
  and the rewritten docstrings and `get_info` instructions. Driven by two new tests appended to the
  pre-existing `crates/skein-connectors/tests/connector.rs`. **The pre-existing
  `the_connector_lists_the_three_tools_with_their_derived_schemas` must keep its assertions
  unchanged** — only its import line moves — because its fixture root is a plain `TempDir`
  (fact 15). If that test needs an assertion changed, the gate is wrong; stop and fix the gate.
- **T6 · RED→GREEN — the headline governed run.** New
  `crates/skein-connectors/tests/governed_git_run.rs`, structured on `governed_fs_run.rs` and
  reusing its shapes (`Stub::serving`, `request_body`, `tool_call_reply`, `final_reply`,
  `NoGroundTruth`, `captured_requests`, `escaped`) — copied rather than shared, exactly as
  `governed_fs_run.rs` restates `chat_policy` rather than importing it, because `skein-cli` has no
  `lib` target and Rust integration-test binaries do not share helpers. Nothing between the model
  and the repository is a double.
- **T7 · RED→GREEN — redaction over a commit message** (SC-008), in the same file.
- **T8 · RED→GREEN — the injection boundary** (SC-009), in `git_server.rs` and
  `governed_git_run.rs`.
- **T9 · RED→GREEN — `skein-cli` wiring.** `read_only()` splits into the fs pair plus a
  `git_tools(&self)` helper on `ToolArgs` returning the two git names when
  `self.fs_root.as_ref().and_then(|p| FsRoot::new(p).ok()).is_some_and(is_git_repository)`;
  `chat_policy` and `agent_policy` both append it. `unwrap_or(false)` on a bad root is safe here and
  must be commented as such: `transport()` / `verify_root()` have already failed loudly on a
  mistyped `--fs-root` earlier in both commands' documented ordering, so this can never be the thing
  that hides an operator's typo. Reword `--fs-root`'s doc comment and `main.rs`'s module docstring
  (D4).
- **T10 · RED→GREEN — CLI acceptance against the real binary.** One test appended to
  `crates/skein-cli/tests/cli_chat.rs` and one to `crates/skein-cli/tests/cli_acp_agent.rs`
  (SC-010, SC-011). No pre-existing assertion in either file changes (fact 15).
- **T11** The `#[ignore]`d live-model test `a_live_model_calls_a_real_git_tool` in
  `governed_git_run.rs`, gated on `SKEIN_LIVE_MODEL` and skipping with a printed note when it is
  unset — `a_live_model_calls_a_real_fs_tool`'s pattern exactly, so the hand-verification is
  repeatable rather than a one-off.
- **T12** Gates, control diff, dependency drift, close-out. The drift section must state the
  **measured** package delta (fact 10: four new normal packages; `chrono` a new edge to an
  already-present package; `cc`/`jobserver` build-only) and must re-measure rather than quote this
  plan. It must also correct slice 016's recorded "libgit2 is a tri-OS build burden" claim with fact
  (11), because that claim is now load-bearing in the wrong direction.
  `git diff dev --stat -- crates/skein-silo/ spikes/ .github/ rust-toolchain.toml` must be empty.
- **T13** Hand-verification against live Ollama. **Not part of the implementation run**; performed
  separately and recorded under `## Live verification` in `tasks.md`.

---

## Validation

### The project's own gates (ADR-0004 D1(c)/(d), `docs/QUALITY-GATES.md`)

`cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo test --workspace`; `cargo build --workspace`. Tri-OS CI (`.github/workflows/core.yml`) needs
no edit: the workspace is `members = ["crates/*"]` and the workflow's `paths:` already reads
`crates/**`. **No `#[cfg]` anywhere in this slice's product code** — the containment comparison is
between two canonicalized paths, which is exactly why both sides are canonicalized.

### New tests

`crates/skein-connectors/tests/git_root.rs` — containment (SC-001…SC-004):

- `a_repository_at_the_root_opens_and_reports_that_root_as_its_worktree`
- `a_directory_that_is_not_a_repository_is_refused_and_says_so`
- `a_subdirectory_of_a_repository_is_refused_rather_than_walked_up_from` — the escape test.
  Repository at `<tmp>/repo` with a commit, `FsRoot` at `<tmp>/repo/sub`. Asserts the refusal, and
  asserts the parent repository's branch name and commit summary appear nowhere in it. Also asserts
  `Repository::discover` on the same path *does* succeed, so the test states which of the two APIs
  the guarantee depends on — the way `fs_root.rs`'s absolute-path test asserts the `Path::join`
  footgun itself.
- `a_repository_whose_config_points_its_worktree_outside_the_root_is_refused` — the second escape
  test, built exactly as fact (9): repo at `<tmp>/root`, `core.worktree` set to `<tmp>/outside`
  holding `outside.txt`. Asserts refusal, and that `outside.txt` appears in nothing.
- `a_bare_repository_is_refused`
- `is_git_repository_agrees_with_open_on_every_one_of_those_cases` — the wiring gate and the server
  gate must not be able to disagree.

`crates/skein-connectors/tests/git_server.rs` — the tools as server methods (SC-005, SC-006, SC-009):

- `git_status_reports_the_branch_and_the_staged_and_worktree_changes_in_porcelain_form` — one
  committed-then-modified file, one staged addition, one untracked file; exact expected string.
- `git_status_says_plainly_when_the_working_tree_is_clean`
- `git_status_caps_its_entries_and_says_how_many_it_did_not_show`
- `git_status_names_an_unborn_branch_rather_than_failing`
- `git_log_returns_the_most_recent_commits_newest_first` — three commits; asserts order, the short
  oid prefix, the author name, and the UTC date shape.
- `git_log_returns_only_a_commits_summary_line_not_its_body` — a commit with a multi-line message;
  asserts the body text is absent.
- `git_log_refuses_a_count_over_the_cap_and_names_the_cap`
- `git_log_refuses_a_count_of_zero`
- `git_log_on_a_repository_with_no_commits_is_a_tool_error_not_a_panic`
- `git_status_outside_its_root_is_refused_by_the_server` — the same containment fixtures reached
  through the tool method, so the refusal is proven at the layer the model actually touches.
- `a_count_that_is_not_a_number_is_refused_by_deserialization` —
  `serde_json::from_value::<LogParams>(json!({"count": "5 --upload-pack=touch pwned"}))` fails. The
  positive half of the injection claim: the only model-supplied value in the slice is a `u32`, and
  there is no command line for it to reach.

`crates/skein-connectors/tests/connector.rs` — appended (SC-007):

- `the_connector_lists_the_git_tools_only_when_the_root_is_a_repository` — plain root → the three fs
  names; repository root → five names, with `git_status`'s advertised schema having an **empty**
  `properties` object (there is nothing to inject) and `git_log`'s carrying `count`.
- `a_git_tool_whose_route_is_disabled_is_not_callable_by_name` — asserts `Err` from
  `LocalConnector::call` on a non-repository root, and its comment records *why the CLI allowlist
  gate exists*: this error is a `SkeinError::Tool`, which `NativeLoop::mediate` treats as fatal.

`crates/skein-connectors/tests/governed_git_run.rs` — the headline (SC-008, SC-009, and the
acceptance criterion that nothing between the model and git is a double):

- `a_model_asks_for_git_status_and_gets_the_real_repositorys_state_through_the_governed_gateway` —
  a real socket serving OpenAI chat-completions bytes, the real `OpenAiCompatClient`, the real
  `NativeLoop`, the real `ToolGateway` with a real `ToolPolicy`, the real `LocalConnector`, the real
  `EmbeddedServer`, and a real temporary repository with real commits. Asserts: the first request's
  `tools` array names `git_status`/`git_log` with the server's derived schemas; the fed-back message
  starts `[tool_result tool=git_status status=ok]` and carries the modified file's porcelain line;
  the full `IterationBoundary…Exit` step sequence; and `verify_chain` passes.
- `a_model_asks_for_git_log_and_gets_the_real_commit_summaries`
- `a_secret_in_a_commit_message_is_scrubbed_from_every_payload_of_the_run` — the secret is committed
  into a real commit message, configured on the `Redactor`, and must appear in no Ledger payload
  while at least one payload contains `***`. As in slice 016's T7, if this is green on arrival it is
  labelled a composition guard, and its teeth are demonstrated by removing the secret from the
  run's configuration and recording the resulting failure.
- `a_crafted_count_is_refused_as_a_tool_error_and_the_run_survives` — the model asks for `git_log`
  with `{"count": "5 --upload-pack=touch pwned"}`; the result arrives `status=ok` with
  `"isError":true` (fact 5), the run reaches `Exit`, and the chain verifies.
- `a_live_model_calls_a_real_git_tool` — `#[ignore]`d, `SKEIN_LIVE_MODEL`-gated.

`crates/skein-cli/tests/cli_chat.rs` — appended (SC-010):

- `chat_with_an_fs_root_that_is_a_git_repository_advertises_the_git_tools_and_reports_real_status` —
  the shipped binary; asserts the five advertised names, the porcelain line reaching the model, the
  twelve-kind `ledger log`, and `ledger verify` reporting `ok`.

`crates/skein-cli/tests/cli_acp_agent.rs` — appended (SC-011):

- `acp_agent_over_a_git_repository_advertises_the_git_tools_too` — proves `agent_policy` gained them,
  which nothing else can: `skein-cli` has no `lib` target, so its policies are only observable
  through the binary.

### Success criteria for `spec.md`

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
  repository, `git_status`'s advertised schema has empty `properties`, and a disabled git tool is
  not callable by name.
- **SC-008** A secret configured on the `Redactor` and committed into a real commit message appears
  in **no** Ledger payload of a governed run, and at least one payload contains `***`.
- **SC-009** The only model-supplied value in the slice is a `u32`; a non-numeric `count` is refused
  at the typed boundary, reaches the model as `isError: true`, and the run survives. No subprocess
  is spawned and no argument vector is constructed anywhere in the slice.
- **SC-010** `skein chat --fs-root <a real repository>` (the **real binary**) advertises the five
  tools and reports the repository's real status; `ledger verify` passes.
- **SC-011** `skein acp-agent --fs-root <a real repository>` advertises the five tools.
- **SC-012** Every pre-existing test passes with **no assertion changed or removed**. The only edits
  to pre-existing test files are import lines touched by D3's rename. In particular
  `connector.rs`'s three-tool catalogue test, `cli_chat.rs`'s `["fs_read","fs_list"]` assertion,
  `cli_chat.rs`'s no-`tools`-key control and `cli_acp_agent.rs`'s
  `["fs_read","fs_list","fs_write"]` assertion all keep their bodies, because each uses a
  non-repository root (fact 15).
- **SC-013** `git diff dev -- crates/skein-silo/ spikes/ .github/ rust-toolchain.toml` is empty.

### Hand-verification (T13, after implementation)

Against a live local Ollama, the way `skein chat`, `skein acp-agent` and `skein chat --fs-root` were
verified in the prior slices, with the transcript recorded under `## Live verification`:

1. `skein chat --fs-root <a real git repository> --model <a tool-capable model> --prompt "what has
   changed in this repository, and what were the last three commits?"` — a prompt that needs both
   tools.
2. `skein ledger log --run <id>` to see the `tool_call`/`approval`/`tool_result` triples, then
   `skein ledger show <step_id>` on each `tool_result` to read the porcelain and the log lines as
   they landed on the chain, then `skein ledger verify --run <id>`.
3. A second run with `--redact keychain://…` for a value planted in a commit message, confirming
   `***` in the `tool_result` payload.
4. The repeatable form: `$env:SKEIN_LIVE_MODEL = "<model>"; cargo test -p skein-connectors --test
   governed_git_run -- --ignored --nocapture`.

---

## Risks and rollback

**Blast radius.** One new module (`crates/skein-connectors/src/git.rs`), four new `#[tool]`-adjacent
lines in `src/server.rs`, a rename across six files, and one helper plus two reworded doc comments in
`crates/skein-cli`. `skein-core`, `skein-silo`, `skein-gateway`, `skein-acp`, `skein-mcp`, `spikes/`,
`.github/` and `rust-toolchain.toml` are untouched. Absent `--fs-root`, and absent a repository at
that root, behaviour is byte-identical to `dev`.

| Risk | Mitigation |
|---|---|
| **libgit2 fails to build on a CI leg** (macOS/Linux legs are unobserved until this repository has a remote — the standing caveat of slices 004–016, and this is the first slice to add a C dependency). | `vendored-libgit2` pins one libgit2 for all three OSes and needs no system library. A C toolchain is already required by `rusqlite`'s bundled SQLite (fact 11), so no new prerequisite appears. T2 is a separate, early step precisely so a build failure lands before any behaviour is written. Fallback if a leg still fails: switch the D1 decision to `gix` — nothing outside `git.rs` names `git2`, which is why the module boundary is drawn there. |
| **The `core.worktree` escape (fact 9) is missed**, and the connector reports on files outside the root. | It is the single most likely defect in the slice and it has its own test, built from the measured reproduction. The check compares two canonicalized paths, so it holds on Windows verbatim paths. |
| **`Repository::discover` gets used later** (by a helpful refactor) and silently reopens the upward-walk escape. | The containment test asserts `discover`'s contrasting behaviour explicitly, so the escape becomes a failing test rather than a review miss. |
| **An allowlisted-but-disabled git tool kills a run** (fact 4). | The two-layer gate (D4), and a test that asserts the transport error exists so the reason for the CLI layer is recorded rather than remembered. |
| **A pre-existing advertisement assertion breaks**, meaning the gate is wrong. | SC-012 makes it a stop condition, not a thing to update. |
| **A live model will not call a zero-argument tool.** | Not a code defect and not stubbable; it is a T13 finding, recorded the way slice 016 recorded model-selection findings. If it recurs, the follow-up is a documented `spec.md` note, not an invented dummy parameter. |
| **A huge worktree makes `git_status` slow** (the entry cap bounds the *output*, not the walk). | `recurse_untracked_dirs(false)` and `include_ignored(false)` bound the walk the way `git status --porcelain` bounds its own. Accepted and stated; a timeout belongs to a slice that has timeout machinery. |
| **libgit2's owner-validation** refuses a repository owned by another user. | Surfaces as a tool-level refusal the model is told, like any other. Recorded as a residual; `git2::opts::set_verify_owner_validation` is deliberately not called — silencing a safety check to make a tool convenient is the wrong trade. |
| **`chrono`'s `std`-only feature set** turns out not to expose the formatting path. | Verified compiling and running this session (fact 14). |

**Residuals, recorded rather than hidden** (`FsRoot`'s docstring idiom): the connector trusts the
system and global git configuration the way `git` itself does, so a global `core.excludesFile`
affects what `git_status` reports; `.git/` remains readable through `fs_read` exactly as on `dev`,
which this slice neither widens nor narrows; and slice 016's TOCTOU window between `canonicalize`
and open is unchanged and still open.

**Rollback.** The branch is not merged until the gates pass, so rollback is deleting the branch. Post
merge, `git revert` of the slice's commits restores `dev` exactly: the only cross-crate coupling is
D3's rename and the manifest edits, both of which revert mechanically. No data format, no `StepKind`,
no ledger schema and no CLI flag changes, so no chain written by this slice becomes unreadable.

---

## Out of scope

Deliberately not done, so nobody helpfully does it:

- **`git diff`, `git blame`, `git show`, branch/tag/remote listing, `git stash list`.** Two tools,
  per the request and Principle VII. `diff` in particular is an unbounded-output tool and would need
  its own cap design.
- **Any git operation that mutates repository state** — `add`, `commit`, `checkout`, `reset`,
  `restore`, `stash`. Not classified `Mutating`-and-deferred: **not built**. This slice is read-only
  in scope, not read-only by policy choice, so there is no code path to misclassify.
- **Any operation that could reach a remote** — `fetch`, `pull`, `push`, `clone`, `ls-remote`,
  `submodule update`. Constitution II is NON-NEGOTIABLE. Beyond not calling them, `git2` is compiled
  with `default = []`, so no HTTPS or SSH transport is linked in at all (fact 10) — the same
  build-property guarantee `skein-gateway` makes. This repository has no remote, so its own tests
  could not have caught a remote call by accident; the guarantee is therefore made at the code and
  build level rather than left to the fixture.
- **An arbitrary `git` subcommand passthrough tool.** Shell execution in git clothing. Not built,
  and D1's fact (8) is why a fixed argv would not have made it safe either.
- **Any subprocess at all.** No `Command`, no `std::process`, in product code.
- **A `shell` connector.** Still deferred, and slice 016's reasoning stands unamended: shell's blast
  radius is bounded by nothing this tree has, and an allowlist of command *names* is not an
  allowlist of *effects*. D1's rejection of a `git` subprocess is a narrower judgment about one
  fixed argv and does **not** reopen that deferral — if anything fact (8) strengthens it.
- **A `--git-root` flag, or any second root.** D4.
- **Design §5.4/§5.5's connector configuration hierarchy**, the scope-owner resolver and
  `AccessScope::{Project,Folder,FullComputer}` — still absent from `crates/`, verified again this
  session. One operator-named root; nothing hierarchical.
- **Rename detection in `git_status`**, submodule status recursion, `-uall`-style untracked
  recursion, ignored-file reporting, pathspec filtering.
- **Deriving `ToolAccess` from MCP tool annotations.** Classification stays operator configuration.
- **The ACP permission gate exercised end to end**, the TOCTOU residual, `role: "tool"` /
  `tool_call_id` replay, raw wire-byte capture, streaming (SSE), provider authentication, a config
  file, `--json` output, and the slices-008-vs-014 `serde_json/preserve_order` reconciliation — all
  carried unchanged from slice 016's `## Next slice`.
- **`crates/skein-silo/`, `spikes/`** (ADR-0004 D2), **`.github/`, `rust-toolchain.toml`.**
