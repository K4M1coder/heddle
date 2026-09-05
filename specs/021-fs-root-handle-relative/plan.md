# Implementation Plan: slice 021 — a pinned, handle-relative `FsRoot`

**Spec:** `specs/021-fs-root-handle-relative/spec.md` · **Branch:** `021-fs-root-handle-relative`,
cut from `dev` at `8e61c64` · **No PR** (this repository has no remote) · Conventional Commits ·
Strict TDD (Constitution III).

## Problem

`FsRoot::resolve` canonicalized a candidate path, prefix-checked the canonical result against the
canonicalized root, and returned a `PathBuf`. Its callers then re-walked that path **by string**
with `std::fs::metadata`, `std::fs::read_to_string`, `std::fs::read_dir` and `std::fs::write`. Every
re-walk is a fresh name resolution by the operating system, so the thing checked and the thing
opened were related only by hope. Between the check and the use, anything that changed what a name
meant — a symlink, a directory junction, a rename — redirected the operation outside the root.

This is Constitution Principle II (NON-NEGOTIABLE) and it is the oldest unclosed residual in the
tree.

## What was verified before planning, rather than inherited from the comment

Read in the working tree at `8e61c64`:

| Operation | Path through `FsRoot` | Re-walked by string after the check? |
|---|---|---|
| `fs_read` (`server.rs`) | `self.root.resolve(&arg)` | **Yes, twice** — `std::fs::metadata` for the `READ_BYTE_CAP` check, then `std::fs::read_to_string`. The size checked was not provably the bytes read. |
| `fs_list` (`server.rs`) | `self.root.resolve(&arg)` | **Yes** — `std::fs::read_dir`. |
| `fs_write` (`server.rs`) | `self.root.resolve_new(&arg)` | **Yes**, and worse — the leaf was never examined at all. |
| `git_status` / `git_log` (`git.rs::open_contained`) | `root.path()` | **Yes** — `git2::Repository::open`, and every later open inside libgit2. |
| `proc_run` (`run.rs::resolve_exe`) | `root.resolve(command)` | **Yes** — the `PathBuf` goes to `CreateProcessW`. |
| `proc_run`'s cwd and ACL | `root.path()` | **Yes** — `Sandbox::create` writes the DACL by path. |

The `fs_write` row is a **deterministic hole, not a race**: `resolve_new` canonicalized the parent,
prefix-checked it, and re-appended the untouched leaf, so a pre-existing symlink at the leaf was
written straight through to its target.

## Decisions

### D1 — adopt `cap-std`, and pin the root's directory handle in `FsRoot`

`FsRoot` gains a `cap_std::fs::Dir` opened once, at construction, on the canonicalized root. Every
filesystem operation the `fs` tools perform resolves relative to that handle, component by
component, with the containment check and the open being the same walk. `FsRoot` keeps its
`root: PathBuf` for the callers that genuinely need a path (`git.rs`, `run.rs`, `Sandbox::create`)
and keeps its existing refusal messages, so no caller's contract changes (Principle IV).

**Why a crate, and why this one.** ADR-0006's standing rule is: prefer the well-adopted primitive
and justify it with measurement. ADR-0006 rejected `rappct` on *"~5,400 all-time downloads, single
maintainer, pre-1.0"*. `cap-std` 4.0.3 clears every one of those criteria by orders of magnitude —
~18.9M all-time downloads, Bytecode Alliance, 4.0.3 released 2026-08-20, a filed-and-fixed RustSec
advisory (RUSTSEC-2024-0445) proving a security process exists, a license compatible with this
workspace's Apache-2.0, and it resolves and builds under the pinned `1.97` toolchain. It is the
filesystem sandbox underneath Wasmtime's WASI, so its Windows path is exercised by every Wasmtime
user on Windows.

**Rejected alternative — hand-rolling the narrow primitive.** This is what the project chose over
`rappct` for AppContainer construction, so it is the serious alternative. It loses on three
measurements:

1. **The Win32 layer cannot express it.** `CreateFileW` has no `RootDirectory` parameter.
   Handle-relative opens on Windows require `NtCreateFile` with `OBJECT_ATTRIBUTES.RootDirectory`,
   i.e. the `Wdk_*` feature set of `windows-sys` — a strictly lower layer than the six
   `Win32_*` features `heddle-sandbox` uses. "We already do Win32 FFI here" does not transfer.
2. **It would put `unsafe` in `heddle-connectors`**, breaking the tree's recorded boundary that
   `heddle-sandbox` holds every `unsafe` block in the product — and breaking it in the crate that
   serves model-supplied paths.
3. **It would need two containment implementations** (`NtCreateFile` and `openat`) where the project
   deliberately has one with no `#[cfg]` in it.

**Rejected alternative — the weaker hand-rolled variant**: walk the components refusing reparse
points, then open with `FILE_FLAG_OPEN_REPARSE_POINT` / `O_NOFOLLOW`. Rejected as **not closing the
race at all**: both flags apply only to the final component, so every intermediate directory stays
swappable between the walk and the open. It narrows the window without shutting it, which is the
property this slice exists to stop claiming.

**Rejected alternative — `Dir::open_ambient_dir` per call.** It would avoid pinning the root, at the
price of leaving the root's *own name* re-walkable between calls — the outermost component of the
very window being closed.

