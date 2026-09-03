# Feature Specification: a pinned, handle-relative `FsRoot` (v0 slice)

**Feature Branch:** `021-fs-root-handle-relative` · **Created:** 2026-09-04 · **Status:**
Implemented (v0 slice) · **Input:** `crates/skein-connectors/src/fs.rs`'s own recorded residual —
*"there is a TOCTOU window between the `canonicalize` below and the `File::open` that follows it — a
symlink swapped into that window escapes the root. Closing it needs `cap-std`-style
directory-handle-relative opens, which this slice deliberately does not add"* — first written in
`specs/016-fs-connector/plan.md` and carried forward verbatim through slices 017, 018, 019 and 020,
whose *Out of scope* still names "the `canonicalize`-to-open TOCTOU fix" · Constitution II
(**local-first / containment**, NON-NEGOTIABLE), III (**test-first**), IV (**explicit boundaries**),
VII (**no capability without a real need**) · design §5.5.

`FsRoot` is the single containment primitive every connector this project has shipped rests on, and
until this slice it decided containment on a **string** and then let its callers re-walk that string
with `std::fs`. The check and the use were two different walks. This slice makes them one.

## What this slice changes for a user

Nothing about the tools' names, arguments, output or refusal wording changes. Two things change
underneath.

**A path a model names is now resolved and opened in a single walk** from a directory handle taken
when `--fs-root` was accepted. `fs_read`, `fs_list` and `fs_write` no longer hold a resolved path at
all, so there is no moment between deciding a path is contained and using it.

**The `--fs-root` directory is pinned for as long as a run is attached to it.** This is a real,
user-visible consequence and not an implementation detail: an operator who tries to rename, move or
delete their project directory while a `skein chat` or `skein acp-agent` session is running will get
a sharing violation on Windows (measured here: `os error 32`). On Unix the directory can still be
renamed, and the session keeps working on the same directory rather than following the name.
Stopping the session releases the handle.

## Eight things a reader must know up front

1. **`fs_write` had a hole, not merely a race.** `FsRoot::resolve_new` canonicalized the *parent* of
   a target and prefix-checked it, then re-appended the leaf name untouched. If `root/link.txt` was
   already a symlink to a file outside the root, `resolve_new` returned `root/link.txt` — contained
   by its parent — and `std::fs::write` followed the link and overwrote the outside file. No timing
   was involved. `resolve_new` is deleted; `create_file` walks the leaf like every other component.
2. **`fs_read` measured one file and could return another.** It called `std::fs::metadata` for the
   `READ_BYTE_CAP` check and then `std::fs::read_to_string` — two independent walks of one path.
   Both now come off one open handle, so the bytes counted are provably the bytes returned.
3. **No test asserts a won race, and none could honestly.** Race-freedom here is an argument from
   the mechanism — the containment check and the open are one `NtCreateFile` / `openat` walk — not
   from timing. Every test in this slice is deterministic. What they prove is that the mechanism
   refuses the escapes, and each one that could otherwise be a tautology carries an **unsandboxed
   positive control** showing the escape is real.
4. **`cap-std` was adopted, and it costs 16 packages on the Windows product tree.** Measured as a
   set difference against `dev` at `8e61c64`: `ambient-authority`, `cap-primitives`, `cap-std`,
   `fs-set-times`, `io-extras`, `io-lifetimes` ×2, `ipnet`, `maybe-owned`, `winx`, `windows-sys`
   0.59 and 0.60, `windows-targets` 0.52 and 0.53, `windows_x86_64_msvc` 0.52 and 0.53. Three
   `windows-sys` majors now sit in the tree on top of the `windows 0.61` the workspace pins, against
   that pin's own stated "one copy of a very large generated crate beats two".
5. **That cost was accepted on the `windows`/`win32job` precedent and not the `gix` one.** `gix` was
   refused at 112 packages because it was a convenience over a working `git2`. `windows-rs` and
   `win32job` were accepted, both large, because they were the only way to get a **security
   property** the product had claimed. This is the second shape.
6. **Hand-rolling was the serious alternative and it lost on three measurements**, not on taste:
   `CreateFileW` has no `RootDirectory` parameter, so the Win32 layer `skein-sandbox` uses cannot
   express this at all; a hand-rolled resolver would put `unsafe` in `skein-connectors`, the crate
   that serves model-supplied paths, breaking the tree's "`skein-sandbox` holds every `unsafe` block
   in the product" boundary; and it would need an `NtCreateFile` implementation and an `openat` one
   where the project deliberately has a single containment implementation with no `#[cfg]` in it.
