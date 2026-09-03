# Implementation Plan: slice 020 — an operator-configured `--run-dir` allowlist for `proc_run`

**Spec dir to create:** `specs/020-run-dir-allowlist/{spec.md,plan.md,tasks.md}` ·
**Branch:** `020-run-dir-allowlist`, cut from `dev` at `09d61f8` · **No PR** (this repository has no
remote) · Conventional Commits.

Everything named below was read in the working tree at `09d61f8`, or measured on this machine, **this
session**. Anything marked **new** does not exist yet. Where this plan contradicts the request's own
assumptions, it says so and gives the measurement.

`git status --short` in `D:\claudecode\skein` was empty when this plan was written and is empty now.
Nothing in the repository was modified; the only writes this session were to a scratch directory under
this run's artifacts folder, since removed.

---

## Problem

Slice 019 shipped `proc_run`: a Windows-only sandboxed process launcher (AppContainer + Job Object)
behind two opt-ins (`--fs-root` *and* `--allow-run`) and the per-call ACP permission gate. It works —
its containment properties are proven hermetically in `crates/skein-sandbox/tests/escape.rs`, and its
governed chain was verified live against a real Ollama model
(`specs/019-shell-connector-windows/tasks.md`, `## Live verification`).

It also cannot run anything an agent actually wants to run. `specs/019-shell-connector-windows/tasks.md`
names the consequence itself under `## Next slice`: *"cargo, node and python are unreachable, so
`proc_run` cannot build this project. That is the single largest gap between what this slice ships and
what an agent needs."*

An agent that can read files, inspect git history and launch `cmd.exe` but cannot invoke the project's
own build tool, linter or test runner is missing the capability that turns *observe* into *verify*.
This slice closes that gap in the smallest shape that keeps every property slice 019 proved.

---

## What was verified before planning

### The tree as it is

Read this session, at `09d61f8`:

1. **`crates/skein-connectors/src/run.rs` — `resolve_exe(root: &FsRoot, command: &str)`.** The request
   located this in `crates/skein-sandbox/src/launch.rs`; it is not there. `launch.rs` takes an
   already-resolved absolute `exe: &Path`. Resolution lives in `skein-connectors`, and that boundary is
   deliberate — `run.rs`'s module doc says *"what this module decides is which executable a `command`
   string may name at all"*. The rule today:
   - `command` containing `/` or `\` → `root.resolve(command)` (an existing path inside the fs-root);
   - otherwise append `.exe` when absent, then look in `%SystemRoot%\System32`, then `%SystemRoot%`;
   - otherwise `Err`, naming both directories and saying `%PATH%` is deliberately not searched.
2. **`crates/skein-sandbox/src/profile.rs` — `create(root)` and `unsafe fn grant(root, sid)`.** The
   grant is `GetNamedSecurityInfoW` → one `EXPLICIT_ACCESS_W { grfAccessPermissions: GENERIC_ALL.0,
   grfAccessMode: GRANT_ACCESS, grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT, Trustee:
   TRUSTEE_IS_SID }` → `SetEntriesInAclW` → `SetNamedSecurityInfoW`. It is written against a single
   `dir: &Path` and takes the SID as a parameter, so it generalizes to N directories by changing
   nothing but the mask and adding a loop. **The request's assumption that D7's mechanism generalizes
   cleanly is confirmed.**
3. **`crates/skein-sandbox/src/lib.rs`** — `Sandbox { root: PathBuf, sid: String }`,
   `Sandbox::create(&Path)`, `Sandbox::string_sid()`, `Sandbox::run(exe, args, cap, timeout)`;
   uninhabited off Windows (`Sandbox(std::convert::Infallible)`), where `create` is a loud refusal.
4. **`crates/skein-sandbox/src/launch.rs` — `environment_block()`.** Five variables, sorted
   case-insensitively: `LOCALAPPDATA`, `PATH`, `PATHEXT`, `SystemRoot`, `windir`. Its doc states the
   invariant this slice must keep true: *"`PATH` is the same two directories the caller's own
   executable resolution searches"*. `LOCALAPPDATA` is load-bearing (its absence fails the launch with
   `ERROR_ENVVAR_NOT_FOUND`); the child's cwd is `sandbox.root`.
5. **`crates/skein-connectors/src/server.rs`** — `pub enum RunAccess { Denied, Allowed }`
   (`Debug, Clone, Copy, PartialEq, Eq`), `EmbeddedServer { root: Arc<FsRoot>, sandbox:
   Option<Arc<Sandbox>>, tool_router }`, `EmbeddedServer::new`, `EmbeddedServer::with_run(root, run)`,
   the private `build(root, sandbox)` that calls `disable_route("proc_run")` when the sandbox is
   `None`, and `proc_run`'s `#[tool(description = "…")]` **static** string.
6. **`crates/skein-connectors/src/connector.rs`** — `local_connector_with_run(root, run)`.
7. **`crates/skein-cli/src/wiring.rs`** — `ToolArgs { fs_root: Option<PathBuf> }` (**no environment
   fallback**), `RunArgs { allow_run: bool }` with `RunArgs::resolve() -> Result<RunAccess>`,
   `ToolArgs::transport(run)`, `ToolArgs::agent_policy(run)` (compares `run == RunAccess::Allowed`),
   `ToolArgs::chat_policy()`. `RunArgs` is flattened into the `AcpAgent` variant only
   (`crates/skein-cli/src/main.rs`).
8. **`crates/skein-cli/src/acp.rs` — `serve`.** `run.resolve()?` is called before `Silo::open`, then
   `run` is used **twice per session** inside the `move` factory closure: `tools.transport(run)?` and
   `tools.agent_policy(run)`. Today that works because `RunAccess` is `Copy`.
