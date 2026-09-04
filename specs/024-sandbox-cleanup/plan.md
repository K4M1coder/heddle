# Implementation Plan: `skein sandbox list` / `skein sandbox prune` (v0 slice)

**Feature Branch:** `024-sandbox-cleanup` · **Spec:** `specs/024-sandbox-cleanup/spec.md` ·
**Tasks:** `specs/024-sandbox-cleanup/tasks.md`

Written as an advisory plan against `d364405` before any edit, and adopted unchanged as this
slice's plan. Two numbers in it were re-measured at implementation time and are recorded here
rather than silently corrected in the text below:

- The orphaned-profile count read **955** when the plan was written and **1009** at the
  implementation run's start, on the same machine. The plan's number is not wrong; it is a
  point-in-time reading of a quantity that grows. The mechanism is the same one the plan names, and
  T1 measured its rate directly: **+27 profiles per `cargo test --workspace`**.
- The `%LOCALAPPDATA%\Packages\skein-*` DACL readings in *The AppContainer folder layout, measured*
  were re-taken with `icacls` at implementation time and reproduce exactly: the package folder
  names `NT AUTHORITY\SYSTEM`, `BUILTIN\Administrators` and the user and **no** AppContainer SID;
  its `AC` subfolder names the unresolvable AppContainer SID at `(OI)(CI)(CR)(F)` plus
  `Mandatory Label\Low Mandatory Level:(NW)`. D1.3 and D3 rest on that difference and it holds.

Corrections and deviations discovered while executing this plan are recorded in `tasks.md` under
`## Observed red` and `## Deviations from the plan`, not by editing the plan retroactively.

---

## Problem

`Sandbox::create` (`crates/skein-sandbox/src/profile.rs`) does two things that outlive the process:

1. It creates a Windows **AppContainer profile** named `skein-` + the first 16 hex characters of
   `sha256(canonical root path)` (`profile_name`, `NAME_HASH_BYTES = 8`), via
   `CreateAppContainerProfile`.
2. It merges an **inheritable ACE** for that profile's SID into the DACL of the `--fs-root`
   (`GENERIC_ALL`) and of every `--run-dir` (`RUN_DIR_ACCESS = GENERIC_READ | GENERIC_EXECUTE`),
   via `GetNamedSecurityInfoW` → `SetEntriesInAclW(GRANT_ACCESS)` → `SetNamedSecurityInfoW`.

