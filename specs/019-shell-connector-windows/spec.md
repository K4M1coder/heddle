# Feature Specification: a Windows-only sandboxed process launcher and one `proc_run` tool (v0 slice)

**Feature Branch:** `019-shell-connector-windows` · **Created:** 2026-09-03 · **Status:** Implemented
(v0 slice) **Input:** ADR-0004 D3 (*"MCP tools (fs/git/shell)"* as v0 scope), ADR-0006
(`docs/superpowers/adr/0006-shell-connector-windows-first-sandbox.md`, which supersedes ADR-0005's
blanket deferral and settles the two crate decisions), `specs/018-acp-permission-gate/spec.md`'s
*Out of scope* — *"A `shell` connector. Still deferred; ADR-0006 scopes it Windows-first and it is
not this slice."* · Constitution II (**local-first**, NON-NEGOTIABLE), III (**test-first**), VI
(**deny-by-default**), VII (**no capability without a real need**) · design §4.3.

Slice 016 closed `fs`, slice 017 closed `git`. A Skein agent can read a file, write one behind the
permission gate slice 018 proved live, and inspect a repository — it can observe a codebase but not
act on it. This slice adds the missing capability in the smallest shape that is actually safe: a
Windows sandbox (AppContainer + Job Object) in its own crate, and exactly **one** MCP tool on top of
it, gated behind a second opt-in flag on `skein acp-agent` alone.

## What this slice changes for a user

One new flag, `--allow-run`, on `skein acp-agent` only. With it, and with `--fs-root`, the agent
gains one tool: `proc_run`, which launches one process inside an AppContainer bounded by a Job
Object over that root, with a 30-second wall clock and a 16 KiB-per-stream output cap. Without the
flag nothing changes at all — no tool is advertised, no sandbox is built, no directory's ACL is
touched. On Linux and macOS the flag is an exit code and a message, never a silently missing tool.

## Seven things a reader must know up front

1. **`FsRoot` is NOT the containment mechanism for the child process.** `FsRoot::resolve` is a path
   check inside *this* Rust process; a spawned process never passes through it. **The load-bearing
   mechanism is the DACL: one explicit inheritable `GRANT_ACCESS` ACE for the run's AppContainer SID
   on the configured root, and nothing else.** `FsRoot` still supplies *which* directory receives
   that ACE, and still validates the `command` argument when it names a relative path. A spec that
   conflated the two would ship a false containment claim.
2. **The provable claim is "cannot *write* outside its configured root", not "cannot read anything
   outside it".** An AppContainer token carries a low integrity level and the AppContainer SID; it
   can open any object whose DACL names that SID **or** `ALL APPLICATION PACKAGES` (S-1-15-2-1),
   which `C:\Windows`, `C:\Windows\System32` and `C:\Program Files` carry by default. The sandboxed
   process can read System32 and must be able to, or no executable would launch.
3. **`Sandbox::create` modifies the ACL of a directory the operator named.** It is the only way an
   AppContainer process can see the workspace at all (point 1), it is scoped to the one directory
   `--fs-root` already designates as the agent's workspace, and it is stated in `--allow-run`'s doc
   comment, in the tool's description, and here. Requiring the operator to run `icacls` by hand was
   rejected: it trades a stated, scoped side effect for a silent `ERROR_ACCESS_DENIED` at first use.
4. **No-network is `CapabilityCount: 0`, and WFP is what enforces it.** The AppContainer profile is
   created with `pcapabilities: None` — zero capability SIDs — so `internetClient` (S-1-15-3-1),
   `internetClientServer` (S-1-15-3-2) and `privateNetworkClientServer` (S-1-15-3-3) are all absent,
   and the `SECURITY_CAPABILITIES` passed at launch carries `CapabilityCount: 0`. Enforcement is the
   Windows Filtering Platform: MPSSVC compiles firewall rules into WFP filters and TCPIP.SYS
   evaluates them per connection, with capability SIDs as the condition its permit-filters match on.
   **Disabling the machine's firewall removes the permit filters and leaves the default block rules**
   — a machine with the firewall off is *more* restricted for an AppContainer, not less — which is
   why the network test is a legitimate gate rather than a flake. Loopback is blocked by a separate
   filter matching the `IsLoopback` condition, independently of the three capability SIDs, and this
   slice never performs the administrative act that would exempt it.
