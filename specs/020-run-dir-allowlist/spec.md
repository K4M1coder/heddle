# Feature Specification: an operator-configured `--run-dir` allowlist for `proc_run` (v0 slice)

**Feature Branch:** `020-run-dir-allowlist` · **Created:** 2026-09-04 · **Status:** Implemented
(v0 slice) **Input:** `specs/019-shell-connector-windows/spec.md` point 5 — *"An operator-configured
`--run-dir` allowlist that both extends the search list and grants the AppContainer SID
read+execute on each named directory is the explicit next slice."* — and that slice's `tasks.md`
`## Next slice`: *"cargo, node and python are unreachable, so `proc_run` cannot build this project.
That is the single largest gap between what this slice ships and what an agent needs."* ·
ADR-0006 (`docs/superpowers/adr/0006-shell-connector-windows-first-sandbox.md`, **inherited, not
amended**) · Constitution II (**local-first**, NON-NEGOTIABLE), III (**test-first**), VI
(**deny-by-default**), VII (**no capability without a real need**) · design §4.3.

Slice 019 shipped `proc_run` and proved its containment. It also could not reach any executable an
agent actually wants — resolution was `%SystemRoot%\System32`, then `%SystemRoot%`, then a path
inside the fs-root, and nothing else. This slice closes that gap in the smallest shape that keeps
every property slice 019 proved: the operator names directories, the sandbox grants each of them
read **and execute only**, and executable resolution searches exactly the directories that were
granted.

## What this slice changes for a user

One new flag, `--run-dir <PATH>`, repeatable, on `skein acp-agent` only and requiring `--allow-run`.
Each named directory has this run's AppContainer identity granted an inheritable
read-and-execute entry on its ACL, is appended to the child's `PATH`, and is appended to the
directories a bare `command` name is looked for in. The advertised `proc_run` description names
them, so a model can tell a reachable `cargo` from an unreachable one without spending a turn
finding out.

Without the flag **nothing changes at all**: the same two directories are searched, the same
`PATH` is handed to the child, the same description is advertised, and no directory but the fs-root
has its ACL touched.

## Eight things a reader must know up front

1. **The mask is `GENERIC_READ | GENERIC_EXECUTE`, and it is copied from what Windows itself does.**
   Measured on this machine (Windows 11 Pro 10.0.26200) with `Get-Acl`: `%SystemRoot%\System32` and
   `C:\Program Files` each carry two ACEs for `ALL APPLICATION PACKAGES` (S-1-15-2-1) — an effective
   one at `0x1200A9` (`FILE_GENERIC_READ | FILE_GENERIC_EXECUTE`) and an inherit-only one at
   `0xA0000000` (`GENERIC_READ | GENERIC_EXECUTE`). Those are the two directories every AppContainer
   on the machine already executes from, including in slice 019's green tests. Writing
   `GENERIC_READ | GENERIC_EXECUTE` with `SUB_CONTAINERS_AND_OBJECTS_INHERIT` reproduces that pair
   exactly. There is no guessing in this decision.
2. **A generic mask written with inheritance splits into two ACEs, so an ACL read-back has to
   normalise.** Measured directly: writing `0xA0000000` reads back as `0x1200A9` with no inherit
   flags **plus** `0xA0000000` flagged `ObjectInherit, ContainerInherit, InheritOnly`. A test that
   compared against the constant it wrote would be wrong half the time. `MapGenericMask` with the
   file `GENERIC_MAPPING` is what normalises, and it is a no-op on an already-specific mask.
3. **A run directory is read+execute and the fs-root stays full access, and the difference is
   proven twice.** Once as an ACL read-back (the run dir's normalised masks carry `FILE_READ_DATA`
   and `FILE_EXECUTE` and **not** `FILE_WRITE_DATA`, `FILE_APPEND_DATA` or `WRITE_DAC`; the
   fs-root's do carry `FILE_WRITE_DATA`), and once as an effect (a sandboxed `copy` into the run
   directory leaves no file, where the same copy into the fs-root does). Narrowness in intent and
   narrowness in effect are two claims and this slice makes both.
4. **Resolution appends, never prepends: System32, `%SystemRoot%`, then each `--run-dir` in
   operator order.** Every `command` that resolved before this slice resolves to the same file
   after it. A named directory cannot shadow `cmd.exe`, `curl.exe` or `find.exe` for a
   configuration that already works. Ties between two run directories go to the first named. An
   operator who genuinely wants to override a System32 name puts the binary in the fs-root and names
   it as a path.