Nothing removes either. `specs/019-shell-connector-windows/tasks.md` `## Next slice` names this
plainly (*"Nothing removes either today, by design. A `skein sandbox prune` subcommand — or an
explicit decision that the operator owns it — is a real question this slice answers only with a
residual"*), `specs/019-…/plan.md`'s risk table records it, and
`specs/020-run-dir-allowlist/spec.md` *Assumptions and residuals* widens it from one directory to N
and confirms it survives `git revert`.

**Measured, not assumed.** On this machine right now:

```
$ ls -d "$LOCALAPPDATA/Packages"/skein-* | wc -l
955
```

955 orphaned `skein-<16 hex>` profile folders, each containing exactly one entry, `AC`. Most were
produced by the test suite itself — `crates/skein-sandbox/tests/{profile,launch,escape}.rs` and
`crates/skein-connectors/tests/connector.rs` each call `Sandbox::create` over a fresh `TempDir`, so
every `cargo test --workspace` on Windows mints a new batch. This is the strongest single argument
for the slice, and it appears in no spec.

---

## What was verified this session

Read in full: `crates/skein-sandbox/src/{profile.rs,lib.rs}`, `crates/skein-sandbox/tests/profile.rs`,
`crates/skein-cli/src/{main.rs,acp.rs,wiring.rs,secret.rs}`, `crates/skein-cli/Cargo.toml`,
`crates/skein-sandbox/Cargo.toml`, workspace `Cargo.toml`; targeted reads of
`crates/skein-cli/src/ledger.rs`, `crates/skein-cli/tests/cli_secret.rs`,
`crates/skein-sandbox/src/launch.rs`, `crates/skein-connectors/src/server.rs`; specs 019 and 020
(`spec.md`, `plan.md`, `tasks.md`); `.specify/memory/constitution.md` Additional Constraints.

### Windows APIs, re-verified against the vendored source that will actually compile

Source: `~/.cargo/registry/src/index.crates.io-*/windows-0.61.3/src/Windows/Win32/Security/…`.
The workspace pins `windows = "0.61"` with features `Win32_Foundation`, `Win32_Security`,
`Win32_Security_Isolation`, `Win32_Security_Authorization`, `Win32_System_Threading`,
`Win32_System_Pipes`. **This slice needs no new feature and no new dependency.**

- `Isolation/mod.rs:15` — `pub unsafe fn DeleteAppContainerProfile<P0>(pszappcontainername: P0) ->
  windows_core::Result<()>` where `P0: Param<PCWSTR>`, linked from `userenv.dll`. **Unchanged from
  what slice 019's plan recorded.**
- `Isolation/mod.rs:23` — `DeriveAppContainerSidFromAppContainerName<P0>(…) -> Result<PSID>`.
  Unchanged. Already used by `profile::create`'s `ERROR_ALREADY_EXISTS` arm. It *derives*; it does
  not create, so calling it during `list`/`prune` mints nothing.
- `Isolation/mod.rs:45` — `GetAppContainerFolderPath<P0>(pszappcontainersid: P0) -> Result<PWSTR>`.
  Unchanged. **Rejected for this slice** — see D3.
- **There is no enumeration API.** The complete public surface of `Win32::Security::Isolation` in
  0.61.3 is: `CreateAppContainerProfile`, `DeleteAppContainerProfile`,
  `DeriveAppContainerSidFromAppContainerName`,
  `DeriveRestrictedAppContainerSidFromAppContainerSidAndRestrictedName`,
  `GetAppContainerFolderPath`, `GetAppContainerNamedObjectPath`, `GetAppContainerRegistryLocation`,
  `IsCrossIsolatedEnvironmentClipboardContent`, `IsProcessInIsolatedContainer`,
  `IsProcessInIsolatedWindowsEnvironment`, `IsProcessInWDAGContainer`. Nothing lists profiles. This
  settles the request's open research question — see D2.
- `Authorization/mod.rs:9775` — `pub const REVOKE_ACCESS: ACCESS_MODE = ACCESS_MODE(4i32)`. Exists.
  This is the precise-removal primitive: `SetEntriesInAclW` with one `EXPLICIT_ACCESS_W` whose
  `grfAccessMode` is `REVOKE_ACCESS` and whose `Trustee` is `TRUSTEE_IS_SID` removes **every ACE for
  that one trustee** and touches no other trustee's ACEs. A blind ACL wipe is neither needed nor
  used.
- `Authorization/mod.rs:406` — `GetExplicitEntriesFromAclW(pacl, pccount, plist) -> WIN32_ERROR`.
  Exists; an alternative read-back to the `GetAce` loop `tests/profile.rs` already uses.
- `Authorization/mod.rs:513` — `SetEntriesInAclW(Option<&[EXPLICIT_ACCESS_W]>, Option<*const ACL>,
  *mut *mut ACL) -> WIN32_ERROR`. The same signature `profile::grant` already calls.
- `ConvertStringSidToSidW` is already imported by `crates/skein-sandbox/src/launch.rs`, and that
  file's own comment records that its allocation is freed with `LocalFree`, **not** `FreeSid` —
  `prune` must keep that distinction if it goes via a string SID.

### The AppContainer folder layout, measured

`icacls` on a real profile folder and its `AC` subfolder, this machine (Windows 11 Pro 10.0.26200):

| Object | Trustees |
|---|---|
| `…\Packages\skein-000986f7bfe73a3d` | `NT AUTHORITY\SYSTEM (I)(OI)(CI)(F)`, `BUILTIN\Administrators (I)(OI)(CI)(F)`, `<user> (I)(OI)(CI)(F)` — **no AppContainer SID** |
| `…\Packages\skein-000986f7bfe73a3d\AC` | the unresolvable AppContainer SID at `(OI)(CI)(CR)(F)`, plus the three above, plus `Mandatory Label\Low Mandatory Level:(NW)` |

**The package folder is unreachable by the sandboxed child; its `AC` subfolder is the child's own.**
That measured difference decides D1.3 and D3.

`crates/skein-sandbox/src/launch.rs` already establishes `LOCALAPPDATA` as load-bearing and measured
(*"`%LOCALAPPDATA%\Packages\<profile name>\`, and process creation resolves that path from the
child's own environment"*, with `std::env::var("LOCALAPPDATA")`), so this slice inherits an
already-proven fact rather than introducing a new assumption.

### Conflicts with the request's premises — recorded

- The request suggests *"a small manifest file under the silo root or a well-known Skein state
  directory"*. **The silo is the wrong scope and the plan rejects it** (D2): the profile is keyed on
  the fs-root path alone, so two silos over one root share one profile and one ACE. A per-silo
  manifest would hide a grant from the other silo and would let `--silo a` prune state `--silo b`
  still depends on. That is a correctness argument, not a taste one.
- The request asks whether a CLI subcommand must follow `proc_run`'s `#[cfg(windows)]`
  absent-not-refusing pattern. **It must not**, and slice 019 already decided the analogous case in
  the opposite direction — see D5, which cites the verified precedent.
- Everything else load-bearing in the request (the profile-naming scheme, both ACE masks, the API
  names, the residual's wording in both specs, `dev` being green after 020–022) was checked against
  source and is **correct as stated**.

### Branch state — action required before T0

`git worktree list` shows `024-sandbox-cleanup` sitting at `d364405`, the same commit as
`023-raw-wire-capture`, which is **8 commits behind `dev` (`12c14f5`)**. `dev` carries slices 021
and 022. The invariant is *branch cut from dev*, so **T0 must reset `024-sandbox-cleanup` onto `dev`
at `12c14f5`** before any edit. `git diff HEAD dev --stat` touches **no file this slice touches** —
`crates/skein-sandbox/**`, `crates/skein-cli/src/main.rs` and `crates/skein-cli/Cargo.toml` are
identical on both — so the reset is mechanical and carries no merge risk.

In-flight: `023-raw-wire-capture` shares the `d364405` base. Its subject (raw wire capture) lives in
`skein-gateway`/`skein-core`/ledger paths this slice never enters. File overlap: **none**, except
`crates/skein-cli/src/main.rs`'s `enum Command` if 023 also adds a subcommand — a one-variant
conflict at worst, to check at merge time.

---

## Approach

`skein sandbox list` and `skein sandbox prune`, backed by two new public functions in
`skein-sandbox` and a per-profile record file that `Sandbox::create` writes beside the profile
folder Windows itself created.

### D1 — The record is a plain text file, `skein-grants`, in the profile's own package folder

`Sandbox::create` writes `%LOCALAPPDATA%\Packages\<profile-name>\skein-grants`: one absolute path
per line, the fs-root first, then each run-dir. No JSON, no serde.

*Why a record at all.* The profile name is `sha256(root)` truncated — **one-way**. Nothing can
recover which directories carry a profile's ACEs from the profile itself, and Win32 offers no way to
ask. A record is not optional if `list` is to exist.

*Why here.* Putting it in the folder Windows created for the profile buys four things no other
location does:

1. **Lifetime coupling.** `DeleteAppContainerProfile` removes the profile *and its folder*, so the
   record cannot outlive what it describes. No orphan-record class of bug, nothing to
   garbage-collect.
2. **Not invented.** `SiloArgs::root` records this project's standing refusal to guess a data
   location (*"v0 has no config file and no platform data directory, so guessing a root would put an
   agent's journal somewhere the operator did not name"*). `…\Packages\<name>\` is not a location
   Skein chose — it is where Windows put the profile Skein created. Recording beside it follows the
   OS rather than inventing a `%LOCALAPPDATA%\skein\` the operator never named.
3. **Out of the sandbox's reach**, measured above: the package folder's DACL names SYSTEM,
   Administrators and the user, and *not* the AppContainer SID. A sandboxed child cannot read or
   write the record a later `prune` reads. Writing inside `AC` would have handed the child write
   access to it.
4. **Zero signature churn.** `Sandbox::create(root, run_dirs)` already has everything the record
   needs. No parameter threads through `RunAccess`, `EmbeddedServer::with_run`, `RunArgs`, or any
   existing test fixture.

*Format.* One path per line, UTF-8. A Windows path cannot contain a newline, so the format is
unambiguous, and it adds **no dependency** to a crate whose only non-Win32 dependency is `sha2` —
the same discipline that put `sha2` rather than a hashing framework in its `Cargo.toml`.
`serde_json` is a workspace dependency but is not in `skein-sandbox`'s graph, and this does not earn
it.

*Cumulative, not overwriting.* Two sessions over one root may name different `--run-dir`s; both
grants persist on the one profile. `create` therefore **reads the existing record, unions the new
paths in, and writes back**. Overwriting would silently orphan the first session's run-dir ACE —
precisely the bug this slice exists to close.

*Written before the grants, and fatal if it fails.* The ordering is decidable: a record with no ACE
is harmless (`prune` finds nothing and reports `clear`); an ACE with no record is unremovable and
*is* the residual. So the record goes first, and a record that cannot be written fails
`Sandbox::create` with the same loudness a failed grant does — `lib.rs`'s docstring already commits
to *"a sandbox that cannot be built must be an exit code before a model sees a tool"*.

**Rejected: a manifest under the silo root.** Wrong scope; a correctness bug, per *Conflicts* above.
It would also force a Skein-crate dependency into `skein-sandbox`, which slices 019 and 020 both
certify as *"a leaf depending on no Skein crate"* (Constitution IV).

**Rejected: a `%LOCALAPPDATA%\skein\` state directory.** Machine-scoped and out of the child's
reach, so it clears the safety bar — but it invents a location against `SiloArgs::root`'s stated
precedent and decouples the record's lifetime from the profile's, adding an orphan-record case
`list` would then have to explain.

**Rejected: encoding the root in the profile's `pszdisplayname`/`pszdescription`.**
`CreateAppContainerProfile` takes both (today `profile::create` passes the name three times), so the
root *could* live there. Reading it back needs the undocumented
`HKCU\…\AppContainer\Mappings\<SID>` registry layout — `GetAppContainerRegistryLocation` returns the
container's own hive, not the mapping — plus the `Win32_System_Registry` feature, and it cannot hold
N run-dirs. Rejected for resting a destructive command on an undocumented registry shape.

**Rejected: no record at all; the operator re-states the paths** (`skein sandbox prune --fs-root X
--run-dir Y`). Genuinely the smallest thing that removes an ACE and needs no persistent state. It
loses on the half that actually hurts: an operator with 955 profiles cannot discover what exists.
`list` is the point.

### D2 — Enumeration is a directory scan of `%LOCALAPPDATA%\Packages` for `skein-` + 16 hex

There is no Win32 enumeration API (full module surface listed above). The profile folder is the only
machine-wide, self-naming artifact, and `launch.rs` already depends on `LOCALAPPDATA` resolving to
that layout as a measured fact. `grants()` reads that directory, keeps entries matching
`^skein-[0-9a-f]{16}$`, and for each reads `skein-grants` if present.

`CheckNetIsolation.exe -s` (named as manual cleanup in `specs/019-…/spec.md`) is rejected: shelling
out to a system executable and parsing its localised output — this machine's `icacls` prints French
— is not a mechanism a `-D warnings` codebase should rest a destructive command on.

**Consequence, and `spec.md` must state it:** the 955 profiles that already exist, and any created by
slices 019–023, have **no record**. `grants()` lists them with no directories, and `prune` deletes
the profile while saying plainly that any ACEs it granted are unknown and were not removed. Most of
those roots were `TempDir`s that no longer exist, so their ACEs died with the directories; for the
rest `icacls` remains the operator's tool. Refusing to prune them would leave 955 profiles
permanently unremovable by the command built to remove profiles.

### D3 — `GetAppContainerFolderPath` is not used

It returns the **`AC` subfolder**, which is the container-writable one — the wrong place for the
record (D1.3). Using it for the write while the scan uses `LOCALAPPDATA` would also give two
independent derivations of one path that can disagree. One derivation, in one helper:
`fn packages_dir() -> Result<PathBuf, String>` reading `LOCALAPPDATA` and refusing loudly if unset.

### D4 — `prune` proves ownership from the trustee, not from the record

The safety invariant (Constitution VI, *never delete something it did not create*) is enforced
**structurally**, in three layers, and the record is deliberately not one of them:

1. **Name gate.** `prune(profile)` refuses anything not matching `^skein-[0-9a-f]{16}$` before it
   touches Win32 at all. `DeleteAppContainerProfile` is never reached with an arbitrary name.
2. **Trustee gate.** ACE removal is `SetEntriesInAclW` with `REVOKE_ACCESS` and `TRUSTEE_IS_SID`
   naming exactly the SID `DeriveAppContainerSidFromAppContainerName(profile)` returns. It is
   *incapable* of removing an ACE for any other trustee — the operator's, SYSTEM's,
   `ALL APPLICATION PACKAGES`', an inherited one. This is why a blind ACL wipe is not merely avoided
   but unrepresentable.
3. **Live check.** Before writing, `prune` reads the directory's actual DACL and writes back only if
   it finds an explicit ACE for that SID. No ACE ⇒ report `clear`, write nothing. A DACL that was
   never touched cannot be rewritten by accident.

Given layers 1–3 the record is only a *hint about where to look*. Even a tampered record could at
worst make `prune` revoke a `skein-<hash>` SID's ACE from some directory — an ACE only Skein could
have placed — and it cannot be tampered with anyway (D1.3).

**Is the deterministic name alone sufficient proof of ownership?** For deleting the *profile*: yes —
`skein-` + 16 lowercase hex is a namespace nothing else on a Windows machine produces. For revoking
an *ACE*: the name is not even the relevant question, because the trustee **is** the profile Skein
made. A cryptographic proof is additionally available for the fs-root — `profile_name(dir) ==
profile` — and holds by construction for no run-dir; it is used to label a recorded root in `list`'s
output and **not** as a precondition for revoking, since requiring it would make run-dir ACEs
unremovable.

*Failure handling.* Per directory: `missing` (path gone) and `clear` are reported and skipped. A
DACL write that fails — `ERROR_ACCESS_DENIED` on a directory the user does not own is reachable, as
`profile::unwritable` already records — is reported with the same elevation advice, and **the profile
is not deleted**, so the record survives and a retry or an elevated run can finish the job. **Order
is ACEs first, profile last**, for exactly that reason: deleting the profile first would delete the
record and orphan every remaining ACE irrecoverably.

### D5 — `skein sandbox {list,prune}`: present on every platform, refusing loudly off Windows

*Naming.* `main.rs`'s `enum Command` already has two noun-plus-subcommand groups for inspecting and
mutating local state — `Ledger { log, show, verify }` and `Secret { set, delete }` — against two bare
verbs for running the loop (`Chat`, `AcpAgent`). Cleanup of persistent machine state is the first
family, so `Sandbox { list, prune }`. It is also the exact name both specs already wrote down
(`skein sandbox prune`), so no operator-facing term is invented.

*Presence.* Slice 019 already decided the analogous case explicitly — `specs/019-…/tasks.md` T9:
*"`--allow-run` appears in `skein acp-agent --help` on **every** platform, deliberately: the flag is
present and refuses loudly on Linux and macOS rather than being silently absent, which is what 'fail
clearly, never silently degrade' means for a flag."*

The two interfaces differ in who reads them and what absence costs. A **tool advertisement** must be
absent off Windows because a model calling an allowlisted-but-disabled name gets a *fatal* run —
`wiring.rs`'s `agent_policy` and `git_tools` both record it: a disabled route is "not found", which
`NativeLoop::mediate` treats as fatal, where an unlisted name is a survivable `denied`. A **CLI
subcommand** is read by a human, and a missing subcommand is indistinguishable from a stale binary or
a typo. So `skein sandbox --help` works everywhere, and `list`/`prune` return a `NO_BACKEND`-shaped
`Err` off Windows.

*Confirmation.* No interactive prompt. `secret::delete` — the closest destructive precedent — prints
`deleted {reference}` and asks nothing, and `secret::set` records why this codebase refuses
interactive stdin at all. The invariant is met by the operator *running the command*: `prune`
requires an explicit selector (`--profile <NAME>` or `--all`, mutually exclusive, one required), so a
bare `skein sandbox prune` is a usage error, never a machine-wide delete.

### D6 — Public API added to `skein-sandbox`

Free functions, not methods: no `Sandbox` exists at cleanup time, and off Windows `Sandbox` is
uninhabited.

```rust
pub enum GrantKind { Root, RunDir }
pub enum GrantState { Granted, Clear, Missing }
pub struct GrantedDir { pub path: PathBuf, pub kind: GrantKind, pub state: GrantState }

/// One AppContainer profile Skein created. `dirs` is `None` when the profile
/// carries no record — every profile made before slice 024.
pub struct Grant { pub profile: String, pub sid: String, pub dirs: Option<Vec<GrantedDir>> }

pub struct Pruned {
    pub profile: String,
    pub revoked: Vec<PathBuf>,
    pub clear: Vec<PathBuf>,
    pub missing: Vec<PathBuf>,
    pub unrecorded: bool,
}

pub fn grants() -> Result<Vec<Grant>, String>;
pub fn prune(profile: &str) -> Result<Pruned, String>;
```

`GrantState` is computed from the **live DACL**, not from the record — that is what makes `list`
report reality rather than echo a manifest, and it costs one `GetNamedSecurityInfoW` per directory.
Both functions get `#[cfg(not(windows))]` arms returning the refusal, mirroring `Sandbox::create`.

`skein-cli` gains `skein-sandbox = { path = "../skein-sandbox" }` as an **unconditional**
dependency. Measured cost off Windows is zero new packages: `skein-sandbox`'s only non-Windows
dependency is `sha2`, already in the graph — slice 019's dependency-drift table measured exactly this
(`+1` on Linux, and that one is `skein-sandbox` itself). Routing through `skein-connectors` instead
was rejected: `prune` is not an MCP tool, and putting it behind the tool crate would hide a non-tool
capability behind the tool boundary. `main.rs`'s own docstring authorises the direct edge — *"each
subcommand is a call onto `skein-core`/`skein-silo` plus a rendering of the result"*.

### D7 — Output format

`skein sandbox list`: one **five-column tab-separated** line per (profile, directory), columns fixed
regardless of state so field offsets never shift — `ledger::log`'s stated rule (*"Four tab-separated
columns, unconditionally — the set does not change with `--run`, so a script's field offsets never
shift"*), applied.

```
<profile>\t<sid>\t<root|run-dir|unrecorded>\t<granted|clear|missing|->\t<path|->
```

An unrecorded profile is one line reading `unrecorded`, `-`, `-`.

`skein sandbox prune`: one line per action, in `secret::delete`'s shape — `revoked <path>`,
`clear <path>`, `missing <path>`, `deleted profile <name>`, and for the legacy case
`deleted profile <name> (no record: any directories it was granted are unknown and were not
touched)`.

No `--json`. Slice 020's residuals carry `--json` forward as an untaken cross-slice decision; this
slice does not take it either.

---

## Steps

Ordered, each independently verifiable. TDD: every behavioural step records its red verbatim under
`## Observed red` in `tasks.md`, per slices 019 and 020.

- **T0 — branch and specs.** Reset `024-sandbox-cleanup` onto `dev` at `12c14f5` (see *Branch state*;
  the reset touches no file this slice edits). Write
  `specs/024-sandbox-cleanup/{spec.md,plan.md,tasks.md}` mirroring slice 020's structure: spec with
  *What this slice changes for a user* / numbered *things a reader must know up front* / `FR-*` /
  `SC-*` / *Assumptions and residuals* / *Out of scope*; tasks with the **Constitution Check** block
  whose Cross-platform row follows 019's and 020's honest `⚠️` treatment (`#[cfg]` met, equivalent
  **not** met, deferred to Landlock/Seatbelt slices) and adds that there is nothing to clean up on
  those platforms because no backend there creates anything. Record the 955-profile measurement and
  the two `icacls` readings as facts in `plan.md`.

- **T1 — control baseline.** Re-measure, do not quote: `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, plus
  `ls -d "$LOCALAPPDATA/Packages"/skein-* | wc -l` immediately before and after that test run. The
  delta is how many profiles one suite run leaks today; T8's control is that it becomes 0.

- **T2 — types and signatures, no behaviour.** Add `GrantKind`, `GrantState`, `GrantedDir`, `Grant`,
  `Pruned`, `grants()` and `prune()` to `crates/skein-sandbox/src/lib.rs` with `todo!("T3")` /
  `todo!("T5")` Windows bodies and real `#[cfg(not(windows))]` refusal arms; add the new module
  `crates/skein-sandbox/src/record.rs`; add `skein-sandbox` to `crates/skein-cli/Cargo.toml`. This
  step exists so T3's and T5's reds are **unwritten-code** reds — slice 019's T2 established that a
  signatures-only step is worth its own commit for exactly this reason.

- **T3 — RED→GREEN: the record.** New `crates/skein-sandbox/tests/record.rs` (`#![cfg(windows)]`,
  following `tests/profile.rs`'s whole-file gate and its stated reason).
  - `a_created_sandbox_records_its_root_and_run_dirs_beside_its_profile` — `Sandbox::create` over a
    `TempDir` root plus a `TempDir` run-dir; read `%LOCALAPPDATA%\Packages\<profile>\skein-grants`
    directly and assert it holds both absolute paths, root first.
  - `a_second_create_over_one_root_unions_rather_than_replaces` — create with run-dir A, then create
    the same root with run-dir B; assert the record holds root, A **and** B. This is D1's cumulative
    rule, and without it B's session orphans A's ACE.

  Then implement `record.rs` (`packages_dir`, read, union, write) and its call in `profile::create`,
  positioned **before** the grants.

- **T4 — RED→GREEN: `grants()`.** Same file.
  - `a_created_profile_is_listed_with_its_directories_and_their_live_state` — after
    `Sandbox::create`, `grants()` contains the profile; `dirs` names both paths at
    `GrantState::Granted`; the root is `GrantKind::Root` and the run-dir `GrantKind::RunDir`; `sid`
    equals `sandbox.string_sid()`.
  - `a_profile_with_no_record_is_listed_with_no_directories` — create, then delete the
    `skein-grants` file by hand (the shape of all 955 legacy profiles); assert `grants()` still lists
    it with `dirs: None`.
  - `nothing_but_a_skein_hash_name_is_listed` — every returned `profile` matches
    `^skein-[0-9a-f]{16}$`. A real machine-wide assertion, since `%LOCALAPPDATA%\Packages` holds
    hundreds of unrelated Store packages.

- **T5 — RED→GREEN: `prune()` removes, and removes only its own.** First extract `tests/profile.rs`'s
  existing `allow_aces` / `granted_sids` / `granted_masks` / `FILE_MAPPING` read-back helpers into
  `crates/skein-sandbox/tests/dacl/mod.rs`, and `mod dacl;` from both files — one copy of a subtle
  `GetAce`/`MapGenericMask` helper rather than two. `profile.rs`'s assertions must stay
  byte-identical; the measured control is `cargo test -p skein-sandbox --test profile` unchanged
  before and after the extraction. Then, in new `crates/skein-sandbox/tests/prune.rs`
  (`#![cfg(windows)]`):
  - `a_real_grant_is_listed_then_pruned_and_the_ace_is_gone_from_the_dacl` — the acceptance test.
    `Sandbox::create` over a `TempDir` root + `TempDir` run-dir; assert via `granted_sids` that both
    DACLs name the SID; assert `grants()` lists it; `prune(&name)`; assert `granted_sids` on both
    directories **no longer contains the SID** — a real `GetNamedSecurityInfoW` read-back, not an
    assertion about intent; assert the package folder is gone and `grants()` no longer lists the
    profile.
  - `pruning_leaves_every_ace_it_did_not_write` — the refusal test. Snapshot `allow_aces(root)`
    **before** `Sandbox::create`; additionally place an unrelated ACE on the run-dir for
    `ALL APPLICATION PACKAGES` (`S-1-15-2-1`, well-known and emphatically not a `skein-` profile);
    create; prune; assert the root's `allow_aces` set equals the pre-create snapshot exactly, and
    that the run-dir still carries the `S-1-15-2-1` ACE. Proving this on a directory `prune` *does*
    rewrite is strictly stronger than proving it ignores one it never visits.
  - `prune_refuses_a_name_it_could_not_have_created` —
    `prune("Microsoft.WindowsCalculator_8wekyb3d8bbwe")` and `prune("skein-notahexstring")` are both
    `Err` naming the required shape; the control is that the calculator's package folder still exists
    afterwards. No destructive Win32 call is reachable from this path.
  - `an_unrecorded_profile_is_deleted_and_says_its_aces_are_unknown` — create, delete the record,
    prune; assert `Pruned::unrecorded`, assert the profile folder is gone, and assert the root's ACE
    for the SID **survives** — the honest, documented legacy behaviour from D2.

- **T6 — RED→GREEN: the non-Windows refusal.** `crates/skein-sandbox/tests/absent.rs`
  (`#![cfg(not(windows))]`): `grants()` and `prune("skein-0000000000000000")` are both `Err`. Runs on
  two of three CI legs and **cannot be executed on this machine** — record that under the standing
  caveat slice 019's *What could not be verified* established, and verify it compiles with
  `cargo check -p skein-sandbox --target x86_64-unknown-linux-gnu --all-targets`, which slice 019
  confirmed works for this crate precisely because its dependency graph is Skein-free.

- **T7 — RED→GREEN: the CLI.** `Command::Sandbox { command: SandboxCommand }` in `main.rs` with
  `SandboxCommand::{List, Prune { profile: Option<String>, all: bool }}`; new
  `crates/skein-cli/src/sandbox.rs` doing the D7 rendering and nothing else. New
  `crates/skein-cli/tests/cli_sandbox.rs`:
  - `sandbox_list_and_prune_are_documented_on_every_platform` — `skein sandbox --help` names both
    subcommands; `skein sandbox prune --help` names `--profile` and `--all`. No `#[cfg]`: this is
    D5's decision under test.
  - `prune_without_a_selector_is_a_usage_error` — exits nonzero naming both flags; `--profile X
    --all` also exits nonzero. No `#[cfg]`.
  - `#[cfg(windows)] a_real_grant_is_listed_and_pruned_through_the_binary` — create a sandbox
    in-process via `skein_sandbox::Sandbox::create` over a `TempDir`; run the real `skein sandbox
    list` subprocess and assert the profile name and the root path appear on one tab-separated line
    with `granted`; run `skein sandbox prune --profile <name>`, assert exit 0, and assert a second
    `list` no longer names it. Deliberately **no Win32 in `skein-cli`** — the DACL read-back is T5's
    job, and slice 019's discipline keeps `windows` out of this crate's graph.
  - `#[cfg(not(windows))] sandbox_list_refuses_with_a_reason` — exits nonzero and the message names
    the platform, matching `--allow-run`'s shape.
  - Follow `cli_secret.rs`'s stated convention throughout: a `Drop` guard that prunes, *"so a failing
    assertion cannot leave a credential behind on the developer's machine"* — here, a profile.

- **T8 — the test suite stops leaking profiles.** Add the same prune-on-`Drop` guard wherever
  `Sandbox::create` is called over a `TempDir`: `crates/skein-sandbox/tests/{profile,launch,escape}.rs`
  and `crates/skein-connectors/tests/connector.rs`. The measured acceptance is T1's counter — the
  `%LOCALAPPDATA%\Packages\skein-*` count before and after a full `cargo test --workspace` must be
  **equal**, against T1's measured positive delta. Every pre-existing assertion in those four files
  must stay byte-identical and green; note that
  `the_same_root_reuses_one_profile_and_two_roots_do_not` deliberately creates one profile twice, so
  its guard must run at test end, not between the creates. **This task is severable**: if it
  destabilises any slice-019/020 test, drop it and record the residual rather than weakening a proven
  test.

- **T9 — gates, dependency drift, control diff, close-out.** `cargo fmt --all --check`;
  `cargo clippy --workspace --all-targets -- -D warnings` with **no new `#[allow]`**;
  `cargo test --workspace` with counts named test-by-test against T1. `cargo tree --workspace
  --target x86_64-pc-windows-msvc` and `--target x86_64-unknown-linux-gnu` against `dev` — expected
  drift is **zero third-party packages on every target** (no new crate, no new `windows` feature).
  Control diff: `git diff dev --stat -- crates/skein-core/ crates/skein-gateway/ crates/skein-silo/
  crates/skein-acp/ crates/skein-mcp/ spikes/ rust-toolchain.toml` must be **empty**, and
  `crates/skein-connectors/` must show T8's test-only change with nothing in its `src/`.

- **T10 — hand-verification against a real session.** Not part of the implementation run; performed
  after merge and recorded under `## Live verification` in `tasks.md`, following slice 019's T13 and
  slice 020's T10:
  1. Run a real `skein acp-agent --silo … --model … --fs-root <dir> --allow-run --run-dir <toolchain>`
     session and drive one `proc_run` through it.
  2. `icacls <dir>` and `icacls <toolchain>` — confirm each names an `S-1-15-2-…` trustee. Read the
     SID, not the message: `icacls` prints French on this machine.
  3. `skein sandbox list` — confirm the profile, both directories, and `granted` on both.
  4. `skein sandbox prune --profile skein-<hash>`.
  5. `icacls` both directories again — confirm the SID is gone and every other trustee is unchanged;
     confirm `%LOCALAPPDATA%\Packages\skein-<hash>` no longer exists.
  6. Optionally `skein sandbox prune --all` against the 955 legacy profiles, recording the count
     before and after. This is the operator's call, not the command's — record it as a separate,
     explicitly consented step.

---

## Validation

**Project gates**, as slices 019/020 applied them: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings` with no new `#[allow]`, and
`cargo test --workspace` with per-target counts stated against T1's re-measured baseline;
`cargo check -p skein-sandbox --target x86_64-unknown-linux-gnu --all-targets` for the non-Windows
arm. The `%LOCALAPPDATA%\Packages\skein-*` count before/after a suite run is a **new** gate this
slice both introduces and meets.

**New tests, and the claim each one alone makes:**

| Test | Claim |
|---|---|
| `a_created_sandbox_records_its_root_and_run_dirs_beside_its_profile` | the record exists, is where D1 says, and holds both directories |
| `a_second_create_over_one_root_unions_rather_than_replaces` | a second session's run-dir does not orphan the first's ACE |
| `a_created_profile_is_listed_with_its_directories_and_their_live_state` | `grants()` reports the live DACL, not the record |
| `a_profile_with_no_record_is_listed_with_no_directories` | the 955 legacy profiles are visible, not invisible |
| `nothing_but_a_skein_hash_name_is_listed` | `list` does not report the machine's Store packages |
| `a_real_grant_is_listed_then_pruned_and_the_ace_is_gone_from_the_dacl` | **acceptance**: real create → real list → real prune → real DACL read-back shows the ACE gone |
| `pruning_leaves_every_ace_it_did_not_write` | **acceptance**: `REVOKE_ACCESS` on one trustee removes nothing else, proven on a directory prune rewrites |
| `prune_refuses_a_name_it_could_not_have_created` | `DeleteAppContainerProfile` is unreachable with a non-Skein name |
| `an_unrecorded_profile_is_deleted_and_says_its_aces_are_unknown` | the legacy path is honest about what it did not do |
| `grants_and_prune_refuse_off_windows` (`absent.rs`) | the platform gate, on two of three CI legs |
| `sandbox_list_and_prune_are_documented_on_every_platform` | D5's presence decision |
| `prune_without_a_selector_is_a_usage_error` | no bare destructive default |
| `a_real_grant_is_listed_and_pruned_through_the_binary` | the CLI is a real client of the library capability |
| `sandbox_list_refuses_with_a_reason` (non-Windows) | fail clearly, not silently absent |

No padding: no test of the record's line format beyond T3, no test of `Pruned`'s `Debug`, no property
test over path spellings — `FsRoot` canonicalizes upstream and `profile_name` already relies on that.

---

## Risks and rollback

**Blast radius.** `crates/skein-sandbox/src/{lib.rs,profile.rs}` plus new `record.rs`;
`crates/skein-cli/src/main.rs` plus new `sandbox.rs` and `Cargo.toml`; new tests in both crates; and
test-only edits in `crates/skein-connectors/tests/connector.rs` (T8 only).
**`skein-core`, `skein-gateway`, `skein-silo`, `skein-acp`, `skein-mcp`, `spikes/`, and every `src/`
file in `skein-connectors` are untouched.** The governed `ToolGateway`/`Ledger`/ACP chain gains and
loses nothing: no `StepKind`, no protocol change, no network.

| Risk | Why it is bounded | If it bites |
|---|---|---|
| `prune` removes an ACE it did not create | Structurally impossible: `REVOKE_ACCESS` names one `TRUSTEE_IS_SID` — the SID derived from a `skein-<16 hex>` name — so no other trustee's ACE is representable in the write. Proven by `pruning_leaves_every_ace_it_did_not_write`. | — |
| `prune` deletes a non-Skein AppContainer profile | Name gate refuses anything outside `^skein-[0-9a-f]{16}$` before any Win32 call. Proven by `prune_refuses_a_name_it_could_not_have_created`. | — |
| `prune --all` yanks a live ACP session's ACEs mid-run | Real, and deliberately not engineered around (YAGNI). `prune` is an explicit operator action; the session fails loudly on its next file access — the same failure a manual `icacls` would cause. | Record as an *Assumptions and residuals* entry; the operator restarts the session, which re-creates the profile and the grants |
| `DeleteAppContainerProfile` does not remove the folder, leaving an orphan directory | Not verified from documentation this session, and it is the mechanism D1.1 leans on. **T5's acceptance test asserts the folder is gone**, so this is measured before anything depends on it. | `prune` follows the API call with `std::fs::remove_dir_all`, and `tasks.md` records the correction under `## Observed red` |
| A failed record write breaks a previously-working `--allow-run` session | `create` gains a failure mode before its grants. It is a file write into a folder Windows just created and the user owns (measured DACL above). | The error names the path; the alternative — a silent unremovable grant — is the residual itself |
| The record exposes workspace paths to a reader of `%LOCALAPPDATA%` | The same user's own profile directory; paths only, no content, no secret. `Redactor` is not involved — nothing is written to a chain. | — |
| T8 destabilises slice 019/020's proven tests | T8 is last and severable by design. | Drop T8, record the leak as a residual |
| Concurrent test threads collide on one profile | Each fixture uses its own `TempDir`, so each hashes to a distinct name. `escape.rs` already runs `--test-threads=1` for its own separate reason. | — |

**Rollback.** `git revert` of the slice's commits removes the command, the record writer and the
tests. It does not remove records already written — they are deleted with their profiles, or become
harmless orphan files inside package folders — and, as slices 019 and 020 both record, a revert never
removed ACEs anyway. Uniquely for this slice, the rollback is *strictly safer than the status quo
ante*: any grant pruned before the revert stays pruned, and nothing in the revert can re-grant.

---

## Out of scope

- **Any Linux or macOS cleanup.** There is no backend on those platforms, so nothing is created and
  there is nothing to remove. The `#[cfg]`-without-an-equivalent position is inherited from ADR-0006
  via slices 019 and 020, not amended.
- **Automatic cleanup of any kind** — no `Drop` on `Sandbox`, no cleanup on process exit, no
  scheduled or age-based pruning, no `--prune-on-exit`. Deterministic per-root profile reuse is the
  feature slice 019 built; removing the grant implicitly would defeat it and would race concurrent
  sessions over one workspace. `prune` runs when, and only when, an operator types it. (T8's test
  `Drop` guards are fixture hygiene, live in test files only, and are not product behaviour.)
- **Cleanup of anything `skein-sandbox` does not create** — no silo pruning, no ledger compaction, no
  credential-store sweep, no `%TEMP%` cleaning, no general system-state management subsystem.
- **Recovering the directories of the 955 pre-existing recordless profiles.** The hash is one-way and
  no record exists. `list` says so and `prune` says so; `icacls` remains the operator's tool for those
  specific ACEs.
- **`--json` output, a config file, an `--fs-root`-as-selector convenience flag, `prune --dry-run`, or
  a repeatable `--profile`.** `list` already is the dry run. Carried forward from slice 020's
  residuals unchanged.
- **Reopening whether `--run-dir` should grant an ACE at all** — `specs/020-…/tasks.md` leaves that
  open explicitly (*"Whether `--run-dir` should grant at all, or grant only on request, is a decision
  this run does not take"*). This slice makes the grant removable; it does not revisit whether it
  should be made.
- **Touching `skein-core`, `skein-gateway`, `skein-acp`, `skein-mcp`, `skein-silo`,
  `skein-connectors`' `src/`, or `spikes/`** (ADR-0004 D2).
- **A PR.** No real remote; the local bare mirror exists only for Archon's worktree isolation.