5. **`proc_run` cannot build this project, and saying otherwise would be a false claim.**
   Executable resolution is `%SystemRoot%\System32`, then `%SystemRoot%`, then a path inside the
   configured root — **there is no `PATH` search**, because `%PATH%` is ambient and per-process and
   resolving through it would make the reachable executable set undecidable from the configuration.
   `cargo`, `node` and `python` under the user profile are therefore **not reachable**, purely because
   the search never looks there. An operator-configured `--run-dir` allowlist that extends the search
   list is the explicit next slice, since built as slice 020 (`specs/020-run-dir-allowlist`).
   **Correction, made during that slice and recorded here rather than only there:** the clause this
   replaces claimed such a binary "would not launch even if the search found them, for want of an
   `ALL APPLICATION PACKAGES` ACE." Measured false: `CreateProcessW` opens the executable image under
   the *parent* process's (the operator's own) rights, before the AppContainer token exists, so the
   grant was never what gated launchability — the search list alone was. The grant is not pointless
   (slice 020 measured that it *is* what lets the running child read a file, or find and launch a
   sibling, inside that directory), but it does not do the job this sentence said it did.
6. **The argv discipline buys less than it looks like, and the boundary is elsewhere.** The tool
   never interprets shell syntax and Skein never builds a shell command line, so there is nothing in
   Skein's own code for an argument to be injected *into*. It does **not** follow that the model
   cannot obtain a shell — nothing stops it naming `cmd.exe` as `command`. A blocklist of shell
   binaries would be theatre (`powershell.exe`, `wsl.exe`, `mshta.exe`, a copy of `cmd.exe` placed
   inside the root). **The containment boundary is the AppContainer, plus the Job Object, plus the
   per-call human approval — not the identity of the executable.**
7. **This slice introduces the workspace's first `unsafe`.** There is none on `dev`: `grep -rn
   unsafe --include=*.rs crates` returns two hits, both the English word inside a doc comment. That
   is the strongest argument for `crates/skein-sandbox` existing as its own crate — a reviewer
   auditing memory safety has exactly one directory to read, the same discipline that makes
   `skein-connectors` the only crate naming MCP as a server and `src/git.rs` the only module naming
   `git2`.

## Functional requirements

- **FR-001** A new crate `crates/skein-sandbox` holds every `unsafe` block in this slice and is the
  only crate in the workspace containing `unsafe`. It compiles on all three operating systems;
  `windows` and `win32job` are `[target.'cfg(windows)'.dependencies]` and appear in no other crate's
  graph.
- **FR-002** `Sandbox::create(root) -> Result<Sandbox, String>` creates (or reuses) an AppContainer
  profile named `skein-` plus the first 16 hex characters of `sha256(canonical root path)` and grants
  its SID an inheritable full-access ACE on `root`. Deterministic, so repeated runs over one root
  reuse one profile and one ACE. `CreateAppContainerProfile` returning
  `HRESULT_FROM_WIN32(ERROR_ALREADY_EXISTS)` falls through to
  `DeriveAppContainerSidFromAppContainerName`.
- **FR-003** `Sandbox` stores the **string** SID and rebuilds a `PSID` per launch, which makes it
  `Send + Sync` **by construction** — no `unsafe impl` — because rmcp's handler must be
  `Clone + Send + Sync + 'static`.