**The cost, stated rather than glossed:** 16 new packages on the Windows product tree, three
`windows-sys` majors, two duplicate `windows-targets` pairs, and `ipnet` pulled in for a
`cap_std::net` this slice never uses (`cap-primitives` has no feature to drop it). Accepted on the
shape of the two precedents: `gix` was refused at 112 packages as a convenience over a working
`git2`; `windows-rs` and `win32job` were accepted, both large, as the only way to get a security
property the product had claimed. This is the second shape.

### D2 — three methods, not an abstraction

`open_file`, `create_file` and `read_dir`, each with exactly one current caller, plus a private
`explain`. `resolve_new` is deleted (`fs_write` was its only caller); `resolve` is kept because
`run::resolve_exe` needs a `PathBuf` for `CreateProcessW`, and its docstring now names that single
caller and the residual it still carries. No `cap_std` type appears in `heddle-core` or in any
`ToolTransport` signature (Principle IV, VII).

### D3 — the error discriminator, and why it is pinned by a test

`cap-primitives` reports an escape as an `io::Error` it constructs itself — `PermissionDenied` with
**no** raw OS error — where a genuine access denial from the operating system carries one (measured:
reading a directory as a file is `PermissionDenied` / `Some(5)`). `explain` splits on exactly that,
so a real denial is never dressed up as an escape. That discriminator is a dependency's internal
detail, so both arms are pinned by a test rather than trusted.

### D4 — the fixture drop-order fix lands first

Rust drops struct fields in **declaration order**, and six test fixtures declared their `TempDir`
first, so the temp directory is removed while the `FsRoot`, `Repository` or connector inside it is
still alive. `TempDir::drop` ignores removal failure, so today the only symptom is a leaked temp
directory per run and nothing fails. It had to land **before** the `Dir` handle existed, or the fix
would be indistinguishable from the regression it prevents.

## Validation

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo tree -e normal --target x86_64-pc-windows-msvc` — the delta is exactly the 16 packages
  named in `spec.md` point 4, and nothing was removed.

| Test | File | Proves |
|---|---|---|
| impostor root | `tests/fs_root.rs` | the root is the directory the operator named, not whatever answers to that name later. **Genuine red.** |
| leaf symlink on the write path | `tests/fs_root.rs` | `resolve_new`'s unchecked leaf is closed. **Skips on this machine; see `tasks.md`.** |
| junction swap, `FsRoot` level | `tests/fs_root.rs` | a reparse point out of the root is refused by the handle walk, with an unsandboxed positive control. No red; reason recorded. |
| junction swap, tool level | `tests/fs_server.rs` | the same for `fs_read`, `fs_list`, `fs_write`, plus *nothing was planted outside*. |
| `explain`'s two arms | `tests/fs_root.rs` | an escape is named as one; a real `PermissionDenied` is not. |
| governed refusal | `tests/governed_fs_run.rs` | the refusal reaches a model as `isError: true` and the run survives, with the escape planted **after** the server exists. |
| existing symlink test, un-skipped | `tests/fs_root.rs` | slice 016's containment assertion executes on Windows for the first time. |

**No test asserts a won race.** Race-freedom is an argument from the mechanism, not from timing, and
`spec.md` point 3 says so rather than implying a race was defeated in a test.

## Risks

- **Risk 1 — the pinned handle interferes with `Sandbox::create`'s ACL write on the root.**
  *Measured and cleared:* `run_server` (10 passed) and `governed_proc_run` are green on Windows
  after the change. `SetNamedSecurityInfoW` opens the directory for `WRITE_DAC` separately and is
  compatible with the `Dir` handle's sharing mode. The prepared fallback — opening the `Dir` per
  call — was not needed and was not taken.
- **Risk 2 — the root can no longer be renamed or deleted while heddle runs.** Real, measured
  (`os error 32`), and intended. Stated in `spec.md`'s *"What this slice changes for a user"*.
- **Risk 3 — `explain` depends on a dependency's internal detail.** Mitigated by D3's test.
- **Risk 4 — dependency footprint drift.** The three `windows-sys` majors come from
  `fs-set-times` / `winx` (0.59) and `io-extras` (0.60), not from `cap-std` directly, so they
  converge as those crates update. The measured tree is recorded in `tasks.md`.

**Rollback.** Every step is its own commit and the change is additive-then-substitutive: reverting
the manifest commit plus the two source commits restores `8e61c64`'s behaviour. The drop-order fix
and the un-skipped symlink test are independently valuable and can stay.

## Assumptions and residuals

Carried in `spec.md`'s *Assumptions and residuals*, not duplicated here.

## Out of scope

Carried in `spec.md`'s *Out of scope*, not duplicated here.

## Next slice

- **The `git2` residual.** `git_status`/`git_log` still open by path. Closing it means replacing
  `git2`, which is a different footprint argument and a different slice.
- **Hard-link containment**, which needs device+inode identity and so a toolchain move or a
  platform-specific `#[cfg]` this slice's invariant refuses.
- **The `ipnet` / `cap_std::net` dead weight.** Worth a look upstream: `cap-primitives` has no
  feature to drop it today.
