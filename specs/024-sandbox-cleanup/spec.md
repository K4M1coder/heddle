# Feature Specification: `skein sandbox list` / `skein sandbox prune` (v0 slice)

**Feature Branch:** `024-sandbox-cleanup` · **Created:** 2026-09-04 · **Status:** Implemented
(v0 slice) **Input:** `specs/019-shell-connector-windows/tasks.md` `## Next slice` — *"Nothing
removes either today, by design. A `skein sandbox prune` subcommand — or an explicit decision that
the operator owns it — is a real question this slice answers only with a residual."* — and
`specs/020-run-dir-allowlist/spec.md` *Assumptions and residuals*, which widens the same residual
from one directory to N and records that it survives a `git revert` ·
ADR-0006 (`docs/superpowers/adr/0006-shell-connector-windows-first-sandbox.md`, **inherited, not
amended**) · Constitution III (**test-first**), VI (**destructive actions, deny-by-default**), VII
(**no capability without a real need**) · design §4.1.

Slices 019 and 020 gave a sandboxed child two things that outlive the process that made them: an
AppContainer **profile** named `skein-` + 16 hex characters of `sha256(canonical root path)`, and
an inheritable **ACE** for that profile's SID on the `--fs-root` and on every `--run-dir`. Nothing
in the product removed either, and both specs said so and moved on. This slice builds the removal,
and the discovery step without which removal is not possible: a `--fs-root`'s profile name is a
one-way hash, so nothing can recover from a profile which directories carry its ACEs.

## What this slice changes for a user

One new subcommand group, `skein sandbox`, with two members.

`skein sandbox list` prints one tab-separated line per (profile, directory) — the profile name, its
SID, whether the directory was the fs-root or a run-dir, whether that directory's DACL **right
now** actually names the SID, and the path. It writes nothing and touches no ACL.

`skein sandbox prune --profile <NAME>` (or `--all`) revokes that profile's SID from each recorded
directory's DACL and then deletes the AppContainer profile. It reports what it did per directory,
one line each. It requires an explicit selector: a bare `skein sandbox prune` is a usage error, not
a machine-wide delete.

`Sandbox::create` gains one side effect to make `list` possible at all: it writes a plain-text
record of the directories it is about to grant, one absolute path per line, into
`%LOCALAPPDATA%\Packages\<profile-name>\skein-grants` — the folder Windows itself created for the
profile. Nothing else about creating or running a sandbox changes.

## Seven things a reader must know up front

1. **A record is not optional.** The profile name is `sha256(root)` truncated to 16 hex characters
   — one-way — and `Win32::Security::Isolation` has **no enumeration API** at all. Its complete
   public surface in windows-rs 0.61.3 is `CreateAppContainerProfile`,
   `DeleteAppContainerProfile`, `DeriveAppContainerSidFromAppContainerName`,
   `DeriveRestrictedAppContainerSidFromAppContainerSidAndRestrictedName`,
   `GetAppContainerFolderPath`, `GetAppContainerNamedObjectPath`,
   `GetAppContainerRegistryLocation`, `IsCrossIsolatedEnvironmentClipboardContent`,
   `IsProcessInIsolatedContainer`, `IsProcessInIsolatedWindowsEnvironment`,
   `IsProcessInWDAGContainer`. Nothing lists profiles and nothing maps a profile back to a
   directory. Without a record written at grant time, `list` cannot exist.
2. **The record lives beside the profile, and that location is measured, not chosen for taste.**
   `icacls` on this machine (Windows 11 Pro 10.0.26200): `…\Packages\skein-<hash>` names
   `NT AUTHORITY\SYSTEM`, `BUILTIN\Administrators` and the user, and **no** AppContainer SID; its
   `AC` subfolder names the AppContainer SID at `(OI)(CI)(CR)(F)`. So the package folder is out of
   the sandboxed child's reach and its `AC` subfolder is the child's own writable area. The record
   goes in the former. It also inherits the profile's lifetime — `DeleteAppContainerProfile`
   removes the folder — so there is no orphan-record class of bug to garbage-collect.