- **FR-004** `Sandbox::run` composes the two mechanisms on one raw `CreateProcessW`:
  `Job::create_with_limit_info(ExtendedLimitInfo::new().limit_kill_on_job_close())`, a proc-thread
  attribute list carrying `PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES` with `CapabilityCount: 0`,
  `EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT`,
  then `job.assign_process(pi.hProcess.0 as isize)`, then `ResumeThread`. `CREATE_SUSPENDED` closes
  the assignment race completely — the child executes no instruction before assignment.
- **FR-005** Two reader threads each own one pipe read end and **keep draining past the cap**,
  counting dropped bytes, so the child never blocks on a full pipe. The parent's copies of the write
  ends are closed immediately after `CreateProcessW`, or the readers never see EOF.
- **FR-006** On non-Windows, `Sandbox::create` and `Sandbox::run` return a loud `Err` naming the
  platform and saying that shell tools are Windows-only in v0. Fail clearly, never silently degrade.
- **FR-007** One tool, `proc_run`, `#[cfg(windows)]`, on the existing `EmbeddedServer`. Named for
  what it does: `shell_run` would read to a model as an affordance for `|`, `>` and `&&`, every one
  of which this tool refuses. `RunParams { command: String, args: Vec<String> }` — no `cwd` (it is
  the root), no `env` (a fixed minimal block), no `stdin` (the child's `hStdInput` is a pipe whose
  write end is already gone, so a read on it is an immediate EOF), no per-call timeout. The fixed
  block is five variables — `LOCALAPPDATA`, `PATH`, `PATHEXT`, `SystemRoot`, `windir` — sorted
  case-insensitively. `PATH` is the two directories the executable resolution searches and never the
  operator's ambient `PATH`. `LOCALAPPDATA` is **not optional**: an AppContainer's per-package state
  lives under `%LOCALAPPDATA%\Packages\<profile name>\` and process creation resolves that path from
  the child's own block, so omitting it fails the launch with `ERROR_ENVVAR_NOT_FOUND`. There is
  deliberately no `TEMP`, so a tool needing scratch space fails loudly rather than littering the
  workspace.
- **FR-008** `RUN_TIMEOUT = 30s`, justified against `ModelArgs::timeout_secs`'s 120-second whole-turn
  default: a tool that can eat the entire turn budget makes `LoopBudget` meaningless. A timeout is an
  `Err(String)` — a tool error `NativeLoop::mediate` survives.
- **FR-009** `RUN_OUTPUT_BYTE_CAP = 16 KiB` **per stream**, which **truncates with a label** rather
  than refusing. This follows `STATUS_ENTRY_CAP`'s reasoning and not `READ_BYTE_CAP`'s: the process
  has already run and cannot be un-run, and there is no smaller call to suggest — the model cannot
  ask for fewer bytes. 16 KiB × 2 streams is half of `READ_BYTE_CAP`'s single-shot 64 KiB, because a
  run result carries two streams into the same prompt and the same Ledger row.
- **FR-010** A nonzero exit is `exit <n>` inside an `Ok`, never an `Err`: the process ran, the result
  is true, and the model needs the output. An `Err` would discard both.
- **FR-011** `RUN_ARG_COUNT_CAP = 64`; an embedded NUL in any argument and a command line over 32 000
  UTF-16 units are each a named `Err(String)`, refused before any launch.
- **FR-012** `EmbeddedServer::new` and `local_connector` keep their signatures and behave as
  `RunAccess::Denied`. New `EmbeddedServer::with_run(root, run)` and `local_connector_with_run(root,
  run)` are **fallible**, because a sandbox that cannot be built must be an exit code before a model
  sees it, not a per-call refusal. `with_run` calls `disable_route("proc_run")` when
  `run == Denied`, exactly as `new` already does for the git pair.
- **FR-013** Both gates are required. `ToolArgs::agent_policy(run)` appends
  `("proc_run", ToolAccess::Mutating)` to `allowed` **and** `"proc_run"` to `approved` when
  `run == Allowed` — `fs_write`'s exact shape, for `wiring.rs`'s exact recorded reason:
  `call_captured` consults the policy before the transport, so an unlisted mutating tool never
  becomes a question for the human behind the editor. And the allowlist must omit `proc_run` in
  exactly the cases the server disables it, or a model's invented `proc_run` ends the run instead of
  being a survivable `denied`.
- **FR-014** `--allow-run` is flattened into the `AcpAgent` variant **only**. `chat_policy` and
  `crates/skein-cli/tests/cli_chat.rs` are untouched: `proc_run` is `Mutating`, `skein chat` is
  non-interactive, and `chat_policy`'s existing docstring already spells out why a mutating tool that
  could only ever be denied should be *absent* rather than listed.
- **FR-015** `RunArgs::resolve()` is called **before** `Silo::open`, in the same position
  `tools.verify_root()` already occupies and for the documented reason: an unsupported flag must be
  an exit code and a message, not a JSON-RPC error an operator only meets inside an editor after a
  successful handshake.
- **FR-016** No pre-existing test assertion anywhere in the workspace changes. Every existing
  advertisement fixture passes no `--allow-run`, so each stays byte-identical on all three operating
  systems. **If one of them needs an assertion changed, the gate is wrong** — that is a stop
  condition, not a thing to patch.

## Success criteria

- **SC-001** `Sandbox::create` over a `TempDir` yields a string SID starting `S-1-15-2-`; twice over
  the same root yields the **same** SID; two different roots yield different SIDs; and the root's
  DACL afterwards contains an ACE naming that SID, read back through `GetNamedSecurityInfoW`.
- **SC-002** A sandboxed `cmd.exe /c type hello.txt` over a root containing `hello.txt` exits 0 and
  its stdout carries the file's real bytes — which simultaneously proves the child could traverse
  into a temp-dir root, that the granted ACE is what let it read, and that a System32 binary is
  launchable inside the container at all.
- **SC-003** A sandboxed `cmd.exe /c copy <root>\seed.txt <outside>\escaped.txt` leaves **no file at
  `<outside>`**. The file's absence on disk is the proof. The identical argv through plain
  `std::process::Command` **does** create it — without that positive control a mistyped `copy`
  invocation would make the test pass for the wrong reason.
- **SC-004** A sandboxed `curl.exe --max-time 3 --silent http://127.0.0.1:<port>/` against a
  `TcpListener` in the test process results in the listener **never accepting a connection**, and a
  nonzero child exit. The identical argv unsandboxed **does** produce an accepted connection.
  `--max-time 3` is load-bearing: a blocked AppContainer loopback connect times out rather than
  failing fast.