9. **Environment conventions.** `$SKEIN_ROOT` backs `--root` (silo) and `$SKEIN_MODEL_BASE_URL` backs
   `--base-url`. **No path-scoped tool-configuration flag has an environment fallback** — `--fs-root`
   has none. The request asked which convention fits; the code's answer is *flag only*.
10. **Call sites that will move.** `Sandbox::create` in `crates/skein-sandbox/tests/{profile.rs,
    launch.rs,escape.rs}`; `RunAccess::{Allowed,Denied}` in `crates/skein-connectors/tests/{connector.rs,
    run_server.rs,governed_proc_run.rs}`; `--allow-run` in `crates/skein-cli/tests/cli_acp_agent.rs`
    (the help-text test `acp_agent_documents_the_allow_run_flag_and_chat_does_not`, and the
    allow/reject governed pair).
11. **`RmcpToolTransport::list` (`crates/skein-mcp/src/lib.rs`, `fn spec_of(tool: &Tool)`)** maps only
    name / description / parameters into `ToolSpec`. **Server `instructions` never reach the model.**
    That kills the cheapest idea for telling a model which directories are allowlisted.
12. **rmcp 2.2.0** (`~/.cargo/registry/.../rmcp-2.2.0/src/handler/server/router/tool.rs`):
    `pub struct ToolRouter<S> { pub map: HashMap<Cow<str>, ToolRoute<S>>, … }` and
    `pub struct ToolRoute<S> { pub call: …, pub attr: crate::model::Tool }`. Both are
    `#[non_exhaustive]`, which forbids *construction* outside the crate but not field access — so an
    already-registered route's `attr.description` is mutable in place.

### Windows ACL semantics, measured rather than assumed

The request asked for the narrowest mask that lets an AppContainer actually execute a binary from a
directory it otherwise has no access to, *verified*. Measured on this machine (Windows 11 Pro
10.0.26200) with `Get-Acl`:

13. **`%SystemRoot%\System32` and `C:\Program Files` each carry two ACEs for `ALL APPLICATION PACKAGES`
    (S-1-15-2-1):** an effective one rendered `ReadAndExecute, Synchronize` (`0x1200A9` =
    `FILE_GENERIC_READ | FILE_GENERIC_EXECUTE`), and an inherit-only one whose raw mask is
    `-1610612736` = `0xA0000000` = `GENERIC_READ | GENERIC_EXECUTE`. **This is Windows' own answer to
    the question**, on the two directories every AppContainer on the machine already executes from —
    including in slice 019's green tests.
14. **A generic mask written on a directory with `CONTAINER_INHERIT | OBJECT_INHERIT` splits into two
    ACEs.** Measured directly (a scratch directory under this run's artifacts folder, an ACE inserted
    with a raw mask, written back, re-read):
    - wrote `0x10000000` (`GENERIC_ALL`) → read back `0x1F01FF` flags `None`, **plus** `0x10000000`
      flags `ObjectInherit, ContainerInherit, InheritOnly`;
    - wrote `0xA0000000` (`GENERIC_READ|GENERIC_EXECUTE`) → read back `0x1200A9` flags `None`, **plus**
      `0xA0000000` flags `ObjectInherit, ContainerInherit, InheritOnly`.
    So an ACL read-back test cannot simply compare against the constant that was written: it must
    normalise. `MapGenericMask` with the file generic mapping does exactly that, and is a no-op on an
    already-specific mask.
15. **`C:\Program Files\nodejs` does *not* inherit `C:\Program Files`' AppContainer ACEs** — its DACL is
    protected and names only `Authenticated Users`, `SYSTEM`, `Administrators`, `Users`. Its owner is
    `NT AUTHORITY\SYSTEM`. So node is genuinely unreachable to an AppContainer today, **and** granting
    it needs `WRITE_DAC`, which a non-elevated skein does not have. A real failure mode a live
    verification would otherwise walk into; see D10.
16. **`D:\Users\cthedrez\.cargo\bin` and
    `D:\Users\cthedrez\.rustup\toolchains\1.97-x86_64-pc-windows-msvc\bin` are owned by the user with
    `FullControl` and carry no AppContainer ACE.** A non-elevated grant on either will succeed.

### What is actually installed here, and whether it survives the sandbox's environment

17. `where.exe` finds `cargo` at `D:\Users\cthedrez\.cargo\bin\cargo.exe` (a rustup shim), `node` at
    `C:\Program Files\nodejs\node.exe`, `git` at `C:\Program Files\Git\cmd\git.exe`, python under
    `%LOCALAPPDATA%`. Toolchains present: `1.79`, `1.97`, `stable` (msvc).
18. **All three of `node --version`, `~\.cargo\bin\cargo --version` (the shim) and
    `~\.rustup\toolchains\1.97-…\bin\cargo --version` (the real binary) exit 0 under the sandbox's exact
    five-variable environment block** — measured with a cleared `ProcessStartInfo.Environment` populated
    with exactly `LOCALAPPDATA`, `PATH=%SystemRoot%\System32;%SystemRoot%`, `PATHEXT`, `SystemRoot`,
    `windir`. Outputs: `v24.14.0`, `cargo 1.97.1 (c980f4866 2026-06-30)`, same. So no `USERPROFILE`,
    `CARGO_HOME` or `RUSTUP_HOME` needs to be added, and this slice adds none.
19. `…\.rustup\toolchains\1.97-…\bin` contains `cargo.exe`, `rustc.exe`, `rustfmt.exe`,
    `clippy-driver.exe` **and** the `rustc_driver-*.dll` / `std-*.dll` they load — all in that one
    directory, so one inheritable ACE covers the binary and its DLLs.