3. **`prune` proves ownership structurally, and deliberately does not trust the record.** Three
   layers: a **name gate** refusing anything outside `^skein-[0-9a-f]{16}$` before any Win32 call;
   a **trustee gate** — `SetEntriesInAclW` with `REVOKE_ACCESS` and one `TRUSTEE_IS_SID` naming
   exactly the SID derived from that name, which makes removing any other trustee's ACE
   unrepresentable rather than merely avoided; and a **live DACL check** that reads the
   directory's own security descriptor and writes back only if it finds an ACE for that SID. The
   record is a hint about where to look and nothing more.
4. **Order is ACEs first, profile last.** Deleting the profile deletes the record with it, so
   deleting it first would orphan every ACE not yet revoked, irrecoverably. A DACL write that fails
   — `ERROR_ACCESS_DENIED` on a directory the user does not own is reachable, as
   `profile::unwritable` already records — leaves the profile in place so a retry or an elevated
   run can finish the job.
5. **Every profile made before this slice has no record, and `prune` says so rather than refusing.**
   Measured on this machine: **1009** `skein-<16 hex>` folders at this run's start, growing by
   **27 per `cargo test --workspace`** (T1). None carries a record. `list` shows each as one
   `unrecorded` line; `prune` deletes the profile and states plainly that any directories it was
   granted are unknown and were not touched. Most of those roots were `TempDir`s that no longer
   exist, so their ACEs died with the directories; for the rest `icacls` remains the operator's
   tool. Refusing them would leave a thousand profiles permanently unremovable by the command
   built to remove profiles.
6. **`skein sandbox` is present on every platform and refuses loudly off Windows.** This is the
   opposite of what a *tool advertisement* does, and the difference is about who reads it. A model
   calling an allowlisted-but-disabled tool name gets `not found`, which `NativeLoop::mediate`
   treats as **fatal**, where an unlisted name is a survivable `denied` — so `proc_run` must be
   absent off Windows. A CLI subcommand is read by a human, for whom a missing subcommand is
   indistinguishable from a stale binary or a typo. Slice 019's T9 already decided the analogous
   case this way for `--allow-run`.
7. **Cleanup is never automatic.** No `Drop` on `Sandbox`, no cleanup at process exit, no age-based
   sweep. Deterministic per-root profile reuse is the feature slice 019 built; removing the grant
   implicitly would defeat it and would race two concurrent sessions over one workspace. `prune`
   runs when, and only when, an operator types it.

## Functional requirements

- **FR-001** `Sandbox::create` writes `%LOCALAPPDATA%\Packages\<profile>\skein-grants` — one
  absolute path per line, UTF-8, the root first and then each run-dir in the operator's order —
  **before** it grants any ACE. A record with no ACE is harmless; an ACE with no record is
  unremovable and is the residual this slice exists to close.
- **FR-002** The write is **cumulative**: `create` reads any existing record, unions the new paths
  into it preserving first-seen order, and writes back. Two sessions over one root may name
  different `--run-dir`s and both grants persist on the one profile; overwriting would orphan the
  first session's ACE.
- **FR-003** A record that cannot be written fails `Sandbox::create` with the loudness a failed
  grant already has. `lib.rs` commits to *"a sandbox that cannot be built must be an exit code
  before a model sees a tool"* and this is now part of building one.
- **FR-004** `skein_sandbox::grants()` returns one `Grant` per `%LOCALAPPDATA%\Packages` entry whose
  name matches `^skein-[0-9a-f]{16}$` and nothing else. `Grant::dirs` is `None` for a profile with
  no record and `Some` otherwise.
- **FR-005** Each `GrantedDir::state` is computed from the directory's **live DACL** through
  `GetNamedSecurityInfoW`, never from the record: `Granted` if an ACE names the profile's SID,
  `Clear` if none does, `Missing` if the path no longer exists. `list` reports reality, not an echo
  of a manifest.
- **FR-006** `GrantedDir::kind` is `Root` for the record's first line and `RunDir` for the rest,
  which is the order FR-001 writes them in.