- **SC-005** A sandboxed `cmd.exe /c ping.exe -n 60 127.0.0.1` with a 2-second timeout returns the
  timeout `Err` within a few seconds. A leaked descendant would hold the pipe write end open and hang
  the reader join, so a test that *completes* is itself the assertion; the elapsed-time bound turns a
  leak into a failing test rather than a silent pass.
- **SC-006** An adversarial argv table — embedded quotes, trailing backslashes, `a"b`, `a\\`, spaces,
  the empty string, `&`, `|`, `>` — built into a command line and fed back through the real
  `CommandLineToArgvW` parses to exactly the input vector. A real Win32 parser as the oracle, not a
  hand-written mirror of the quoting rules.
- **SC-007** `local_connector_with_run(root, RunAccess::Denied)` on Windows advertises exactly the
  three names it advertises on `dev`. On non-Windows, `Sandbox::create` and
  `EmbeddedServer::with_run(root, RunAccess::Allowed)` both return the platform `Err`, and no
  catalogue contains a `proc_`-prefixed name.
- **SC-008** Over `RUN_OUTPUT_BYTE_CAP` on stdout is truncated **and labelled with the dropped byte
  count**; 65 arguments, a `command` naming a path outside the root, and a `command` that resolves
  nowhere are each a named refusal, the last naming both places it looked; a nonzero exit is
  `exit <n>` inside an `Ok`.