20. **Caveat, from reading rather than running:** the `~\.cargo\bin\cargo.exe` *shim* re-executes the
    real cargo under `~\.rustup\toolchains\<t>\bin`. Under the AppContainer, naming only `~\.cargo\bin`
    gets a successful launch followed by a failure to exec the toolchain binary. `--run-dir` is
    repeatable precisely so the operator can name both; naming the toolchain `bin` directory alone is
    the simpler answer. Recorded as a residual, not fixed here.

---

## Approach

One sentence: **the sandbox becomes the single owner of a short, operator-named list of directories; it
grants each of them read+execute (never write), it puts them on the child's `PATH`, and executable
resolution searches exactly the directories that were granted.**

The list is threaded once, from the CLI flag to `Sandbox`, and read back out of `Sandbox` by everything
that needs it. Nothing else stores a copy, so resolution and grant cannot disagree.

### D1 — The flag is `--run-dir <PATH>`, repeatable, on `skein acp-agent` only, with no environment fallback

Added to `RunArgs` in `crates/skein-cli/src/wiring.rs`, beside `--allow-run`:

```rust
/// A directory whose executables `proc_run` may resolve by bare name and run.
/// Repeatable. Needs --allow-run. Grants this run's AppContainer identity a
/// read-and-execute entry on that directory — narrower than --fs-root's, and
/// still a real and lasting change to the directory's permissions.
#[arg(long = "run-dir", value_name = "PATH", requires = "allow_run")]
pub run_dir: Vec<PathBuf>,
```

Naming: `--run-dir` matches the request and reads as *"a directory for run"*, next to `--fs-root` (*a
root for fs*) and `--allow-run`. It is flattened where `--allow-run` is flattened and nowhere else, so
`skein chat` is untouched — `chat_policy`'s existing docstring already records why a mutating tool that
could only ever be denied belongs absent from a non-interactive command.

`requires = "allow_run"` makes `--run-dir` without `--allow-run` a clap usage error. `RunArgs::resolve`
keeps a belt-and-braces check for the same case, with a message naming both flags: a second reader of
the flag (a future subcommand) must not be able to lose the gate silently.

**Rejected — an environment fallback (`$SKEIN_RUN_DIRS`).** The request asked which convention fits.
Verified fact 9: `$SKEIN_ROOT` and `$SKEIN_MODEL_BASE_URL` exist, but **no** path-scoped
tool-configuration flag has one — `--fs-root`, the flag this most resembles, has none. Adding one here
would invent a convention rather than follow one, and a `;`-separated directory list in an environment
variable is precisely the shape slice 019's D8 rejected on decidability grounds.

**Rejected — `--allow-cargo` / `--allow-node` convenience flags.** Principle VII, and slice 019's
`tasks.md` already lists a per-tool flag among the things it rejected with a reason.