- **FR-007** `skein_sandbox::prune(profile)` refuses a name outside `^skein-[0-9a-f]{16}$` with an
  `Err` naming the required shape, **before** any Win32 call. `DeleteAppContainerProfile` is
  unreachable from that path.
- **FR-008** ACE removal is one `EXPLICIT_ACCESS_W` with `grfAccessMode: REVOKE_ACCESS` and
  `TrusteeForm: TRUSTEE_IS_SID` naming the SID
  `DeriveAppContainerSidFromAppContainerName(profile)` returns. No other trustee's ACE is
  representable in the write.
- **FR-009** `prune` reads each recorded directory's DACL first and rewrites it only where an ACE
  for that SID is present. A directory with none is reported `clear` and not written; one that no
  longer exists is reported `missing` and skipped.
- **FR-010** `prune` deletes the profile only after every recorded directory has been handled
  without a write failure. A failed DACL write is an `Err` carrying `profile::unwritable`'s
  elevation advice, and the profile — and therefore the record — survives it.
- **FR-011** A profile with no record is pruned, and `Pruned::unrecorded` says so. Its
  directories are not guessed at.
- **FR-012** Both functions have `#[cfg(not(windows))]` arms returning the same shape of refusal
  `Sandbox::create` already returns there.
- **FR-013** `skein sandbox --help` documents `list` and `prune` on **every** platform, and
  `skein sandbox prune --help` documents `--profile` and `--all`.
- **FR-014** `--profile` and `--all` are mutually exclusive and one is required. A bare
  `skein sandbox prune` is a clap usage error naming both.
- **FR-015** `skein sandbox list` prints five tab-separated columns unconditionally —
  `<profile>\t<sid>\t<root|run-dir|unrecorded>\t<granted|clear|missing|->\t<path|->` — so a
  script's field offsets never shift with a profile's state. `ledger::log`'s stated rule, applied.
- **FR-016** `skein-cli` depends on `skein-sandbox` **unconditionally**. The measured cost off
  Windows is zero new third-party packages: `skein-sandbox`'s only non-Windows dependency is
  `sha2`, already in the graph.
- **FR-017** No pre-existing assertion's text changes. Test fixtures gain cleanup guards (T8);
  expectations do not move.

## Success criteria

- **SC-001** After `Sandbox::create` over a `TempDir` root and a `TempDir` run-dir,
  `%LOCALAPPDATA%\Packages\<profile>\skein-grants` exists and holds both absolute paths, root
  first. (`skein-sandbox/tests/record.rs`)
- **SC-002** A second `create` over the same root with a different run-dir leaves a record holding
  the root and **both** run-dirs. (`skein-sandbox/tests/record.rs`)
- **SC-003** `grants()` lists the created profile with both directories at `GrantState::Granted`,
  the root as `Root` and the run-dir as `RunDir`, and a `sid` equal to `Sandbox::string_sid()`.
  (`skein-sandbox/tests/record.rs`)
- **SC-004** A profile whose record file has been removed is still listed, with `dirs: None`.
  (`skein-sandbox/tests/record.rs`)
- **SC-005** Every profile `grants()` returns matches `^skein-[0-9a-f]{16}$`, on a machine whose
  `%LOCALAPPDATA%\Packages` holds hundreds of unrelated Store packages.
  (`skein-sandbox/tests/record.rs`)
- **SC-006** **Acceptance.** Create → both DACLs name the SID (read back through
  `GetNamedSecurityInfoW`) → `grants()` lists it → `prune` → neither DACL names the SID, the
  package folder is gone, and `grants()` no longer lists it. (`skein-sandbox/tests/prune.rs`)
- **SC-007** **Acceptance.** `prune` on a directory it *does* rewrite leaves the root's allow-ACE
  set byte-identical to a snapshot taken before `Sandbox::create`, and leaves an unrelated
  `ALL APPLICATION PACKAGES` (`S-1-15-2-1`) ACE planted on the run-dir intact.
  (`skein-sandbox/tests/prune.rs`)
- **SC-008** `prune("Microsoft.WindowsCalculator_8wekyb3d8bbwe")` and `prune("skein-notahexstring")`
  are both `Err` naming the required shape, and the calculator's package folder still exists
  afterwards. (`skein-sandbox/tests/prune.rs`)