- **SC-009** An ACP client answering `AllowOnce` over the real protocol to the real
  `skein acp-agent --allow-run` binary lets `proc_run` execute; the model is told
  `[tool_result tool=proc_run status=ok]` with the real bytes, the chain is the 12-step allow shape
  and `skein ledger verify` reports `12 steps`.
- **SC-010** An ACP client answering `RejectOnce` under a command whose effect would be visible
  leaves **no file on disk**; `status=denied`, the 11-step chain, `11 steps`, and
  `StopReason::EndTurn`.
- **SC-011** `skein acp-agent --help` documents `--allow-run` and `skein chat --help` does not.
- **SC-012** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace` all pass. `git diff dev --stat` is empty for `crates/skein-silo/`,
  `crates/skein-core/`, `crates/skein-gateway/`, `crates/skein-mcp/`, `spikes/`, `.github/` and
  `rust-toolchain.toml`.

## Assumptions and residuals

- **This slice is intentionally Windows-only, and the Constitution's cross-platform rule is met on
  the `#[cfg]` and not on the equivalent.** ADR-0006 authorizes shipping `shell` on one OS first. A
  Linux (Landlock) and a macOS (Seatbelt) backend are each a separately-scoped future slice. On those
  two CI legs `skein-sandbox` compiles to a crate whose only reachable behaviour is a loud refusal,
  and `proc_run` is absent from every catalogue.
- **Orphaned AppContainer profiles accumulate.** The profile is deterministic per root and never
  deleted; deleting on drop would race concurrent sessions over the same root. Manual cleanup is
  `DeleteAppContainerProfile` (or `CheckNetIsolation.exe -s` to list them).
- **The granted ACE outlives the profile.** Deleting a profile leaves an ACE naming an unresolvable
  SID. Harmless — an unresolvable SID grants nobody anything — but stated rather than discovered.
  Both survive a `git revert` of this slice and are removed by hand.
- **No memory and no process-count Job limit.** `win32job` 2.0.3's `ExtendedLimitInfo` exposes
  `limit_working_memory`, `limit_kill_on_job_close`, `limit_breakaway_ok`,
  `limit_silent_breakaway_ok`, `limit_priority_class`, `limit_scheduling_class`, `limit_affinity` and
  `clear_limits` — and **nothing else**. `ActiveProcessLimit` and `JobMemoryLimit` are unreachable
  through it (the inner `JOBOBJECT_EXTENDED_LIMIT_INFORMATION` is `pub(crate)`) and would need a raw
  `SetInformationJobObject` — more unsafe code for a bound the timeout already terminates.
  `limit_working_memory` sets `JOB_OBJECT_LIMIT_WORKINGSET`, a working-set *trim* rather than a hard
  cap; setting it low would make a legitimate compiler thrash instead of fail.
- **Named upstream residual: a specific-executable WFP permit filter.** An AppContainer child can
  inherit a WFP permit filter scoped to a *particular* executable, so naming such a binary as the
  command can reach the network. Accepted and recorded rather than hidden.
- **SC-004 proves *a* real network denial hermetically and does not separately prove internet
  denial**, because no hermetic test can. The code-level fact behind that is `CapabilityCount: 0`,
  which `run_server.rs` asserts directly.
- **One inheritable ACE on the root is enough; no ancestor needs a traverse ACE.** The plan named
  the opposite as this slice's one stop condition, on the theory that an AppContainer token might not
  retain `SeChangeNotifyPrivilege`. Measured at T4: a sandboxed `cmd.exe` read a file inside a
  `TempDir` under the user profile with no ACE anywhere above the root. D7 stands unamended.