**Rejected — a config file.** v0 has none (`SiloArgs::root`'s docstring says so explicitly).

### D2 — The allowlist rides inside `RunAccess::Allowed`

```rust
pub enum RunAccess {
    Denied,
    Allowed(RunDirs),
}
```
`Clone + Debug + PartialEq + Eq`; **not `Copy` any more**.

This makes "run directories without run access" unrepresentable, which is the shape this codebase
already prefers for a capability gate (`RunAccess`'s own docstring: *"deny-by-default is structural here
rather than merely policy"*). It also keeps `EmbeddedServer::with_run(root, run)` and
`local_connector_with_run(root, run)` at their present arity — the change is where the value is built,
not how far it is threaded.

The cost, stated: `RunAccess` loses `Copy`, so `crates/skein-cli/src/acp.rs`'s session factory clones it
per session (`tools.transport(run.clone())?`), and `ToolArgs::agent_policy` takes `&RunAccess` and
switches its `run == RunAccess::Allowed` comparison to `matches!(run, RunAccess::Allowed(_))`.

**Rejected — a third parameter, `with_run(root, run, dirs)`.** Two values that must agree, with nothing
in the type system saying so; a `Denied` plus a non-empty list would have to be either silently ignored
or loudly refused at a layer that should not be deciding it.

### D3 — `RunDirs` is a validated newtype in `crates/skein-connectors/src/fs.rs`, beside `FsRoot`

```rust
pub struct RunDirs(Vec<PathBuf>);
impl RunDirs {
    pub fn none() -> RunDirs;
    pub fn new(paths: &[PathBuf]) -> skein_core::Result<RunDirs>;  // canonicalize each, refuse a non-directory
    pub fn paths(&self) -> &[PathBuf];
}
```

`FsRoot::new`'s shape and its recorded reason: *"an operator who mistyped `--fs-root` wants to hear
about it before a model does"*. Canonicalizing here means every later comparison and every Win32 call
sees one spelling, exactly as `FsRoot` does; `win32_path` in `skein-sandbox` already strips the
resulting `\\?\` prefix for the name-based ADVAPI32 calls and for `lpCurrentDirectory`.

It lives in `fs.rs` rather than `run.rs` because `run.rs` is `#[cfg(windows)]` and this type must exist
on all three platforms — `RunAccess` already does, for the reason its docstring gives (*"unconditional
on every OS so no caller needs a `#[cfg]` around a call site"*). `fs.rs` is where operator-named
directories are validated; the rule that *uses* them stays in `run.rs`, cfg-gated. Re-exported from
`crates/skein-connectors/src/lib.rs` beside `RunAccess`.

Duplicates are removed on construction (after canonicalization) so a doubled flag does not double an ACL
write or a `PATH` entry. Order is otherwise preserved: D5 makes it observable.

### D4 — The grant is `GENERIC_READ | GENERIC_EXECUTE`, and that is Windows' own answer

`crates/skein-sandbox/src/profile.rs`'s `grant` gains a mask parameter:

```rust
unsafe fn grant(dir: &Path, sid: PSID, access: u32) -> Result<(), String>
```

- the fs-root keeps `GENERIC_ALL` — unchanged, so no containment claim slice 019 made moves;
- each run directory gets `GENERIC_READ | GENERIC_EXECUTE`
  (`windows::Win32::Foundation::{GENERIC_READ, GENERIC_EXECUTE}`), same `GRANT_ACCESS`, same
  `SUB_CONTAINERS_AND_OBJECTS_INHERIT`.

**Why exactly this mask, verified (facts 13–14):** `%SystemRoot%\System32` — the directory every
AppContainer on this machine already executes from — carries `ALL APPLICATION PACKAGES` at effective
`0x1200A9` (`FILE_GENERIC_READ | FILE_GENERIC_EXECUTE`) plus an inherit-only `0xA0000000`
(`GENERIC_READ | GENERIC_EXECUTE`). Writing `GENERIC_READ | GENERIC_EXECUTE` with
`SUB_CONTAINERS_AND_OBJECTS_INHERIT` reproduces that pair exactly (measured in fact 14). There is no
guessing left in this decision: the mask is copied from what the operating system itself does to make a
directory executable by an AppContainer.

**Why not narrower — `FILE_GENERIC_EXECUTE` alone.** The image loader must *read* the PE file;
`FILE_GENERIC_EXECUTE` carries `FILE_READ_ATTRIBUTES` and `FILE_EXECUTE` but not `FILE_READ_DATA`.
System32's own ACE is read **and** execute, which settles it.

**Why not `GENERIC_ALL`.** A toolchain directory does not need to be writable by the sandboxed child,
the request's invariant is explicit, and `FILE_ALL_ACCESS` would let a child overwrite `cargo.exe`
itself — a side effect that outlives the run.

**Ancestors need nothing.** Slice 019 measured (`tasks.md`, T4 observed red) that an AppContainer token
retains `SeChangeNotifyPrivilege`, so one inheritable ACE on the directory is enough and no traverse ACE
is needed above it. Both measured run-dir candidates (fact 16) sit several levels deep under
`D:\Users\…`, the same situation the `TempDir` fixtures already prove.

### D5 — Resolution order: System32, `%SystemRoot%`, then each `--run-dir` in operator order

`resolve_exe` gains the granted list and appends it to the existing search:

```rust
pub(crate) fn resolve_exe(root: &FsRoot, run_dirs: &[PathBuf], command: &str) -> Result<PathBuf, String>
```

First hit wins. The list is **appended, not prepended**, so this change is strictly additive: every
`command` that resolved before this slice resolves to the same file after it. That is what keeps slice
019's assertions — including `an_absolute_command_is_refused_even_when_it_exists`, which pins
`C:\Windows\System32\cmd.exe`'s behaviour — green without an edit.

**Rejected — run directories first, so the operator's toolchain wins.** It would let a named directory
silently shadow `curl.exe`, `find.exe` or `cmd.exe` for every configuration that already exists, changing
what a working setup resolves to. An operator who genuinely wants to override a System32 name can put the
binary in the fs-root and name it as a path.

Ties between two run directories go to the first named. Stated in `spec.md` so it is a decision rather
than an accident of iteration order.

### D6 — A `command` containing a separator stays fs-root-only. This contradicts the request, deliberately

The request says the allowlist *"must still refuse a command containing `/` or `\` that resolves outside
every allowlisted directory and outside the fs-root"*, which reads as licence for a separator-bearing
`command` to resolve **inside** a run directory. This plan does not do that, and records the conflict.

A separator-form `command` is resolved relative to the fs-root (which is also the child's cwd). To let
one land in a run directory, `FsRoot::resolve` would have to stop meaning *"inside the root"* — the
single invariant `fs_read`, `fs_list`, `fs_write` and slice 016's whole test file rest on — or `run.rs`
would need a second, parallel traversal rule with its own `..` and symlink story. Both are a large amount
of new surface for a capability the bare-name rule already delivers: the operator named the directory, so
its binaries are reachable **by name**, which is how anyone actually invokes `cargo`, `node` or `rustfmt`.

The request's actual requirement — *"extend that message to name every place it looked, not just two"* —
is honoured in full by D7.

### D7 — The refusal names every place it looked

`resolve_exe`'s bare-name failure becomes, in the same voice as today's:

> `cargo.exe` is in none of `C:\WINDOWS\System32`, `C:\WINDOWS`,
> `D:\Users\…\.rustup\toolchains\1.97-x86_64-pc-windows-msvc\bin`; `%PATH%` is deliberately not searched,
> so name an executable in one of those directories or a path relative to the configured root.

With no `--run-dir` configured the list is the same two directories it is today, so
`a_command_that_resolves_nowhere_names_both_places_it_looked` keeps passing unedited. The separator-form
arm keeps `FsRoot::contained`'s existing *"resolves outside the root … and is refused"* message, which
`a_command_naming_a_path_outside_the_root_is_a_named_refusal` pins.

### D8 — The child's `PATH` gains the run directories

`environment_block` becomes `environment_block(run_dirs: &[PathBuf])` and renders
`PATH=%SystemRoot%\System32;%SystemRoot%;<run dir 1>;<run dir 2>…` (each through `win32_path`, so no
`\\?\` prefix reaches the child).

This preserves the invariant that function's doc already states — *"`PATH` is the same … directories the
caller's own executable resolution searches"* — rather than quietly breaking it. It widens nothing: every
directory on that `PATH` is one the operator named and one the AppContainer was just granted read+execute
on. It is also what lets a child that spawns a sibling (the rustup shim of fact 20, a linter invoking a
helper) find it.

The case-insensitive sort stays: `ERROR_ENVVAR_NOT_FOUND` is the failure mode for an unsorted block, and
it names nothing like its cause.

**No other environment variable is added.** Fact 18 measured that `cargo --version` and `node --version`
need none.

### D9 — The advertised `proc_run` description names the allowlisted directories

A model cannot tell a reachable `cargo` from an unreachable one, and the tool description is the only
channel that reaches it (fact 11: server `instructions` are dropped by `RmcpToolTransport::list`).

In `EmbeddedServer::build`, after the router is constructed and before the struct is returned:

```rust
#[cfg(windows)]
if let Some(dirs) = sandbox.as_ref().map(|s| s.run_dirs()).filter(|d| !d.is_empty()) {
    if let Some(route) = tool_router.map.get_mut("proc_run") {
        let base = route.attr.description.take().unwrap_or_default();
        route.attr.description =
            Some(format!("{base} A bare name is also looked for in: {list}.").into());
    }
}
```

`ToolRouter::map` and `ToolRoute::attr` are `pub` in rmcp 2.2.0 (fact 12); `#[non_exhaustive]` forbids
constructing one of these outside the crate, not mutating a field of one the macro already built. The
static `#[tool(description = …)]` string stays the single home of the rule, the caps and the "`PATH` is
not searched" sentence — the appended sentence only enumerates.

**Rejected — leave the description static and let the refusal teach.** It costs a wasted turn and an
`isError` round trip for the model to learn something the operator already decided at launch, and a model
that has been refused once tends to stop asking.

**Rejected — put the list in `get_info().instructions`.** Verified dead: `spec_of` in
`crates/skein-mcp/src/lib.rs` maps only name, description and parameters.

### D10 — The sandbox owns the list; a failed grant is an exit code

```rust
pub struct Sandbox { root: PathBuf, run_dirs: Vec<PathBuf>, sid: String }
pub fn create(root: &Path, run_dirs: &[PathBuf]) -> Result<Sandbox, String>   // both cfgs
#[cfg(windows)] pub fn run_dirs(&self) -> &[PathBuf]                          // new, public
```

`EmbeddedServer` gains **no field**: `build` and `proc_run` read `sandbox.run_dirs()`. That is the point —
the only searchable directories are the ones that were actually granted, by construction, with no second
copy to drift. `run_dirs()` is public for the reason `string_sid()` is: a test must be able to read what
was really configured rather than trust what the constructor claims.

`Sandbox::create` grants the root `GENERIC_ALL`, then each run directory `GENERIC_READ|GENERIC_EXECUTE`,
and **fails the whole construction** if any grant fails. `EmbeddedServer::with_run` is already fallible
for exactly this reason (*"a sandbox that cannot be built must be an exit code before a model is shown a
tool"*), and `RunArgs::resolve` is already called before `Silo::open` in `acp.rs`, so an operator who
names a directory they cannot re-permission meets a message at startup.

That message must be actionable, because fact 15 makes it reachable in practice: naming
`C:\Program Files\nodejs` from a non-elevated shell fails with `ERROR_ACCESS_DENIED`. It names the
directory, the Win32 error, and says the directory's DACL is not writable by this user — run an elevated
skein once, or name a directory you own.

**Rejected — skip the grant when the directory already carries an `ALL APPLICATION PACKAGES` ACE.** It
would make `C:\Program Files\…` work unelevated in *some* cases, at the cost of a second code path whose
correctness depends on parsing an existing DACL and reasoning about effective access — the exact kind of
inference this codebase refuses to make. `GRANT_ACCESS` is already idempotent for the directories the
operator can write; for the ones they cannot, a clear refusal beats a silent maybe.

---

## Steps

Each step is independently verifiable and anchored to named items, never line numbers. Strict TDD: the
red is observed and pasted into `tasks.md` under `## Observed red` before the green, following
`specs/019-shell-connector-windows/tasks.md`'s recorded discipline.

**T0 — Spec documents and branch.** Create `specs/020-run-dir-allowlist/{spec.md,plan.md,tasks.md}`
mirroring slice 019's format: `spec.md` with *What this slice changes for a user*, a numbered *things a
reader must know up front* list (facts 13–20 above are its raw material), `FR-`/`SC-` sections,
*Assumptions and residuals*, *Out of scope*; `tasks.md` with the **Constitution Check (ADR-0004 D1
solo-v0 bar)** block, carrying slice 019's `Cross-platform ⚠️` row **verbatim in substance** — this slice
remains Windows-only and inherits, does not amend, ADR-0006's scope. Branch `020-run-dir-allowlist` from
`dev` at `09d61f8`.

**T1 — Control baseline, re-measured not quoted.** `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`; record the pass count
per test target in `tasks.md`. Slice 019 closed above its own 193 baseline; the number here is whatever
`dev` at `09d61f8` actually reports, which is why it is measured rather than quoted.

**T2 — Types and signatures, no new behaviour.** `RunDirs` in `crates/skein-connectors/src/fs.rs` (real
body — it is plain path validation, and a `todo!()` here would make every later red ambiguous);
`RunAccess::Allowed(RunDirs)`; `Sandbox::create(root, run_dirs)` on both `cfg`s with the new field
threaded and `grant`'s new `access` parameter passed `GENERIC_ALL` for the root and **nothing else granted
yet**; `Sandbox::run_dirs()`; `resolve_exe(root, run_dirs, command)` **ignoring** its new argument. Update
every call site from fact 10: `Sandbox::create(dir.path(), &[])`, `RunAccess::Allowed(RunDirs::none())`,
`acp.rs`'s two uses of `run`, `ToolArgs::agent_policy(&RunAccess)`. **Gate: `cargo test --workspace` is
green and no existing assertion's *text* changed** — only constructor spelling. This is slice 019's FR-016
discipline applied to a refactor: if an assertion needs rewording here, the shape is wrong, and that is a
stop condition rather than a thing to patch.

**T3 — RED→GREEN: the narrower grant, read back off the directory.** Test in
`crates/skein-sandbox/tests/profile.rs`, beside
`a_sandbox_derives_an_appcontainer_sid_and_grants_it_the_root`. Add a `granted_masks(dir, sid) -> Vec<u32>`
helper next to the existing `granted_sids` — the same `GetNamedSecurityInfoW` / `GetAce` walk, returning
`(*allowed).Mask` for the matching SID, each mask normalised through `MapGenericMask` with the file
`GENERIC_MAPPING` (`GenericRead: FILE_GENERIC_READ`, `GenericWrite: FILE_GENERIC_WRITE`, `GenericExecute:
FILE_GENERIC_EXECUTE`, `GenericAll: FILE_ALL_ACCESS`). Fact 14 is why normalisation is not optional: an
inheritable generic ACE is stored as two ACEs, one mapped and one not. Then implement the run-dir grant in
`profile::create`.

**T4 — RED→GREEN: a binary in a named run directory actually executes.** Test in
`crates/skein-sandbox/tests/launch.rs`. Fixture: one `TempDir` root, a second `TempDir` "toolbin",
`std::fs::copy(system32("cmd.exe"), toolbin.join("toolchain.exe"))`. `Sandbox::create(root, &[toolbin])`,
then `sandbox.run(&toolbin.join("toolchain.exe"), &args(&["/c", "echo", MARKER]), …)`. Asserts exit 0 and
`MARKER` in real stdout. **This is slice 019's T4/V1 evidentiary bar**: a real ACE, a real
`CreateProcessW`, real captured bytes — no mocked resolution check. A `TempDir` carries no
`ALL APPLICATION PACKAGES` ACE (slice 019's `grant` docstring records this), so a pass is attributable to
the new grant and nothing else. Renaming the copy to `toolchain.exe` is deliberate: no such name exists in
System32, so T6's resolution tests cannot pass for the wrong reason.

**T5 — RED→GREEN: the run directory is not writable from inside the sandbox.** Test in
`crates/skein-sandbox/tests/escape.rs`, in that file's positive-control style. A sandboxed
`cmd.exe /c copy seed.txt <toolbin>\escaped.txt` leaves **no file** in the run directory and exits nonzero;
the control is the *same* copy into the fs-root, which **does** land. The absent file is the ground truth
that the narrower mask is narrower in effect, not merely in intent — this and T3's ACL read-back are two
independent proofs of one claim, which is the discipline `escape.rs` already documents.

**T6 — RED→GREEN: resolution, ordering and the refusal.** Tests in
`crates/skein-connectors/tests/run_server.rs`, extending its `Fixture`/`run` helpers with a run directory:
- a bare `toolchain` (no extension) resolves inside the named run directory and reports `exit 0` with its
  real stdout;
- the same bare name with the directory **not** named is refused, and the refusal names System32,
  `%SystemRoot%`, every directory that *was* named, and still says `%PATH%` is not searched;
- System32 wins a shadowing tie: a **non-executable** file called `cmd.exe` placed in the run directory,
  and `cmd.exe /c type seed.txt` still succeeds — which can only happen if System32 was searched first.
Then implement D5 and D7 in `resolve_exe`, and D8 in `environment_block`.

**T7 — RED→GREEN: the advertisement.** Test in `crates/skein-connectors/tests/connector.rs` beside
`the_connector_lists_proc_run_with_its_caps_stated_when_run_access_is_allowed`: with a run directory
configured, `proc_run`'s advertised description contains that directory's path; with none, the description
still contains the caps and "PATH is not searched" and gains nothing. Update the `#[cfg(not(windows))]`
test's constructor spelling only. Then implement D9 in `build`.

**T8 — RED→GREEN: the CLI.** Tests in `crates/skein-cli/tests/cli_acp_agent.rs`, in the style of
`acp_agent_documents_the_allow_run_flag_and_chat_does_not`:
- `skein acp-agent --help` documents `--run-dir`; `skein chat --help` does not;
- `--run-dir <dir>` without `--allow-run` exits nonzero with a message naming **both** flags;
- `--run-dir <path that is not a directory>` exits nonzero naming the path.
Then implement D1 in `wiring.rs` (`RunArgs::run_dir`, `RunArgs::resolve` building
`RunAccess::Allowed(RunDirs::new(&self.run_dir)?)`), and thread the clone through `acp.rs`.

**T9 — the live test, gates and close-out.** Extend
`crates/skein-connectors/tests/governed_proc_run.rs` with a second `#[ignore]`d live test that configures a
run directory from `$SKEIN_LIVE_RUN_DIR` and prompts for a real toolchain binary (below). Add `skein-silo`
to `crates/skein-connectors/Cargo.toml`'s `[dev-dependencies]` so that test can persist to a real silo when
`$SKEIN_LIVE_SILO_ROOT` is set — `Silo::open(root, "live020")?.ledger()?` returns the same
`skein_core::Ledger` the in-memory path uses, verified in `crates/skein-silo/src/lib.rs` — and fall back to
`Ledger::new()` otherwise, keeping `cargo test --workspace` hermetic and offline. Then re-run the three
gates and the control diff.

**T10 — hand-verification against live Ollama, recorded in `tasks.md`.** See *Validation* below.

---

## Validation

### Project gates (unchanged; all three must pass)

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace` — the T1 count plus exactly the new tests, no pre-existing assertion text
  changed.
- Control diff empty for `crates/skein-silo/`, `crates/skein-core/`, `crates/skein-gateway/`,
  `crates/skein-mcp/`, `spikes/`, `.github/`, `rust-toolchain.toml`. **One stated exception:**
  `crates/skein-connectors/Cargo.toml` gains a single `[dev-dependencies]` entry (`skein-silo`) for T9's
  optional persistence. No product dependency is added anywhere; `skein-sandbox`'s dependency set is
  untouched.

### New tests

| Test | File | What it proves |
|---|---|---|
| `a_run_dir_is_granted_read_and_execute_and_the_root_is_not` | `skein-sandbox/tests/profile.rs` | Real `GetNamedSecurityInfoW` read-back, masks normalised through `MapGenericMask`: the run dir's ACEs carry `FILE_READ_DATA` and `FILE_EXECUTE` and **not** `FILE_WRITE_DATA` / `FILE_APPEND_DATA` / `WRITE_DAC`; the fs-root's **do** carry `FILE_WRITE_DATA`. Narrower, measured against the object's own descriptor rather than asserted about intent. |
| `a_binary_in_an_allowlisted_run_dir_executes_and_its_stdout_comes_back` | `skein-sandbox/tests/launch.rs` | A real ACE grant + a real `CreateProcessW` + real captured stdout, on a directory carrying no `ALL APPLICATION PACKAGES` ACE. |
| `a_sandboxed_process_cannot_write_into_a_run_dir` | `skein-sandbox/tests/escape.rs` | Narrowness as an **effect**: the copy lands in the fs-root (control) and not in the run dir. |
| `a_bare_name_in_an_allowlisted_run_dir_resolves_and_runs` | `skein-connectors/tests/run_server.rs` | The resolution rule at the tool's own level. |
| `a_bare_name_in_a_directory_that_was_not_named_names_every_place_it_looked` | `skein-connectors/tests/run_server.rs` | The allowlist has not become a de facto `%PATH%`, and the refusal enumerates System32, `%SystemRoot%` and every named directory. |
| `system32_still_wins_over_a_run_dir_that_shadows_it` | `skein-connectors/tests/run_server.rs` | D5's append-not-prepend order, proven by a run that can only succeed if System32 was searched first. |
| `the_advertised_description_names_the_allowlisted_directories` | `skein-connectors/tests/connector.rs` | D9 — and, with no run dir, that the advertisement keeps everything slice 019 pinned. |
| `acp_agent_documents_the_run_dir_flag_and_chat_does_not` | `skein-cli/tests/cli_acp_agent.rs` | The flag exists on exactly one subcommand. |
| `run_dir_without_allow_run_is_an_exit_code_naming_both_flags` | `skein-cli/tests/cli_acp_agent.rs` | Deny-by-default at the operator boundary, against the real binary. |
| `a_run_dir_that_is_not_a_directory_is_a_loud_refusal` | `skein-connectors/tests/fs_root.rs` | `RunDirs::new`'s validation, in the file that already owns `FsRoot::new`'s. |
| `a_live_model_runs_a_real_toolchain_binary` (`#[ignore]`) | `skein-connectors/tests/governed_proc_run.rs` | Repeatable hand-verification, `governed_fs_run.rs`'s established pattern. |

### The live hand-verification (T10)

Run on this machine and pasted verbatim into `specs/020-run-dir-allowlist/tasks.md` under
`## Live verification`, mirroring slice 019's T13 section.

**Target: `D:\Users\cthedrez\.rustup\toolchains\1.97-x86_64-pc-windows-msvc\bin`, command
`cargo --version`.** Chosen on measurement, not preference:
- it is user-owned with `FullControl` (fact 16), so the ACL grant succeeds **without elevation**;
- it holds the real `cargo.exe` and `rustc.exe` **plus** the `rustc_driver-*.dll` and `std-*.dll` they load,
  all in the one directory one inheritable ACE covers (fact 19);
- `cargo --version` exits 0 under the sandbox's exact five-variable environment block (fact 18);
- `node --version` is the documented **fallback**: it also survives that environment, but
  `C:\Program Files\nodejs` is owned by SYSTEM and carries no AppContainer ACE (fact 15), so it needs an
  elevated skein to grant. If the operator prefers node, that is the cost — stated here rather than met as
  `ERROR_ACCESS_DENIED`;
- `~\.cargo\bin` (the rustup **shim**) is deliberately not the target — fact 20.

```powershell
$env:SKEIN_LIVE_MODEL     = "gemma4:latest"   # or whatever `ollama list` actually offers
$env:SKEIN_LIVE_RUN_DIR   = "D:\Users\cthedrez\.rustup\toolchains\1.97-x86_64-pc-windows-msvc\bin"
$env:SKEIN_LIVE_SILO_ROOT = "$env:TEMP\skein-live-020"
cargo test -p skein-connectors --test governed_proc_run -- --ignored --nocapture
skein ledger log  --root $env:SKEIN_LIVE_SILO_ROOT --silo live020
skein ledger show --root $env:SKEIN_LIVE_SILO_ROOT --silo live020 <the ToolResult step id>
```

Pass condition: the model chose `proc_run` with `{"command":"cargo","args":["--version"]}` (or
`cargo.exe`), the chain carries `ToolCall` → `Approval {decision: allowed}` → `ToolResult`, and that
`ToolResult` payload contains a real `cargo 1.97.…` line produced by a real `CreateProcessW` inside the
AppContainer. The `skein ledger show` output for that step is the evidence pasted into `tasks.md`.

If the model declines to call the tool, that is a model-selection finding and not a defect — slice 019's
live section says so and this one inherits the wording.

---

## Risks and rollback

**Blast radius.** `crates/skein-sandbox` (`lib.rs`, `profile.rs`, `launch.rs`), `crates/skein-connectors`
(`fs.rs`, `run.rs`, `server.rs`, `lib.rs`, `Cargo.toml` dev-deps), `crates/skein-cli` (`wiring.rs`,
`acp.rs`). `skein-core`, `skein-silo`, `skein-gateway`, `skein-mcp` and `spikes/` are untouched. No new
product dependency, and no new `unsafe` block outside `skein-sandbox` — the mask parameter reuses `grant`'s
existing one.

**Risk: the T2 refactor churns call sites and hides a behaviour change.** Mitigated by T2 being a
compile-and-green step with an explicit gate: only constructor spelling may change, never an assertion's
text. If an assertion needs rewording at T2, stop — the shape is wrong.

**Risk: `SetNamedSecurityInfoW` fails on the operator's directory.** Measured as reachable (fact 15).
Mitigated by D10: an exit code with an actionable message before a model is shown a tool, which is the
behaviour `EmbeddedServer::with_run` was already made fallible for.

**Risk: rmcp's `pub map` / `pub attr` change shape on upgrade.** `rmcp = "2.2"` is a workspace pin and both
fields are `pub` at 2.2.0 (fact 12). A future minor could reorganise them; T7's test is what would catch it
— a compile error at the exact line, not a silent loss of the advertisement.

**Risk: an operator names a directory so broad the allowlist becomes a `%PATH%`** (`C:\Windows`, a whole
drive). Not preventable in code, and not this slice's job to guess at: the flag is opt-in, per-directory
and operator-named, and `spec.md` states the cost plainly. Nothing is auto-discovered — no `%PATH%` scan,
no `%USERPROFILE%\.cargo\bin` default — which is the invariant slice 019's D8 established and this slice
does not reopen.

**Rollback.** `git revert` the slice's commits. Two side effects survive it, exactly as slice 019's already
do and for the same recorded reason: the AppContainer profile (deterministic per root, never deleted) and
now the read+execute ACEs on each named run directory. Both are removed by hand
(`icacls <dir> /remove:g *S-1-15-2-…`), and `spec.md`'s *Assumptions and residuals* must say so — slice 019
already carries the fs-root version of this residual, and this slice widens it from one directory to N.

---

## Out of scope

Deliberately not done, so nobody helpfully does it:

- **Any `%PATH%` search, or any auto-discovery.** No scanning `%PATH%`, no defaulting
  `%USERPROFILE%\.cargo\bin`, no reading `rustup` / `nvm` / `pyenv` state. Constitution II and VI; slice
  019's D8 settled this and this slice must not reopen it by a different route.
- **`.cmd` / `.bat` / `.ps1` shims.** `resolve_exe` appends `.exe` and `CreateProcessW` with an
  `lpApplicationName` cannot execute a batch file. So a project-local `node_modules\.bin` — which the
  request names as an example — is a **legal** `--run-dir` whose real `.exe` entries work and whose `.cmd`
  shims (`tsc.cmd`, `eslint.cmd`, `npm.cmd`) do **not**. `spec.md` states this plainly rather than letting
  an operator discover it; supporting a shim would mean building a command line for `cmd.exe /c`, which is
  shell syntax by another name.
- **Any new environment variable for the child** — no `CARGO_HOME`, `RUSTUP_HOME`, `USERPROFILE`, `TEMP`,
  and no `--env` flag. Fact 18 measured that the version-probe case needs none.
- **Making `cargo build` actually work.** This slice makes the toolchain *reachable and launchable*.
  Whether a full build succeeds inside an AppContainer with no network, no `TEMP` and one writable
  directory is a separate slice's finding, and `spec.md` must not imply otherwise — the same honesty slice
  019's point 5 applied to itself.
- **Profile and ACE cleanup** (`skein sandbox prune`, `DeleteAppContainerProfile`). A separately named
  residual in slice 019's `## Next slice`, unchanged.
- **Any Linux or macOS backend, or a cross-OS sandbox trait.** ADR-0006's named future work; this slice
  inherits slice 019's Windows-only scope and its `Cross-platform ⚠️` Constitution Check row verbatim in
  substance.
- **A second tool of any kind** — no `proc_which`, no `proc_kill`, no run-dir listing tool. Principle VII.
  The advertisement (D9) and the refusal (D7) already tell a model what it can reach.
- **`--allow-run` or `--run-dir` on `skein chat`.**
- **Touching the governed chain.** No new `StepKind`, no change to `ToolGateway`, `Approval`, `Redactor` or
  `AcpPermissionTransport`. Constitution V is satisfied by machinery that already covers every `proc_run`
  call; this slice changes only which executables `resolve_exe` accepts.
- **`spikes/`** (ADR-0004 D2), **`.github/`**, **`rust-toolchain.toml`**, and the residuals slices 018 and
  019 carried forward — the `canonicalize`-to-open TOCTOU fix, conversation replay, raw wire-byte capture,
  streaming, provider authentication, a config file, `--json` output.