- **SC-009** Pruning a recordless profile reports `unrecorded`, removes the profile folder, and
  leaves the root's ACE for that SID in place. (`skein-sandbox/tests/prune.rs`)
- **SC-010** Off Windows, `grants()` and `prune("skein-0000000000000000")` are both `Err`.
  (`skein-sandbox/tests/absent.rs`, two of three CI legs)
- **SC-011** `skein sandbox --help` names `list` and `prune`, and `skein sandbox prune --help` names
  `--profile` and `--all`, on every platform. `skein sandbox prune` with no selector, and with
  both, each exit nonzero. (`skein-cli/tests/cli_sandbox.rs`)
- **SC-012** On Windows, a real profile created in-process appears on one `skein sandbox list` line
  with its root path and `granted`; `skein sandbox prune --profile <name>` exits 0; a second `list`
  no longer names it. (`skein-cli/tests/cli_sandbox.rs`)
- **SC-013** Off Windows, `skein sandbox list` exits nonzero with a message naming the platform.
  (`skein-cli/tests/cli_sandbox.rs`)
- **SC-014** The `%LOCALAPPDATA%\Packages\skein-*` count is **equal** before and after a full
  `cargo test --workspace`, against T1's measured delta of +27. (T8)

## Assumptions and residuals

- **The 1009 profiles that already exist carry no record and their directories are unrecoverable.**
  `list` says `unrecorded`; `prune` deletes the profile and says what it did not do. This is stated
  behaviour, not a gap discovered later.
- **`prune --all` against a live ACP session removes that session's ACEs mid-run.** Real, and
  deliberately not engineered around: `prune` is an explicit operator action and the session fails
  loudly on its next file access — the same failure a manual `icacls` would cause. The operator
  restarts the session, which re-creates the profile and the grants.
- **A `git revert` of this slice does not remove records already written.** They are deleted with
  their profiles, or become harmless files inside package folders. Uniquely for this slice the
  rollback is *strictly safer than the status quo ante*: any grant pruned before the revert stays
  pruned, and nothing in a revert can re-grant.
- **`Sandbox::create` gains a failure mode it did not have** — a file write into a folder Windows
  just created and the user owns. The alternative to failing on it is a silent unremovable grant,
  which is the residual itself.
- **The record exposes workspace paths to a reader of `%LOCALAPPDATA%`** — the same user's own
  profile directory, paths only, no content and no secret. `Redactor` is not involved; nothing is
  written to a chain.
- The `canonicalize`-to-open TOCTOU window, conversation replay, streaming, provider
  authentication, a config file and `--json` output are carried forward from slices 016–022,
  untouched.

## Out of scope

- **Any Linux or macOS cleanup.** There is no sandbox backend on those platforms, so nothing is
  created and there is nothing to remove. The `#[cfg]`-without-an-equivalent position is inherited
  from ADR-0006 via slices 019 and 020, not amended.
- **Automatic cleanup of any kind** — point 7 above.
- **Cleanup of anything `skein-sandbox` does not create** — no silo pruning, no ledger compaction,
  no credential-store sweep, no `%TEMP%` cleaning.
- **Recovering the directories of the pre-existing recordless profiles** — point 5 above.
- **`--json` output, a config file, an `--fs-root`-as-selector flag, `prune --dry-run`, or a
  repeatable `--profile`.** `list` already is the dry run. Carried forward from slice 020's
  residuals unchanged.
- **`CheckNetIsolation.exe -s`, or any shelling out to a system executable.** Parsing a localised
  system tool's output — this machine's `icacls` prints French — is not a mechanism a `-D warnings`
  codebase should rest a destructive command on.
- **Reopening whether `--run-dir` should grant an ACE at all.** `specs/020-…/tasks.md` leaves that
  open explicitly. This slice makes the grant removable; it does not revisit whether it should be
  made.
- **Touching `skein-core`, `skein-gateway`, `skein-acp`, `skein-mcp`, `skein-silo`,
  `skein-connectors`' `src/`, or `spikes/`** (ADR-0004 D2).