- **Captured output is decoded as UTF-8, lossily.** A console program writes the OEM code page, so a
  run whose output contains non-ASCII bytes reaches the model with replacement characters rather
  than as an encoding error. Losing the run entirely would be worse than rendering it imperfectly.
- **Slice 016's TOCTOU residual is inherited unchanged.** A directory swapped between `FsRoot::new`'s
  `canonicalize` and `Sandbox::create`'s `SetNamedSecurityInfoW` escapes the root.
- **`windows = "0.61"`, not 0.62.2.** `win32job` 2.0.3 depends on `windows ^0.61`; pinning 0.61 keeps
  one copy of a very large generated crate in the tree instead of two. Nothing here needs a
  0.62-only API. `assign_process` takes `isize`, not a `HANDLE`, so even a future split would not be
  a type-safety problem — the pin is about footprint.
- **The Windows leg is observed locally; the macOS and Linux legs are unobserved** until this
  repository has a remote — the standing caveat of slices 004–018, unamended. It bites harder here
  than in any prior slice, because this is the first slice whose two other legs run genuinely
  different code.

## Out of scope

Deliberately not done, so nobody helpfully does it:

- **Any Linux or macOS backend.** Landlock and Seatbelt are ADR-0006's named future work, each its
  own separately-scoped slice. Not stubbed, not started, not trait-shaped for three OSes.
- **A cross-OS sandbox trait.** ADR-0005's surviving point: one trait implemented three ways on day
  one is a subsystem, not a slice. `Sandbox` is a concrete type with one backend. Extracting a trait
  is the second backend's job, when there is a second implementation to generalize from.
- **`CreateRestrictedToken`, LPAC (`PROC_THREAD_ATTRIBUTE_ALL_APPLICATION_PACKAGES_POLICY`), and any
  named capability SID.** A restricted token with deny-only SIDs bounds *filesystem* reach but has
  **no** interaction with WFP, so it cannot deliver Constitution II's no-network property;
  AppContainer delivers both bounds through one primitive, and the second mechanism would add unsafe
  code and buy nothing the first does not already give.
- **A second tool of any kind** — no `proc_kill`, no `proc_status`, no streaming output, no
  background or detached runs, no job control. Principle VII.
- **Shell syntax in any form** — pipes, redirection, `&&`, globbing, variable expansion,
  multi-command scripts, interactive stdin.
- **An operator-configured executable allowlist / `--run-dir`.** Named above as the next slice.
- **Arbitrary environment injection.** The child gets a fixed minimal block; `RunParams` has no
  `env`.
- **Loopback exemptions** (`NetworkIsolationSetAppContainerConfig`, `CheckNetIsolation`,
  `Add-AppModelLoopbackException`). Constitution II is NON-NEGOTIABLE and the whole point of SC-004
  is that loopback stays blocked.
- **`--allow-run` on `skein chat`.**
- **`PROC_THREAD_ATTRIBUTE_JOB_LIST`** as a second attribute. A real alternative — it would create
  the process already in the job — rejected because `CREATE_SUSPENDED` closes the same race
  completely while staying inside `win32job`'s documented API and adding one fewer hand-built
  attribute blob to get wrong.
- **Deriving `ToolAccess` from MCP tool annotations.** Classification stays operator configuration.
- **The residuals slice 018 carried forward**, all untouched: the `canonicalize`-to-open TOCTOU fix,
  `role: "tool"` / `tool_call_id` conversation replay, raw wire-byte capture, streaming (SSE),
  provider authentication, a config file, `--json` output, the slices-008-vs-014
  `serde_json/preserve_order` reconciliation, the tool-call-id correlation gap, and the
  ACP-denied-call-projected-as-`Pending` gap.
- **`crates/skein-silo/`, `crates/skein-core/`, `crates/skein-gateway/`, `crates/skein-mcp/`,
  `spikes/`** (ADR-0004 D2), **`.github/`, `rust-toolchain.toml`** — all asserted empty in the
  control diff.