5. **A `command` containing `/` or `\` is still resolved against the fs-root and nowhere else.**
   The bare-name rule is what reaches a run directory. Letting a separator-form command land in one
   would mean either `FsRoot::resolve` stops meaning *inside the root* — the single invariant
   `fs_read`, `fs_list` and `fs_write` rest on — or a second traversal rule with its own `..` and
   symlink story. The operator named the directory, so its binaries are reachable **by name**, which
   is how anyone invokes `cargo`, `node` or `rustfmt`.
6. **Granting a directory needs `WRITE_DAC` on it, and a non-elevated skein does not always have
   it.** Measured: `C:\Program Files\nodejs` does not inherit `C:\Program Files`' AppContainer ACEs
   — its DACL is protected, names only `Authenticated Users`, `SYSTEM`, `Administrators` and
   `Users`, and its owner is `NT AUTHORITY\SYSTEM`. Naming it from a non-elevated shell fails with
   `ERROR_ACCESS_DENIED`. That is an exit code with a message naming the directory, the Win32 error,
   and the two ways out — an elevated skein once, or a directory you own — before a model is shown a
   tool. `D:\Users\<user>\.cargo\bin` and a rustup toolchain `bin` are user-owned with `FullControl`
   and grant without elevation.
7. **`.cmd`, `.bat` and `.ps1` shims do not work, and this is stated rather than discovered.**
   `resolve_exe` appends `.exe`, and `CreateProcessW` with an `lpApplicationName` cannot execute a
   batch file. So a project-local `node_modules\.bin` is a **legal** `--run-dir` whose real `.exe`
   entries run and whose `tsc.cmd`, `eslint.cmd` and `npm.cmd` do not. Supporting a shim would mean
   building a command line for `cmd.exe /c`, which is shell syntax by another name.
8. **This makes the toolchain reachable and launchable. It does not make `cargo build` work.**
   Whether a full build succeeds inside an AppContainer with no network, no `TEMP` and one writable
   directory is a separate slice's finding. `cargo --version` was measured to exit 0 under the
   sandbox's exact five-variable environment block; nothing beyond that is claimed. Relatedly, the
   `~\.cargo\bin\cargo.exe` **shim** re-executes the real cargo under a toolchain `bin`, so naming
   only `~\.cargo\bin` launches successfully and then fails to exec the toolchain binary — name the
   toolchain `bin` directory, or name both.

## Functional requirements

- **FR-001** `skein acp-agent` gains `--run-dir <PATH>`, repeatable. It is flattened where
  `--allow-run` is flattened and nowhere else, so `skein chat` does not carry it.
- **FR-002** `--run-dir` without `--allow-run` is a usage error naming **both** flags. The clap
  `requires` relation and `RunArgs::resolve`'s own check both express it: a second reader of the
  flag must not be able to lose the gate silently.
- **FR-003** There is **no** environment fallback. `--fs-root`, the flag this most resembles, has
  none, and a `;`-separated directory list in an environment variable is the shape slice 019's D8
  rejected on decidability grounds.
- **FR-004** `RunDirs::new` canonicalizes each path and refuses one that is not a directory, loudly
  and at construction — `FsRoot::new`'s rule and its recorded reason. Duplicates are removed after
  canonicalization so a doubled flag doubles neither an ACL write nor a `PATH` entry; order is
  otherwise the operator's.
- **FR-005** The allowlist rides inside `RunAccess::Allowed(RunDirs)`. Run directories without run
  access are unrepresentable. `RunAccess` is `Clone + Debug + PartialEq + Eq` and **not `Copy`**.
- **FR-006** `Sandbox::create(root, run_dirs)` grants the root `GENERIC_ALL` and each run directory
  `GENERIC_READ | GENERIC_EXECUTE`, both `GRANT_ACCESS` with
  `SUB_CONTAINERS_AND_OBJECTS_INHERIT`, and **fails the whole construction** if any grant fails.
- **FR-007** `Sandbox::run_dirs()` is public, for the reason `Sandbox::string_sid()` is: a test must
  read what was really configured rather than trust what the constructor claims. It is the **only**
  store of the list — `EmbeddedServer` gains no field — so what is searched and what was granted
  cannot disagree.
- **FR-008** `resolve_exe` searches `%SystemRoot%\System32`, then `%SystemRoot%`, then each run
  directory in order, first hit wins.
- **FR-009** The bare-name refusal names **every** directory it looked in, not two, and still says
  `%PATH%` is deliberately not searched. With no `--run-dir` the sentence is the same two
  directories it was before this slice.
- **FR-010** The child's `PATH` is `%SystemRoot%\System32;%SystemRoot%;<run dir 1>;<run dir 2>…`,
  each rendered through `win32_path` so no `\\?\` prefix reaches the child. That keeps
  `environment_block`'s stated invariant — the child's `PATH` is the directories the caller's own
  resolution searches — rather than quietly breaking it. No other environment variable is added.
- **FR-011** The advertised `proc_run` description enumerates the allowlisted directories when there
  are any, and is byte-identical to slice 019's when there are none. The tool description is the
  only channel that reaches the model: `RmcpToolTransport::list` maps name, description and
  parameters into `ToolSpec` and drops the server's `instructions`.
- **FR-012** No pre-existing assertion's **text** changes. Constructor spellings move; expectations
  do not. Slice 019's FR-016 discipline, applied to this slice's refactor.

## Success criteria

- **SC-001** A run directory's ACEs for the AppContainer SID, read back off the directory's own
  security descriptor and normalised through `MapGenericMask`, carry `FILE_READ_DATA` and
  `FILE_EXECUTE` and carry none of `FILE_WRITE_DATA`, `FILE_APPEND_DATA`, `WRITE_DAC`. The
  fs-root's carry `FILE_WRITE_DATA`.
  (`skein-sandbox/tests/profile.rs`)
- **SC-002** A binary copied into a `TempDir` named as a run directory launches inside the
  AppContainer and its real stdout comes back — a real ACE, a real `CreateProcessW`, real captured
  bytes, on a directory carrying no `ALL APPLICATION PACKAGES` ACE.
  (`skein-sandbox/tests/launch.rs`)
- **SC-003** A sandboxed `copy` into a run directory leaves **no file** there and exits nonzero,
  where the same copy into the fs-root lands. (`skein-sandbox/tests/escape.rs`)
- **SC-004** A bare name resolves inside a named run directory and `proc_run` reports its real
  output. (`skein-connectors/tests/run_server.rs`)
- **SC-005** The same bare name, with that directory **not** named, is refused with a message
  naming System32, `%SystemRoot%`, every directory that *was* named, and `%PATH%`.
  (`skein-connectors/tests/run_server.rs`)
- **SC-006** System32 wins a shadowing tie: a non-executable `cmd.exe` in a run directory does not
  stop `cmd.exe /c type seed.txt` succeeding. (`skein-connectors/tests/run_server.rs`)
- **SC-007** With a run directory configured, `proc_run`'s advertised description contains that
  directory's path; with none, it still contains the caps and the `PATH is not searched` sentence
  and gains nothing. (`skein-connectors/tests/connector.rs`)
- **SC-008** `skein acp-agent --help` documents `--run-dir`, `skein chat --help` does not,
  `--run-dir` without `--allow-run` exits nonzero naming both flags, and a `--run-dir` that is not a
  directory exits nonzero naming the path. (`skein-cli/tests/cli_acp_agent.rs`,
  `skein-connectors/tests/fs_root.rs`)

## Assumptions and residuals

- **The read+execute ACEs survive a `git revert`, exactly as slice 019's full-access one does.**
  Rolling this slice back removes the code, not the permission it wrote. Both are removed by hand
  (`icacls <dir> /remove:g *S-1-15-2-…`). Slice 019 carries the fs-root version of this residual;
  this slice widens it from one directory to N. Profile and ACE cleanup (`skein sandbox prune`,
  `DeleteAppContainerProfile`) remains the separately named residual it was.
- **An operator can name a directory so broad the allowlist becomes a `%PATH%`** — `C:\Windows`, a
  whole drive. That is not preventable in code and is not guessed at: the flag is opt-in,
  per-directory and operator-named. Nothing is auto-discovered, which is the invariant slice 019's
  D8 established and this slice does not reopen.
- **`--run-dir` writes to a directory's ACL, and that is stated where the operator meets it** — in
  the flag's own doc comment, in this document, and as an exit code when it fails.
- The `canonicalize`-to-open TOCTOU window `FsRoot` records, conversation replay, raw wire-byte
  capture, streaming, provider authentication, a config file and `--json` output are carried
  forward from slices 016–019, untouched.

## Out of scope

- **Any `%PATH%` search, or any auto-discovery.** No scanning `%PATH%`, no defaulting
  `%USERPROFILE%\.cargo\bin`, no reading `rustup` / `nvm` / `pyenv` state. Constitution II and VI;
  slice 019's D8 settled this and this slice does not reopen it by a different route.
- **`.cmd` / `.bat` / `.ps1` shims** — point 7 above.
- **Any new environment variable for the child** — no `CARGO_HOME`, `RUSTUP_HOME`, `USERPROFILE`,
  `TEMP`, and no `--env` flag. `cargo --version` and `node --version` were measured to need none.
- **Making `cargo build` actually work** — point 8 above.
- **A separator-bearing `command` resolving inside a run directory** — point 5 above.
- **Any Linux or macOS backend, or a cross-OS sandbox trait.** ADR-0006's named future work; this
  slice inherits slice 019's Windows-only scope.
- **A second tool of any kind** — no `proc_which`, no `proc_kill`, no run-dir listing tool.
  Principle VII. The advertisement and the refusal already tell a model what it can reach.
- **`--allow-run` or `--run-dir` on `skein chat`.**
- **Touching the governed chain.** No new `StepKind`, no change to `ToolGateway`, `Approval`,
  `Redactor` or `AcpPermissionTransport`. This slice changes only which executables `resolve_exe`
  accepts.
- **`spikes/`** (ADR-0004 D2), **`.github/`**, **`rust-toolchain.toml`**.