7. **A hard link inside the root pointing outside it is not an escape either mechanism can see**,
   and this slice does not close it. A hard link is not a reparse point, so `canonicalize` has
   nothing to resolve and the directory entry genuinely is inside the root. Telling it apart needs
   device+inode identity, which is behind the unstable `windows_by_handle` feature in std on Windows
   and so unavailable on this project's pinned stable `1.97`.
8. **This slice closes the residual for `fs` only.** `git_status`/`git_log` and `proc_run` keep
   theirs, named below rather than quietly folded into the headline.

## Requirements

- **FR-001** Every path `fs_read`, `fs_list` and `fs_write` receive MUST be resolved and opened in
  one walk relative to a directory handle held by `FsRoot`, with no resolved path returned to the
  caller.
- **FR-002** That handle MUST be opened once, when `FsRoot` is constructed, and held for the
  lifetime of the `FsRoot`.
- **FR-003** A failure to open the root MUST be a loud construction failure naming the path the
  operator gave, in the same shape a root that cannot be canonicalized already produced.
- **FR-004** An absolute path, a drive-relative path (`C:foo`), a UNC or verbatim prefix, and an
  empty path MUST keep being refused before any join, with their existing wording.
- **FR-005** A refusal that means *this path left the root* MUST say so; a refusal that means *the
  operating system denied this access* MUST NOT be reported as an escape.
- **FR-006** `fs_read`'s byte-cap check and its read MUST come from the same open file.
- **FR-007** `fs_write` MUST refuse a target whose leaf is a link out of the root, and MUST NOT
  truncate that link's target.
- **FR-008** A refusal MUST reach the model as a tool-level error (`isError: true`) which the run
  survives, exactly as it did before.
- **FR-009** No `cap-std` type may appear in `skein-core`, in any `ToolTransport` signature, or in
  any public signature outside `FsRoot`.
- **FR-010** `src/fs.rs` MUST gain no `#[cfg]`: slice 016's "no `#[cfg]` in the containment code"
  invariant holds.

## Success criteria

- **SC-001** A directory swapped for a reparse point pointing outside the root, **after** the root
  was constructed, is refused by `fs_read`, `fs_list` and `fs_write`, with an unsandboxed positive
  control in the same test proving the swap really escapes, and with nothing planted outside.
- **SC-002** A directory that merely answers to the root's name after the real root is renamed away
  is not the root: a file planted in it is unreachable, and the real root's files still read.
- **SC-003** A pre-existing symlink at a write target's leaf is refused and its target is not
  truncated.
- **SC-004** An escape is reported as *"resolves outside the root … and is refused"*; a genuine
  `PermissionDenied` carrying a raw OS error is not.
- **SC-005** The refusal arrives at a model through a governed run as `isError: true`, with the run
  surviving and the chain verifying, when the escape is planted after the server exists.
- **SC-006** Slice 016's symlink containment test executes on Windows rather than skipping.
- **SC-007** Every pre-existing assertion in `fs_root.rs`, `fs_server.rs`, `connector.rs` and
  `governed_fs_run.rs` stays green with its text unchanged, except the three that named the deleted
  `resolve_new`.

## Assumptions and residuals

- **Residual — `git_status` and `git_log`.** `git2::Repository::open` takes an `AsRef<Path>` and
  libgit2 performs every subsequent open by path in its own C code. There is no libgit2 API that
  opens a repository relative to a directory handle. What this slice does buy git is the pinning:
  the root itself can no longer be replaced under a running session, so the window that remains is
  inside `.git`, below the root. Closing it fully means replacing `git2`.
- **Residual — `proc_run`.** `resolve_exe` keeps `FsRoot::resolve` because `CreateProcessW` takes a
  path and there is no handle-relative process launch, and `Sandbox::create` writes its DACL by
  path. This is not a regression and it is not what containment for the child rests on — that is the
  AppContainer's DACL, the Job Object and the per-call human approval.
- **Residual — hard links**, per point 7 above.
- **Assumption — `cap-primitives` reports an escape with no raw OS error.** The refusal wording
  depends on it. It is a dependency's internal detail, so both arms are pinned by a test rather than
  trusted.

## Out of scope

- Any change to `git2` or to how `git.rs` opens a repository.
- Any change to `proc_run`, `skein-sandbox`, the AppContainer profile or its DACL. The research
  question of whether one Win32 primitive could serve both is answered **no**: `skein-sandbox` uses
  the Win32 layer, handle-relative opens need the NT layer.
- Hard-link containment.
- A general capability-based filesystem abstraction. Three methods with three current callers
  (Principle VII). No `cap_std::net`, no `cap-tempfile`, no `Dir` in any public signature.
- The connector configuration hierarchy, `AccessScope::{Project, Folder, FullComputer}`, the trust
  registry.
- `spikes/` (ADR-0004 D2), `.github/`, `rust-toolchain.toml`.
