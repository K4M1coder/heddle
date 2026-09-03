# Implementation Plan: slice 019 — a Windows-only sandboxed process launcher and one `proc_run` tool

**Spec dir to create:** `specs/019-shell-connector-windows/{spec.md,plan.md,tasks.md}` ·
**Branch:** `019-shell-connector-windows`, cut from `dev` at `b82f37a` · **No PR** (this repository
has no remote).

Everything named below was read in the working tree at `b82f37a`, or fetched from a named upstream
source, **this session**. Anything marked **new** does not exist yet. Where this plan contradicts
the request's own assumptions, it says so and gives the measurement.

`git status --short` in `D:\claudecode\skein` was empty when this plan was written and is empty now.
Nothing in the repository was modified; all probing was `curl` against crates.io and
raw.githubusercontent.com, plus reads of the working tree.

---

## Problem

ADR-0004 D3 names three connector families as v0 scope (*"MCP tools (fs/git/shell)"*). Slice 016
closed `fs`, slice 017 closed `git`, and ADR-0005 deferred `shell` entirely. ADR-0006
(`docs/superpowers/adr/0006-shell-connector-windows-first-sandbox.md`, read in full) supersedes that
deferral: `shell` ships **Windows-first**, gated off on Linux and macOS until each earns its own
backend, and it settles the two crate decisions — `win32job` for Job Object process-lifetime bounds,
and hand-rolled restricted-token/AppContainer construction against the official `windows` crate
rather than the low-adoption `rappct`.

Today a Skein agent can read files (`fs_read`, `fs_list`), write one (`fs_write`, behind the ACP
permission gate slice 018 proved live), and inspect a repository (`git_status`, `git_log`). It
cannot run a build, a test suite, or a linter — it can observe a codebase but not act on it. This
slice adds the missing capability in the smallest shape that is actually safe: a Windows sandbox
(AppContainer + Job Object) and exactly **one** MCP tool on top of it.

---

## What was verified before planning

Load-bearing facts. Each was measured this session. **Two of them refute claims the request or an
upstream docs summary would have led an implementer to assume; both are called out explicitly.**

### The tree as it is

1. `crates/` holds seven crates: `skein-acp`, `skein-cli`, `skein-connectors`, `skein-core`,
   `skein-gateway`, `skein-mcp`, `skein-silo`. Workspace `resolver = "2"`, `exclude = ["spikes"]`,
   `rust-version = "1.97"`; `rust-toolchain.toml` pins channel `1.97`.
2. `crates/skein-connectors/src/` is `{lib.rs, connector.rs, fs.rs, git.rs, server.rs}`.
   `EmbeddedServer::new(root: FsRoot) -> Self` is **infallible** and disables the two git routes via
   `ToolRouter::disable_route` when `git::is_git_repository(&root)` is false.
   `local_connector(root: FsRoot) -> Result<LocalConnector>` owns a `tokio::runtime::Runtime` and
   serves `EmbeddedServer` over a `tokio::io::duplex(DUPLEX_BUFFER)`.
3. `FsRoot` (`crates/skein-connectors/src/fs.rs`) canonicalizes once in `new`, and exposes
   `path()`, `resolve(&str)` (existing path: canonicalize, then prefix-check) and `resolve_new(&str)`
   (canonicalize the parent, re-append the file name). `rooted_relative` rejects
   `Component::Prefix` and `Component::RootDir` **before** the join, because `Path::join` with an
   absolute path discards the base.
4. Caps in `server.rs`: `READ_BYTE_CAP = 64 * 1024` (**refuses**, because the model can retry
   smaller), `LOG_COUNT_CAP = 50` (**refuses**, same reason), `STATUS_ENTRY_CAP = 200`
   (**truncates and labels the drop**, because `git_status` takes no arguments so there is no
   smaller call to fall back to). This asymmetry is documented in the source and is the precedent
   this slice's own cap decision has to answer to.
5. `skein_core::ToolPolicy::decide` denies an unlisted name outright; a `ToolAccess::Mutating` name
   runs only when it is also in `approved`. `ToolGateway::call_captured` appends `ToolCall`, then
   `Approval`, then reaches the transport, then appends `ToolResult` — every payload through
   `Redactor`. `ToolGateway::advertise` filters the transport's catalogue through the allowlist.
6. `skein_acp::AcpPermissionTransport` decorates the transport *inside* `ToolGateway`, offers
   exactly `skein.allow-once` / `skein.reject-once`, and maps anything but `allow-once` onto
   `SkeinError::ToolDenied` — which `NativeLoop::mediate` survives. Any other transport error is
   fatal to the run. This is why an allowlisted-but-absent route is a bug, not a nicety.
7. `crates/skein-cli/src/wiring.rs`: `ToolArgs { fs_root: Option<PathBuf> }` with
   `verify_root()`, `transport() -> Result<ConfiguredTools>`, `chat_policy()`, `agent_policy()`,
   `git_tools()`, `policy()`. `read_only()` is a free function returning the `fs_read`/`fs_list`
   pair. `chat_policy` deliberately omits `fs_write` ("a non-interactive command has nobody to
   ask"); `agent_policy` allowlists **and** approves it, because `call_captured` consults the policy
   before the transport, so an unlisted mutating tool never becomes a question for a human.
   `ModelArgs::timeout_secs` defaults to 120.
8. `crates/skein-cli/src/main.rs`: `ChatArgs` flattens `ModelArgs`/`RedactArgs`/`ToolArgs`; the
   `AcpAgent` variant flattens `SiloArgs`/`ModelArgs`/`RedactArgs`/`ToolArgs` separately. Adding a
   new `#[derive(Args)]` group to **one** subcommand is an established shape here.
   `crates/skein-cli/src/chat.rs` calls `args.tools.transport()?` and `args.tools.chat_policy()`;
   `crates/skein-cli/src/acp.rs` calls `tools.verify_root()?` before `Silo::open`, then
   `tools.transport()?` and `tools.agent_policy()` inside the session factory.
9. `crates/skein-cli/tests/cli_acp_agent.rs` holds slice 018's harness: `struct Answered { stop,
   asked, updates }` and `fn run_answering(root, silo, fs_root, base_url, answer:
   PermissionOptionKind) -> Answered`, driving the **real** `skein acp-agent` binary
   (`env!("CARGO_BIN_EXE_skein")`) against a `StubProvider`, answering every
   `RequestPermissionRequest` by selecting an **offered** option of the requested kind. Helpers
   `tool_call_reply`, `last_message`, `logged_kinds`, `temp_root`, `root_arg`, `skein`,
   `run_with_timeout`, `chunks`, `reply` are all in the same file. Reusable verbatim.
10. `crates/skein-connectors/tests/governed_fs_run.rs` is the "the effect on disk is the proof"
    precedent (`an_unlisted_write_never_reaches_the_server`,
    `an_out_of_root_read_is_refused_by_the_server_and_the_run_survives`), and
    `cli_acp_agent.rs::an_acp_client_that_rejects_stops_the_fs_write_and_the_run_survives` asserts
    `!files.path().join("planted.txt").exists()` as the whole point of the test.
11. **There is no `unsafe` in `crates/` today.** `grep -rn unsafe --include=*.rs crates` returns two
    hits, both the English word inside doc comments (`skein-core/src/tool.rs`,
    `skein-core/tests/tool_gateway.rs`). This slice introduces the workspace's first real `unsafe`,
    and that is the strongest argument for where it goes (see **Approach**, D1).
12. `.github/workflows/core.yml` runs `windows-latest`, `macos-latest`, `ubuntu-latest` with
    `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and
    `cargo test --workspace` (no `--include-ignored`). A `#[cfg(not(windows))]`-gated test runs on
    two of three legs; a `#[cfg(windows)]` test runs on one.
13. Constitution v1.1.0 (`.specify/memory/constitution.md`), Additional Constraints:
    *"First-class cross-platform ... No OS-specific call without `#[cfg]` + an equivalent."* This
    slice ships a `#[cfg]` with **no equivalent yet**, which is exactly what ADR-0006 authorizes and
    what the Constitution Check's Cross-platform row must say in plain words rather than paper over.
14. Existing test names this slice must not disturb:
    `connector.rs::the_connector_lists_the_three_tools_with_their_derived_schemas`,
    `connector.rs::the_connector_lists_the_git_tools_only_when_the_root_is_a_repository`,
    `cli_chat.rs`'s `vec!["fs_read", "fs_list"]` assertion, and `cli_acp_agent.rs`'s
    `["fs_read","fs_list","fs_write"]` assertion.

### The Windows APIs, verified from source (not assumed from the ADR's prose)

All signatures below are from `microsoft/windows-rs` tag `0.61.0`, files under
`crates/libs/windows/src/Windows/Win32/`, fetched raw this session. Latest published `windows` is
**0.62.2**; **this slice pins `windows = "0.61"`** — see D2.

15. `Win32::Security::Isolation` (feature `Win32_Security_Isolation`, present in both 0.61.x and
    0.62.x):
    ```rust
    pub unsafe fn CreateAppContainerProfile<P0, P1, P2>(
        pszappcontainername: P0, pszdisplayname: P1, pszdescription: P2,
        pcapabilities: Option<&[super::SID_AND_ATTRIBUTES]>,
    ) -> windows_core::Result<super::PSID>;                       // userenv.dll
    pub unsafe fn DeriveAppContainerSidFromAppContainerName<P0>(pszappcontainername: P0)
        -> windows_core::Result<super::PSID>;
    pub unsafe fn DeleteAppContainerProfile<P0>(pszappcontainername: P0) -> windows_core::Result<()>;
    pub unsafe fn GetAppContainerFolderPath<P0>(pszappcontainersid: P0) -> windows_core::Result<PWSTR>;
    ```
    `P0..P2: windows_core::Param<PCWSTR>`. **`pcapabilities: Option<&[SID_AND_ATTRIBUTES]>` passed
    as `None` is the concrete meaning of "no network capability"**: the profile is created with zero
    capability SIDs, so `internetClient` (S-1-15-3-1), `internetClientServer` (S-1-15-3-2) and
    `privateNetworkClientServer` (S-1-15-3-3) are all absent.
16. `Win32::Security` (feature `Win32_Security`):
    ```rust
    #[repr(C)] pub struct SECURITY_CAPABILITIES {
        pub AppContainerSid: PSID, pub Capabilities: *mut SID_AND_ATTRIBUTES,
        pub CapabilityCount: u32, pub Reserved: u32,
    }
    #[repr(C)] pub struct SID_AND_ATTRIBUTES { pub Sid: PSID, pub Attributes: u32 }
    #[repr(transparent)] pub struct PSID(pub *mut core::ffi::c_void);
    pub unsafe fn FreeSid(psid: PSID) -> *mut core::ffi::c_void;
    ```
    Constants: `DACL_SECURITY_INFORMATION` (`OBJECT_SECURITY_INFORMATION(4)`),
    `SUB_CONTAINERS_AND_OBJECTS_INHERIT` (`ACE_FLAGS(3)`), `NO_INHERITANCE` (`ACE_FLAGS(0)`),
    `CONTAINER_INHERIT_ACE`, `OBJECT_INHERIT_ACE`. `CreateRestrictedToken` also lives here and is
    **not used** — see D3.
17. `Win32::Security::Authorization` (feature `Win32_Security_Authorization`):
    ```rust
    pub unsafe fn GetNamedSecurityInfoW<P0>(pobjectname: P0, objecttype: SE_OBJECT_TYPE,
        securityinfo: OBJECT_SECURITY_INFORMATION, ppsidowner: Option<*mut PSID>,
        ppsidgroup: Option<*mut PSID>, ppdacl: Option<*mut *mut ACL>, ppsacl: Option<*mut *mut ACL>,
        ppsecuritydescriptor: *mut PSECURITY_DESCRIPTOR) -> WIN32_ERROR;
    pub unsafe fn SetEntriesInAclW(plistofexplicitentries: Option<&[EXPLICIT_ACCESS_W]>,
        oldacl: Option<*const ACL>, newacl: *mut *mut ACL) -> WIN32_ERROR;
    pub unsafe fn SetNamedSecurityInfoW<P0>(pobjectname: P0, objecttype: SE_OBJECT_TYPE,
        securityinfo: OBJECT_SECURITY_INFORMATION, psidowner: Option<PSID>, psidgroup: Option<PSID>,
        pdacl: Option<*const ACL>, psacl: Option<*const ACL>) -> WIN32_ERROR;
    pub unsafe fn ConvertSidToStringSidW(sid: PSID, stringsid: *mut PWSTR) -> windows_core::Result<()>;
    pub unsafe fn ConvertStringSidToSidW<P0>(stringsid: P0, sid: *mut PSID) -> windows_core::Result<()>;
    #[repr(C)] pub struct EXPLICIT_ACCESS_W { pub grfAccessPermissions: u32,
        pub grfAccessMode: ACCESS_MODE, pub grfInheritance: ACE_FLAGS, pub Trustee: TRUSTEE_W }
    #[repr(C)] pub struct TRUSTEE_W { pub pMultipleTrustee: *mut TRUSTEE_W,
        pub MultipleTrusteeOperation: MULTIPLE_TRUSTEE_OPERATION, pub TrusteeForm: TRUSTEE_FORM,
        pub TrusteeType: TRUSTEE_TYPE, pub ptstrName: PWSTR }
    ```
    Constants used: `GRANT_ACCESS` (`ACCESS_MODE(1)`), `TRUSTEE_IS_SID` (`TRUSTEE_FORM(0)`),
    `TRUSTEE_IS_WELL_KNOWN_GROUP` (`TRUSTEE_TYPE(5)`), `NO_MULTIPLE_TRUSTEE`, `SE_FILE_OBJECT`
    (`SE_OBJECT_TYPE(1)`). Note `TRUSTEE_W.ptstrName` is typed `PWSTR` even when the form is
    `TRUSTEE_IS_SID` — the `PSID` is cast into it, which is the documented Win32 idiom, and it is
    the single most transposable field in the slice.
18. `Win32::System::Threading` (feature `Win32_System_Threading`):
    ```rust
    pub unsafe fn CreateProcessW<P0, P7>(lpapplicationname: P0, lpcommandline: Option<PWSTR>,
        lpprocessattributes: Option<*const SECURITY_ATTRIBUTES>,
        lpthreadattributes: Option<*const SECURITY_ATTRIBUTES>, binherithandles: bool,
        dwcreationflags: PROCESS_CREATION_FLAGS, lpenvironment: Option<*const c_void>,
        lpcurrentdirectory: P7, lpstartupinfo: *const STARTUPINFOW,
        lpprocessinformation: *mut PROCESS_INFORMATION) -> windows_core::Result<()>;
    pub unsafe fn InitializeProcThreadAttributeList(lpattributelist: Option<LPPROC_THREAD_ATTRIBUTE_LIST>,
        dwattributecount: u32, dwflags: Option<u32>, lpsize: *mut usize) -> windows_core::Result<()>;
    pub unsafe fn UpdateProcThreadAttribute(lpattributelist: LPPROC_THREAD_ATTRIBUTE_LIST,
        dwflags: u32, attribute: usize, lpvalue: Option<*const c_void>, cbsize: usize,
        lppreviousvalue: Option<*mut c_void>, lpreturnsize: Option<*const usize>)
        -> windows_core::Result<()>;
    pub unsafe fn DeleteProcThreadAttributeList(lpattributelist: LPPROC_THREAD_ATTRIBUTE_LIST);
    pub unsafe fn ResumeThread(hthread: HANDLE) -> u32;
    pub unsafe fn TerminateProcess(hprocess: HANDLE, uexitcode: u32) -> windows_core::Result<()>;
    pub unsafe fn WaitForSingleObject(hhandle: HANDLE, dwmilliseconds: u32) -> WAIT_EVENT;
    pub unsafe fn GetExitCodeProcess(hprocess: HANDLE, lpexitcode: *mut u32) -> windows_core::Result<()>;
    #[repr(transparent)] pub struct LPPROC_THREAD_ATTRIBUTE_LIST(pub *mut core::ffi::c_void);
    #[repr(C)] pub struct PROCESS_INFORMATION { pub hProcess: HANDLE, pub hThread: HANDLE,
        pub dwProcessId: u32, pub dwThreadId: u32 }
    #[repr(C)] pub struct STARTUPINFOEXW { pub StartupInfo: STARTUPINFOW,
        pub lpAttributeList: LPPROC_THREAD_ATTRIBUTE_LIST }
    // STARTUPINFOW carries cb: u32, dwFlags: STARTUPINFOW_FLAGS, hStdInput/hStdOutput/hStdError: HANDLE
    pub const PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES: u32 = 131081;
    pub const EXTENDED_STARTUPINFO_PRESENT: PROCESS_CREATION_FLAGS = PROCESS_CREATION_FLAGS(524288);
    pub const CREATE_SUSPENDED:  PROCESS_CREATION_FLAGS = PROCESS_CREATION_FLAGS(4);
    pub const CREATE_NO_WINDOW:  PROCESS_CREATION_FLAGS = PROCESS_CREATION_FLAGS(134217728);
    pub const CREATE_UNICODE_ENVIRONMENT: PROCESS_CREATION_FLAGS = PROCESS_CREATION_FLAGS(1024);
    pub const STARTF_USESTDHANDLES: STARTUPINFOW_FLAGS = STARTUPINFOW_FLAGS(256);
    ```
    Note `CreateProcessW`'s `lpstartupinfo` is typed `*const STARTUPINFOW`, so the `STARTUPINFOEXW`
    is passed by cast with `StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32`. Note also
    `UpdateProcThreadAttribute`'s `attribute: usize`, so the `u32` constant is cast.
19. `Win32::Foundation`: `#[repr(transparent)] pub struct HANDLE(pub *mut core::ffi::c_void)`,
    `CloseHandle(HANDLE) -> Result<()>`, `LocalFree(Option<HLOCAL>) -> HLOCAL`, `WAIT_OBJECT_0`,
    `WAIT_TIMEOUT` (`WAIT_EVENT(258)`), `ERROR_ALREADY_EXISTS` (`WIN32_ERROR(183)`).
    `Win32::System::Pipes` (feature `Win32_System_Pipes`):
    `CreatePipe(*mut HANDLE, *mut HANDLE, Option<*const SECURITY_ATTRIBUTES>, u32) -> Result<()>`.
    `Win32::UI::Shell` (feature `Win32_UI_Shell`):
    `CommandLineToArgvW<P0>(lpcmdline: P0, pnumargs: *mut i32) -> *mut PWSTR` — **test-only**, V4.
20. **`win32job` 2.0.3** (crates.io, 1 258 220 all-time downloads — a very different adoption
    profile from the ~5 400 that got `rappct` rejected; last published 2025-05-15). Depends on
    `windows ^0.61` with features `Win32_Foundation, Win32_Security, Win32_System_JobObjects,
    Win32_System_Threading, Win32_System_ProcessStatus`. Verified API (docs.rs source):
    ```rust
    pub struct Job;                       // impl Send, impl Sync, impl Drop
    impl Job {
        pub fn create() -> Result<Self, JobError>;
        pub fn create_with_limit_info(info: &ExtendedLimitInfo) -> Result<Self, JobError>;
        pub fn handle(&self) -> isize;
        pub fn into_handle(self) -> isize;
        pub fn query_extended_limit_info(&self) -> Result<ExtendedLimitInfo, JobError>;
        pub fn set_extended_limit_info(&self, info: &ExtendedLimitInfo) -> Result<(), JobError>;
        pub fn assign_process(&self, proc_handle: isize) -> Result<(), JobError>;
        pub fn assign_current_process(&self) -> Result<(), JobError>;
    }
    pub struct ExtendedLimitInfo(pub(crate) JOBOBJECT_EXTENDED_LIMIT_INFORMATION);
    impl ExtendedLimitInfo {
        pub fn new() -> Self;
        pub fn limit_working_memory(&mut self, min: usize, max: usize) -> &mut Self;
        pub fn limit_kill_on_job_close(&mut self) -> &mut Self;
        pub fn limit_breakaway_ok(&mut self) -> &mut Self;
        pub fn limit_silent_breakaway_ok(&mut self) -> &mut Self;
        pub fn limit_priority_class(&mut self, priority_class: PriorityClass) -> &mut Self;
        pub fn limit_scheduling_class(&mut self, scheduling_class: u8) -> &mut Self;
        pub fn limit_affinity(&mut self, affinity: usize) -> &mut Self;
        pub fn clear_limits(&mut self) -> &mut Self;
    }
    ```
    **REFUTED, and this correction is load-bearing.** The request's brief names `ExtendedLimitInfo`
    methods `limit_active_process`, `limit_process_memory` and `limit_job_memory` (they appear in a
    docs.rs summary). **None of the three exists in win32job 2.0.3.** The inner
    `JOBOBJECT_EXTENDED_LIMIT_INFORMATION` is `pub(crate)`, so an implementer cannot reach
    `ActiveProcessLimit` or `JobMemoryLimit` through this crate at all. Do not write a task that
    calls them. D5 records what this slice bounds instead.
21. **`assign_process` takes `isize`**, not a `HANDLE` — so `job.assign_process(pi.hProcess.0 as isize)`
    crosses no type boundary even if two `windows` versions ever coexisted. That is what makes D2's
    pin a footprint decision rather than a correctness one.
22. **AppContainer network semantics** (Project Zero, *Understanding Network Access in Windows
    AppContainers*, Aug 2021, read this session; corroborated by Microsoft Learn's UWP
    network-isolation and Windows Firewall troubleshooting pages):
    - Enforcement is the **Windows Filtering Platform**: MPSSVC compiles firewall rules into WFP
      filters, BFE uploads them to the kernel, TCPIP.SYS evaluates them per connection. Capability
      SIDs are the condition those permit-filters match on.
    - **Disabling the firewall removes the permit filters and leaves the default block rules**, so a
      machine with the firewall off is *more* restricted for an AppContainer, not less. This
      disposes of the obvious "does the network test go green on a runner with the firewall off?"
      worry in the safe direction, and it is why V2 is a legitimate CI gate rather than a flake.
    - **Loopback is blocked for AppContainers by default**, by a separate filter matching the
      `IsLoopback` condition, independently of the three capability SIDs. Exemption requires
      `NetworkIsolationSetAppContainerConfig` / `CheckNetIsolation` / `Add-AppModelLoopbackException`
      — an explicit administrative act this slice never performs.
    - **A blocked loopback connect times out rather than failing fast.** V2 must be designed around
      that or it looks like a hang.
    - Known upstream residual, accepted and recorded: an AppContainer child can inherit a WFP permit
      filter scoped to a *specific executable* (the article's example is `dmcertinst.exe`), so naming
      such a binary as the command can reach the network. Goes in `spec.md`'s residuals, not hidden.
23. **AppContainer filesystem semantics.** An AppContainer token carries a low integrity level and
    the AppContainer SID; it can open only objects whose DACL names that SID (or
    `ALL APPLICATION PACKAGES`, S-1-15-2-1, which `C:\Windows`, `C:\Windows\System32` and
    `C:\Program Files` carry by default). A user-profile subtree — which is where
    `tempfile::TempDir` puts its directories — carries **no** `ALL APPLICATION PACKAGES` ACE. Two
    consequences the request correctly asked to be made explicit:
    - **`FsRoot` is NOT the enforcement mechanism for the child process.** `FsRoot::resolve` is a
      path check inside *this* Rust process; a spawned process never passes through it. **The
      load-bearing mechanism is the DACL: an explicit inheritable `GRANT_ACCESS` ACE for the
      AppContainer SID on the configured root, and nothing else.** `FsRoot` still supplies *which*
      directory receives that ACE, and still validates the `command` argument when it names a
      relative path. A spec that conflated the two would ship a false containment claim.
    - The provable claim is *"cannot **write** outside its configured root"*, **not** *"cannot read
      anything outside it"*: the sandboxed process can read `C:\Windows\System32` and every other
      `ALL APPLICATION PACKAGES` location, and must be able to, or no executable would launch.
      `spec.md` says this in these words.

---

## Approach

One new crate holding the sandbox, one new `#[cfg(windows)]` tool on the existing `EmbeddedServer`,
one new opt-in flag on `skein acp-agent` only. No new tool-calling path, no new approval mechanism,
no trait for three OSes.

### D1 — The sandbox is a new crate, `crates/skein-sandbox`

`skein-sandbox` is the workspace's **only** crate containing `unsafe`, and that is its reason to
exist: fact 11 says there is none today, and this slice adds roughly 400–500 lines of raw Win32 FFI.
A reviewer auditing memory safety in this workspace should have exactly one directory to read — the
same discipline that makes `skein-connectors` the only crate naming MCP as a server and `src/git.rs`
the only module naming `git2`. It also gives the Linux and macOS backends ADR-0006 defers an obvious
future home without building any of it now.

The crate compiles on all three OSes. Its Windows content is under `#[cfg(windows)]`; on Linux and
macOS `lib.rs` exposes the same public names with non-Windows bodies (D6), so no caller needs a
`#[cfg]` around a call site.

`crates/skein-sandbox/Cargo.toml`:
```toml
[dependencies]
sha2.workspace = true                       # the deterministic profile name (D6)

[target.'cfg(windows)'.dependencies]
windows.workspace = true
win32job.workspace = true

[dev-dependencies]
tempfile.workspace = true

[target.'cfg(windows)'.dev-dependencies]
windows = { workspace = true, features = ["Win32_UI_Shell"] }   # V4's CommandLineToArgvW only
```
and in the root `[workspace.dependencies]`:
```toml
windows = { version = "0.61", default-features = false, features = [
  "Win32_Foundation", "Win32_Security", "Win32_Security_Isolation",
  "Win32_Security_Authorization", "Win32_System_Threading", "Win32_System_Pipes",
] }
win32job = "2.0"
```
Neither crate appears in any other crate's graph, and `cargo tree` on Linux and macOS shows neither
at all.

**Rejected: `#[cfg(windows)] mod shell;` inside `skein-connectors`.** ADR-0006 explicitly permits
it, and `[target.'cfg(windows)'.dependencies]` would keep the deps off the other two OSes there just
as well — so the dependency-footprint argument does *not* decide it. It loses on the audit surface:
the crate hosting every MCP tool would also host every `unsafe` block, and its Cargo.toml's careful
"here is each dependency and why" story would gain two entries most of the crate has no business
knowing about.

### D2 — `windows = "0.61"`, not `0.62`

Latest published is 0.62.2 (crates.io, this session). `win32job` 2.0.3 depends on `windows ^0.61`.
Pinning 0.61 keeps **one** copy of a very large generated crate in the tree instead of two. Nothing
in this slice needs a 0.62-only API: every signature in facts 15–19 was read from the 0.61.0 source
and every feature name in D1 exists in both minor versions. Fact 21 means even a future split would
not be a type-safety problem — the pin is about footprint, the same discipline that produced
`git2`'s `default-features = false` and `ureq`'s.

### D3 — AppContainer, **not** `CreateRestrictedToken`

`CreateRestrictedToken` exists in `Win32::Security` (fact 16) and is deliberately unused. A
restricted token with deny-only SIDs bounds *filesystem* reach but has **no** interaction with WFP,
so it cannot deliver Constitution II's NON-NEGOTIABLE no-network property (fact 22: capability SIDs
are the condition WFP filters match on, and only an AppContainer token carries them). AppContainer
delivers both bounds through one primitive. ADR-0006's prose names both "restricted token" and
"AppContainer"; this plan resolves that to **AppContainer only**, because the second mechanism would
add unsafe code and buy nothing the first does not already give.

### D4 — Job Object composition: `CREATE_SUSPENDED` → `assign_process` → `ResumeThread`

The two mechanisms compose on **one** raw `CreateProcessW` call. `std::process::Command` cannot
carry `PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES`, so an AppContainer launch is a raw
`CreateProcessW` with a `STARTUPINFOEXW` regardless; and a raw `CreateProcessW` hands back
`PROCESS_INFORMATION.hThread`, which is what makes a suspended launch clean. Sequence:

1. `Job::create_with_limit_info(ExtendedLimitInfo::new().limit_kill_on_job_close())`.
2. `InitializeProcThreadAttributeList(None, 1, None, &mut size)` — expected to fail with
   `ERROR_INSUFFICIENT_BUFFER`; the out-param `size` is the point. Allocate, then initialize for
   real.
3. `UpdateProcThreadAttribute(list, 0, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
   Some(&caps as *const _ as *const c_void), size_of::<SECURITY_CAPABILITIES>(), None, None)` with
   `caps = SECURITY_CAPABILITIES { AppContainerSid: sid, Capabilities: null_mut(),
   CapabilityCount: 0, Reserved: 0 }`. **`CapabilityCount: 0` is the no-network decision in code.**
4. `CreateProcessW(Some(exe), Some(cmdline), None, None, /* binherithandles */ true,
   EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
   Some(env_block), Some(root), &si.StartupInfo as *const _, &mut pi)`.
5. `job.assign_process(pi.hProcess.0 as isize)`.
6. `ResumeThread(pi.hThread)`.

**Rejected: `PROC_THREAD_ATTRIBUTE_JOB_LIST` as a second attribute** (create the process already in
the job; `Job::handle() -> isize` would even supply the handle). A real alternative. It loses because
`CREATE_SUSPENDED` closes the same race completely — the child executes no instruction before
assignment — while staying inside win32job's documented API and adding one fewer hand-built
attribute blob to get wrong.

**Rejected: `std::process::Command` + `CommandExt::creation_flags(CREATE_SUSPENDED)`.** It cannot
attach the security-capabilities attribute at all, and it does not expose the child's thread handle,
so it could never be resumed.

### D5 — Which bounds this slice actually makes, given fact 20

- **Process-tree lifetime: `limit_kill_on_job_close()`.** Dropping the `Job` kills the whole tree,
  grandchildren included. This is the bound the timeout rests on, and V5 proves it.
- **Wall clock: `RUN_TIMEOUT = Duration::from_secs(30)`.** Justified against
  `ModelArgs::timeout_secs`, which defaults to 120 s for a whole turn (fact 7): a tool that can eat
  the entire turn budget makes `LoopBudget` meaningless. 30 s covers a linter or a focused test run;
  the tool description states the cap so the model can plan around it, and a timeout returns
  `Err(String)` — a tool error `NativeLoop::mediate` survives.
- **No memory and no process-count cap.** `limit_working_memory` sets
  `JOB_OBJECT_LIMIT_WORKINGSET`, a working-set *trim* rather than a hard cap; setting it low would
  make a legitimate compiler thrash instead of fail. `ActiveProcessLimit` and `JobMemoryLimit` are
  unreachable through win32job 2.0.3 (fact 20) and would need a raw `SetInformationJobObject` — more
  unsafe code for a bound the timeout already terminates. **Recorded as a named residual in
  `spec.md`, not silently omitted.**

### D6 — The public surface of `skein-sandbox`

```rust
// present on every OS; on non-Windows every constructor returns Err
pub struct Sandbox { /* root: PathBuf, sid: Vec<u16> — the string SID, wide, NUL-terminated */ }
pub struct Run { pub exit_code: u32, pub stdout: Captured, pub stderr: Captured }
pub struct Captured { pub text: String, pub dropped_bytes: usize }

impl Sandbox {
    /// Creates (or reuses) the AppContainer profile for `root` and grants its SID an
    /// inheritable full-access ACE on `root`. Loud failure, never a silent one.
    pub fn create(root: &Path) -> std::result::Result<Sandbox, String>;
    /// One process, bounded by the Job Object and the AppContainer. `exe` is absolute.
    pub fn run(&self, exe: &Path, args: &[String], stream_cap: usize, timeout: Duration)
        -> std::result::Result<Run, String>;
}
```

`Sandbox` stores the **string** SID (`ConvertSidToStringSidW` once at construction, then `FreeSid`)
rather than a live `PSID`, and rebuilds a `PSID` per launch with `ConvertStringSidToSidW` +
`LocalFree`. That makes `Sandbox` `Send + Sync` **by construction** — no `unsafe impl` — which
matters because rmcp's handler must be `Clone + Send + Sync + 'static`.

Profile name: `"skein-"` followed by the first 16 hex characters of `sha256(canonical root path as
UTF-8)`. `sha2` is already a workspace dependency. Deterministic, so repeated runs over the same root
reuse one profile and one ACE; 22 characters, comfortably inside AppContainer's 64-character name
limit. `CreateAppContainerProfile` returning `HRESULT_FROM_WIN32(ERROR_ALREADY_EXISTS)` falls
through to `DeriveAppContainerSidFromAppContainerName`.

Non-Windows bodies: `Sandbox::create` returns
`Err("a sandboxed process launcher has no backend on this platform; shell tools are Windows-only in v0")`,
and `Sandbox::run` — unreachable, since no `Sandbox` can exist — returns the same. This is the "fail
clearly, never silently degrade" arm, and it is directly assertable by a `#[cfg(not(windows))]` test.

### D7 — The ACL grant is a real, stated side effect on the operator's directory

`Sandbox::create` reads the root's DACL (`GetNamedSecurityInfoW`, `SE_FILE_OBJECT`,
`DACL_SECURITY_INFORMATION`), merges one `EXPLICIT_ACCESS_W` — `grfAccessPermissions = GENERIC_ALL`,
`grfAccessMode = GRANT_ACCESS`, `grfInheritance = SUB_CONTAINERS_AND_OBJECTS_INHERIT`,
`Trustee { TrusteeForm: TRUSTEE_IS_SID, TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
ptstrName: sid.0 as PWSTR }` — through `SetEntriesInAclW`, and writes it back with
`SetNamedSecurityInfoW`. Idempotent: re-granting the same trustee the same access under
`GRANT_ACCESS` merges rather than duplicating.

**This modifies the ACL of a directory the operator named.** It is the only way an AppContainer
process can see the workspace at all (fact 23), it is scoped to the one directory `--fs-root` already
designates as the agent's workspace, and it is stated in the flag's doc comment, in the tool's
description, and in `spec.md`.

**Rejected: require the operator to run `icacls` by hand.** It trades a stated, scoped side effect
for a silent `ERROR_ACCESS_DENIED` at first use, which is the opposite of "fail clearly".

### D8 — Executable resolution: System32 or inside the root. **No `PATH` search.**

`resolve_exe(root, command)`:
- if `command` contains `/` or `\` → `root.resolve(command)`, i.e. an existing path inside the root;
- otherwise → append `.exe` when absent, then look in `%SystemRoot%\System32`, then `%SystemRoot%`;
- otherwise refuse, naming both places it looked.

`%PATH%` is ambient, per-process and influenced by anything that has ever written the user's
environment; resolving through it would make the reachable executable set undecidable from the
configuration. A fixed list plus root-relative paths is decidable, deny-by-default, and makes the
tests hermetic (`cmd.exe`, `curl.exe`, `ping.exe` are all System32 and all carry
`ALL APPLICATION PACKAGES`).

**Stated cost, not hidden:** `cargo`, `node`, `python` and anything else installed under the user
profile are **not reachable** in this slice — and would not launch even if the search found them, for
want of an `ALL APPLICATION PACKAGES` ACE on `%USERPROFILE%\.cargo\bin`. This slice delivers the
launcher and the gate; an operator-configured `--run-dir` allowlist that both extends the search list
and grants the AppContainer SID read+execute on each named directory is the explicit next slice.
`spec.md` must say this in these terms — a spec that implied `proc_run` can build the project would
be a false claim.

### D9 — Argument handling, and an honest statement of what the argv discipline buys

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunParams {
    /// Executable: a bare name found in System32, or a path relative to the configured root.
    pub command: String,
    /// Arguments, each a separate value. Never a shell command line.
    pub args: Vec<String>,
}
```
No `cwd` (it is the root), no `env` (a fixed minimal block), no `stdin` (the child's `hStdInput` is a
closed pipe read end), no per-call timeout.

`CreateProcessW` takes one command-line **string**, so the launcher builds it with the documented
MSVCRT quoting rules — backslashes doubled only immediately before a quote or at the end of a quoted
argument; the argument wrapped in `"` when it is empty or contains space, tab or quote — and passes
`exe` separately as `lpApplicationName`. Refusals before any of that: more than
`RUN_ARG_COUNT_CAP = 64` arguments; any argument containing a NUL; a command line exceeding 32 000
UTF-16 units. Each is a named `Err(String)`, so the model receives a refusal it can act on rather
than a raw Win32 error code.

**Honest boundary statement, required in `spec.md`:** the tool never interprets shell syntax, and
Skein never builds a shell command line, so there is nothing in Skein's own code for an argument to
be injected *into*. It does **not** follow that the model cannot obtain a shell — nothing stops it
naming `cmd.exe` as `command`. A blocklist of shell binaries would be theatre (`powershell.exe`,
`wsl.exe`, `mshta.exe`, a copy of `cmd.exe` placed inside the root). **The containment boundary is
the AppContainer, plus the Job Object, plus the per-call human approval — not the identity of the
executable.** Slice 017's rejection of a `git` subprocess is not reopened by this: it turned on
`core.fsmonitor` execution and upward repository discovery, neither of which is a claim about argv.

### D10 — One tool, named `proc_run`

`#[cfg(windows)] #[tool] pub fn proc_run(&self, params: Parameters<RunParams>) -> Result<String, String>`
on the existing `EmbeddedServer`.

ADR-0006 names the **connector** `shell`, for continuity with ADR-0004 D3, and `spec.md` keeps that
word for the capability. The **tool** is named for what it does, per the request's own instruction:
`shell_run` would read to a model as an affordance for `|`, `>` and `&&`, every one of which this
tool refuses. `proc_` is a third family prefix beside `fs_` and `git_`.

Output, mirroring the existing line-oriented tools:
```
exit 0
--- stdout ---
<captured>
--- stderr ---
<captured>
```
with `# <n> bytes of stdout not shown` immediately under a truncated stream (and the same for
stderr). A nonzero exit is `exit <n>` inside an `Ok` — the process ran, the result is true, and the
model needs the output; an `Err` would discard both.

`RUN_OUTPUT_BYTE_CAP: usize = 16 * 1024`, **per stream**, which **truncates with a label** rather
than refusing. This follows `STATUS_ENTRY_CAP`'s reasoning, not `READ_BYTE_CAP`'s, and the
difference is decidable from fact 4: the process has already run and cannot be un-run, and there is
no smaller call to suggest — the model cannot ask for fewer bytes. A silent truncation would be a
wrong answer in a right answer's shape; a refusal would throw away a side effect a human approved.
16 KiB × 2 streams is 32 KiB worst case, half of `READ_BYTE_CAP`'s single-shot 64 KiB, because a run
result carries two streams into the same prompt and the same Ledger row.

Reader design — this is where a naive implementation deadlocks. Two `std::thread`s each own one pipe
read end, wrapped as a `File` via `FromRawHandle`, and **keep draining past the cap**, counting
dropped bytes, so the child never blocks on a full pipe. The main thread does
`WaitForSingleObject(pi.hProcess, 30_000)`. On `WAIT_TIMEOUT`: `TerminateProcess(pi.hProcess, 1)`,
then drop the `Job` (which kills descendants), which closes the write ends, which ends both readers;
then join. The parent's copies of the write ends are closed immediately after `CreateProcessW`, or
the readers never see EOF.

### D11 — Opt-in at the CLI: `--allow-run`, on `skein acp-agent` only

New in `wiring.rs`:
```rust
#[derive(Args)]
pub struct RunArgs {
    /// Offer the sandboxed `proc_run` tool over --fs-root. Windows only in v0.
    /// Grants the run's AppContainer identity an inheritable ACE on that directory,
    /// which is a real change to the directory's ACL.
    #[arg(long)]
    pub allow_run: bool,
}
impl RunArgs { pub fn resolve(&self) -> Result<RunAccess> { … } }
```
flattened into **only** the `AcpAgent` variant of `main.rs`'s `Command` enum. `resolve()` returns
`RunAccess::Denied` when the flag is absent, `RunAccess::Allowed` on Windows when present, and on
non-Windows an `Err` naming the reason. `acp::serve` calls it **before** `Silo::open`, in the same
position `tools.verify_root()` already occupies and for the documented reason: an unsupported flag
must be an exit code and a message, not a JSON-RPC error an operator only meets inside an editor
after a successful handshake.

Why a flag, and why only `acp-agent`:
- **Deny-by-default becomes structural, not merely policy** (Constitution VI). Running a process is a
  larger capability than `fs_write`; a second opt-in on top of `--fs-root` is the honest shape.
- **`skein chat` has nobody to ask.** `proc_run` is `Mutating`. `chat_policy`'s existing docstring
  already spells out why a mutating tool that could only ever be denied should be *absent* rather
  than listed. A flag on `chat` would either contradict that or build an AppContainer and mutate a
  directory's ACL for a tool that can never fire. `chat_policy` and `cli_chat.rs` are therefore
  **untouched by this slice**.
- **Every pre-existing advertisement assertion stays byte-identical on all three OSes** (fact 14).
  All of those fixtures pass no `--allow-run`. Without the flag, adding a Windows-only tool to the
  shared `EmbeddedServer` would have broken all of them on the Windows leg only. **If one of them
  needs an assertion changed, the gate is wrong; stop and fix the gate** — slice 017's bar, kept.
- **It gives the "gated off" property a Windows-side test too**, which the acceptance criteria
  explicitly allow ("or a Windows build with the shell feature/cfg artificially disabled").

`RunAccess` is a plain unconditional enum in `skein-connectors`:
`pub enum RunAccess { Denied, Allowed }`. `ToolArgs` gains `transport(&self, run: RunAccess)` and
`agent_policy(&self, run: RunAccess)`. Callers: `chat.rs` passes `RunAccess::Denied` (one line),
`acp.rs` passes the resolved value. `agent_policy` appends `("proc_run", ToolAccess::Mutating)` to
`allowed` **and** `"proc_run"` to `approved` when `run == Allowed` — `fs_write`'s exact shape, for
`wiring.rs`'s exact recorded reason: `call_captured` consults the policy before the transport, so an
unlisted mutating tool never becomes a question for the human behind the editor.

### D12 — The two-layer capability gate, mirroring `git`

`EmbeddedServer::new(root)` keeps its signature and stays infallible, behaving as
`RunAccess::Denied`. New `EmbeddedServer::with_run(root: FsRoot, run: RunAccess) -> Result<Self>` —
fallible, because a sandbox that cannot be built must be an exit code before a model sees it, not a
per-call refusal. `with_run` calls `tool_router.disable_route("proc_run")` when `run == Denied`,
exactly as `new` already does for the git pair. Same shape one level up:
`local_connector(root)` keeps its signature; new
`local_connector_with_run(root: FsRoot, run: RunAccess) -> Result<LocalConnector>`.

Both layers are required, and `wiring.rs`'s existing docstring already states the rule: a disabled
route is *not found*, which rmcp reports as a protocol error, `RmcpToolTransport` maps to
`SkeinError::Tool`, and `NativeLoop::mediate` treats as **fatal**. So the CLI allowlist must omit
`proc_run` in exactly the cases the server disables it, or a model's invented `proc_run` ends the run
instead of being a survivable `denied`.

---

## Steps

Ordered; each independently verifiable. Anchors are named items, never line numbers.

- **T0** Write `specs/019-shell-connector-windows/{spec.md,plan.md,tasks.md}` in slice 017/018's
  format, including the `## Constitution Check` table (the eight principles plus the
  `Cross-platform` row). The Cross-platform row must say plainly: *"⚠️ **This slice is intentionally
  Windows-only.** ADR-0006 authorizes shipping `shell` on one OS first; the Constitution's "no
  OS-specific call without `#[cfg]` + an equivalent" is met on the `#[cfg]` and **not** on the
  equivalent, which is deferred to a Linux (Landlock) and a macOS (Seatbelt) slice each. On the
  macOS and Linux CI legs `skein-sandbox` compiles to a crate whose only reachable behaviour is a
  loud refusal, and `proc_run` is absent from every catalogue — verified by a `#[cfg(not(windows))]`
  test that runs on two of the three legs."* Cut branch `019-shell-connector-windows` from `dev` at
  `b82f37a`.
- **T1** Control baseline: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --
  -D warnings`, `cargo test --workspace`, each recorded verbatim per target in `tasks.md`. Do not
  quote slice 018's numbers; re-measure.
- **T2 · manifests, before any behaviour.** Add `windows` and `win32job` to root
  `[workspace.dependencies]` exactly as D1 spells them. Create `crates/skein-sandbox/` with the D1
  `Cargo.toml` and a `lib.rs` holding only the D6 signatures — `todo!()` Windows bodies, real
  non-Windows bodies. Verify `cargo build --workspace` succeeds on Windows *and* that
  `cargo check --workspace` is clean with the Windows target's deps absent (i.e. no `#[cfg]`
  mistakes). This is a separate, early step so a dependency or feature-name error lands before any
  behaviour is written — slice 017's T2 discipline.
- **T3 · RED→GREEN — the AppContainer profile and the ACL grant.** New
  `crates/skein-sandbox/tests/profile.rs`, `#![cfg(windows)]`. Red: `Sandbox::create(tmp.path())`
  yields a string SID starting `"S-1-15-2-"`; calling it twice over the same root yields the **same**
  SID (profile reuse, D6); two different roots yield different SIDs; and the root's DACL afterwards
  contains an ACE naming that SID, read back with `GetNamedSecurityInfoW`. Green: a `profile` module
  in the crate.
- **T4 · RED→GREEN — the launcher walking skeleton.** New
  `crates/skein-sandbox/tests/launch.rs`, `#![cfg(windows)]`. Red, and **this step validates the
  whole D7/D8 model at once**: `sandbox.run(System32/cmd.exe, ["/c", "type", "hello.txt"], …)` over a
  root containing `hello.txt` returns `exit_code == 0` and stdout containing the file's bytes. It
  simultaneously proves the child could traverse into a temp-dir root, that the granted ACE is what
  let it read, and that a System32 binary is launchable inside the container at all.
  **If this step is red for an ACL or traversal reason rather than an unwritten-code reason, stop and
  fix the model before writing anything further** — the fallback is a traverse-only
  (`FILE_TRAVERSE`, `NO_INHERITANCE`) ACE on each ancestor of the root, which changes D7 and must be
  written into `spec.md` rather than done quietly. Green: a `launch` module implementing D4's
  six-step sequence and D10's reader threads.
- **T5 · RED→GREEN — the escape reproductions.** `crates/skein-sandbox/tests/escape.rs`,
  `#![cfg(windows)]`, holding V1, V2 and V5 below, each with its unsandboxed positive control. These
  are the slice's security gates, written before the code that makes them pass is finished — slice
  017's containment-repro discipline applied to a process instead of a repository.
- **T6 · RED→GREEN — argv quoting.** `crates/skein-sandbox/tests/argv.rs`, `#![cfg(windows)]`,
  holding V4's `CommandLineToArgvW` round trip and D9's three refusals (argument count, embedded NUL,
  oversized command line).
- **T7 · RED→GREEN — the tool.** New `#[cfg(windows)] mod run;` in `skein-connectors`, plus the
  `proc_run` `#[tool]` method, `pub struct RunParams`, `pub const RUN_OUTPUT_BYTE_CAP`,
  `RUN_TIMEOUT`, `RUN_ARG_COUNT_CAP`, and the unconditional `pub enum RunAccess`.
  `EmbeddedServer::with_run` and `local_connector_with_run` per D12; `EmbeddedServer::new` and
  `local_connector` keep their signatures. Driven by a new
  `crates/skein-connectors/tests/run_server.rs` calling the `#[tool]` method directly — the level
  that sees an `Err(String)` before rmcp wraps it, which is `fs_server.rs`/`git_server.rs`'s
  precedent. Covers V7.
- **T8 · RED→GREEN — the absence gates.** Two tests appended to
  `crates/skein-connectors/tests/connector.rs` (V6): a `#[cfg(windows)]` one asserting
  `local_connector_with_run(root, RunAccess::Denied)` advertises no `proc_run`, and a
  `#[cfg(not(windows))]` one asserting `Sandbox::create` and
  `EmbeddedServer::with_run(root, RunAccess::Allowed)` both fail loudly and that no catalogue
  contains a `proc_`-prefixed name. `the_connector_lists_the_three_tools_with_their_derived_schemas`
  keeps its body unchanged.
- **T9 · RED→GREEN — `skein-cli` wiring.** `RunArgs` and `RunArgs::resolve` in `wiring.rs`;
  `ToolArgs::transport(&self, run)` and `ToolArgs::agent_policy(&self, run)`; `RunArgs` flattened
  into `main.rs`'s `AcpAgent` variant only; `acp::serve` calling `run.resolve()?` before
  `Silo::open`; `chat.rs` passing `RunAccess::Denied`. `chat_policy` untouched. One test that
  `skein acp-agent --help` documents `--allow-run` and one that `skein chat --help` does not.
- **T10 · RED→GREEN — the governed end-to-end pair.** Two `#[cfg(windows)]` tests appended to
  `crates/skein-cli/tests/cli_acp_agent.rs` (V3), reusing `StubProvider`, `tool_call_reply`,
  `last_message`, `logged_kinds`, `temp_root`, `root_arg`, `skein` **verbatim**. `run_answering`
  gains one trailing parameter for extra CLI arguments — or, if that would churn the two existing
  call sites, a sibling `run_answering_with_args` that `run_answering` delegates to — so
  `--allow-run` can be passed without editing slice 018's tests.
- **T11** One `#[ignore]`d live-model test gated on `SKEIN_LIVE_MODEL`, mirroring
  `governed_fs_run.rs::a_live_model_calls_a_real_fs_tool`, so the hand-verification is repeatable
  rather than a one-off.
- **T12** Gates; dependency drift (state the **measured** package delta for `windows` + `win32job` on
  the Windows leg, and confirm it is zero on the other two); control diff —
  `git diff dev --stat -- crates/skein-silo/ crates/skein-core/ crates/skein-gateway/
  crates/skein-mcp/ spikes/ .github/ rust-toolchain.toml` must be empty; close-out with
  `## Observed red` per step.
- **T13** Hand-verification against live Ollama with `--allow-run`. **Not part of the implementation
  run**; performed separately and recorded under `## Live verification` in `tasks.md`.

---

## Validation

### Project gates (unchanged; all three must pass)

`cargo fmt --all --check` · `cargo clippy --workspace --all-targets -- -D warnings` ·
`cargo test --workspace`, on `windows-latest`, `macos-latest` and `ubuntu-latest` via
`.github/workflows/core.yml`, which this slice does not modify.

### New tests

- **V1 — a sandboxed process cannot write outside its root** (`escape.rs`). A root and a **sibling**
  temp directory. Sandboxed: `cmd.exe /c copy <root>\seed.txt <outside>\escaped.txt`. Assert
  `!outside.join("escaped.txt").exists()` — **the file's absence on disk is the proof**, exactly
  `governed_fs_run.rs`'s and slice 018's pattern, with a real filesystem check as ground truth rather
  than a mocked API. **Positive control in the same test**: the identical argv through plain
  `std::process::Command` **does** create the file, which is then deleted. The control is not
  padding — without it a mistyped `copy` invocation makes the test pass for the wrong reason.
- **V2 — a sandboxed process cannot reach the network** (`escape.rs`). Bind a `TcpListener` on
  `127.0.0.1:0`, take the port, spawn an accept thread. Sandboxed:
  `curl.exe --max-time 3 --silent http://127.0.0.1:<port>/`. Assert the **listener never accepted a
  connection** — the accepted-connection count is the ground truth, mirroring V1's absent file — and
  that the child's exit code is nonzero. **Positive control**: the identical argv unsandboxed **does**
  produce an accepted connection. Two design constraints, both from fact 22: a blocked AppContainer
  loopback connect **times out rather than failing fast**, so `--max-time 3` is load-bearing and the
  whole test must sit inside the sandbox's own 30 s timeout; and disabling the machine's firewall
  makes the block stricter rather than weaker, so the test does not depend on firewall state.
  `spec.md` must record that this proves *a* real network denial hermetically and does **not**
  separately prove internet denial, because no hermetic test can — the code-level fact behind that is
  `CapabilityCount: 0` (D4 step 3), which `run_server.rs` also asserts directly.
- **V3 — the governed chain, end to end through the real binary** (`cli_acp_agent.rs`,
  `#[cfg(windows)]`), reusing slice 018's harness:
  - `an_acp_client_that_allows_lets_a_real_proc_run_execute` — `StubProvider` scripted with
    `tool_call_reply("proc_run", json!({"command":"cmd.exe","args":["/c","type","seed.txt"]}))` and
    `PermissionOptionKind::AllowOnce`. Assert exactly one `RequestPermissionRequest`, whose
    `tool_call_id` and `title` are `"proc_run"` and whose options are the two `skein.*` ids in order;
    assert `last_message` starts `[tool_result tool=proc_run status=ok]` and contains the seed file's
    real bytes; assert `logged_kinds` is the 12-step allow shape; assert `skein ledger verify` exits
    0 reporting `12 steps`.
  - `an_acp_client_that_rejects_stops_the_proc_run_and_the_run_survives` — `RejectOnce`, with a
    command whose *effect* would be visible (`cmd.exe /c copy seed.txt planted.txt` inside the root).
    The proof is `!fs_root.join("planted.txt").exists()`; plus `status=denied`, the 11-step chain,
    `11 steps`, and `StopReason::EndTurn`.
- **V4 — the command line round-trips** (`argv.rs`). A table of adversarial argv — embedded quotes,
  trailing backslashes, `a"b`, `a\\`, spaces, the empty string, `&`, `|`, `>` — is built into a
  command line and fed back through the real `CommandLineToArgvW`; assert the parsed vector equals
  the input. A real Win32 parser as the oracle, not a hand-written mirror of the quoting rules.
- **V5 — the Job Object kills the tree** (`escape.rs`). Sandboxed
  `cmd.exe /c ping.exe -n 60 127.0.0.1` with a 2 s timeout; assert `run` returns the timeout `Err`
  within a few seconds. A leaked descendant would hold the pipe write end open and hang the reader
  join, so a test that *completes* is itself the assertion; additionally assert the elapsed time is
  under 10 s so a hang fails rather than merely being slow.
- **V6 — the tool is absent where it is not supported** (`connector.rs`).
  `#[cfg(not(windows))]`: `Sandbox::create` returns the platform `Err`;
  `EmbeddedServer::with_run(root, RunAccess::Allowed)` returns an `Err` naming the platform rather
  than succeeding with a silently missing tool; `local_connector(root)?.list()` contains no name
  starting with `proc_`. `#[cfg(windows)]`: `local_connector_with_run(root, RunAccess::Denied)`
  advertises exactly the three names it advertises on `dev`.
- **V7 — the caps and the refusals** (`run_server.rs`). A command producing more than
  `RUN_OUTPUT_BYTE_CAP` on stdout is truncated **and labelled with the dropped byte count**; 65
  arguments is a named refusal; a `command` naming a path outside the root is a named refusal; a
  `command` that resolves nowhere names both places it looked; a nonzero exit is `exit <n>` inside an
  `Ok`, never an `Err`; a `RunParams` whose `args` is not an array of strings fails at the typed
  boundary and reaches the model as `isError: true`.

**No padding.** Every test above either proves a Constitution invariant (V1, V2, V3, V6), proves a
mechanism those invariants rest on (V4, V5), or pins a documented cap (V7).

---

## Risks and rollback

**Blast radius.** One new crate; one new `#[cfg(windows)]` module and one new `#[tool]` method in
`skein-connectors`; three additions in `skein-cli` (`RunArgs`, and a `run` parameter on `transport`
and `agent_policy`). `skein-core`, `skein-acp`, `skein-silo`, `skein-gateway`, `skein-mcp`,
`spikes/`, `.github/` and `rust-toolchain.toml` are untouched. No existing test assertion changes
(D11).

| Risk | Why it might bite | Response |
|---|---|---|
| The AppContainer cannot traverse into a `TempDir` root | User-profile subtrees carry no `ALL APPLICATION PACKAGES` ACE (fact 23); if AppContainer tokens did **not** retain `SeChangeNotifyPrivilege`, per-ancestor traverse rights would be needed | **T4 is deliberately the earliest behavioural step precisely to find this out.** Fallback: a traverse-only (`FILE_TRAVERSE`, `NO_INHERITANCE`) ACE on each ancestor of the root, which changes D7 and must be written into `spec.md` rather than done quietly |
| `curl.exe` absent on a runner | Ships with Windows 10 1803+ and Server 2019+, and with `windows-latest` | If it ever is not, swap V2's client to `ping.exe -n 1 127.0.0.1` (ICMP is equally capability-gated) and record the swap in `tasks.md` |
| Reader-thread deadlock on a child that fills a pipe | The classic `CreateProcess` trap | D10's readers drain past the cap and never stop early, and the parent closes its copies of the write ends immediately after `CreateProcessW`; V5's elapsed-time assertion turns a leak into a failing test rather than a silent pass |
| `windows` 0.61 vs 0.62 divergence | `win32job` pins `^0.61` | D2 pins 0.61 workspace-wide; every signature in facts 15–19 was read from the 0.61.0 source, and fact 21 removes the type-crossing hazard entirely |
| Orphaned AppContainer profiles accumulate | The profile is deterministic per root and never deleted — deleting on drop would race concurrent sessions over the same root | Recorded residual; `DeleteAppContainerProfile` and `CheckNetIsolation.exe -s` named in `spec.md` as the manual cleanup |
| The granted ACE outlives the profile | Deleting a profile leaves an ACE naming an unresolvable SID | Recorded residual; harmless (an unresolvable SID grants nobody anything) but stated rather than discovered |
| First `unsafe` in the workspace | Nothing here has needed a memory-safety review before (fact 11) | Confined to one crate (D1); every FFI signature verified against source this session; every raw allocation — `SetEntriesInAclW`'s ACL, `ConvertStringSidToSidW`'s SID, `GetNamedSecurityInfoW`'s descriptor, the proc-thread attribute list, `CreateAppContainerProfile`'s PSID — paired with its documented free (`LocalFree` / `FreeSid` / `DeleteProcThreadAttributeList`) in the same function |
| A model reaches a shell by naming `cmd.exe` | D9 | Stated, not hidden; the boundary is the sandbox and the per-call human approval, and a blocklist would be theatre |

**Rollback.** `git revert` the merge; or delete `crates/skein-sandbox/`, the two
`[workspace.dependencies]` entries, the `#[cfg(windows)] mod run;` and its `#[tool]` method, and the
three `skein-cli` additions. The AppContainer profiles and root ACEs a run created **survive the
revert** and are removed by hand; `spec.md` says so and names the two commands.

---

## Out of scope

Deliberately not done, so nobody helpfully does it.

- **Any Linux or macOS backend.** Landlock and Seatbelt are ADR-0006's named future work, each its
  own separately-scoped slice. Not stubbed, not started, not trait-shaped for three OSes.
- **A cross-OS sandbox trait.** ADR-0005's surviving point: one trait implemented three ways on day
  one is a subsystem, not a slice. `Sandbox` is a concrete type with one backend. Extracting a trait
  is the second backend's job, when there is a second implementation to generalize from.
- **`CreateRestrictedToken`, LPAC (`PROC_THREAD_ATTRIBUTE_ALL_APPLICATION_PACKAGES_POLICY`), and any
  named capability SID.** D3 and D4.
- **A second tool of any kind** — no `proc_kill`, no `proc_status`, no streaming output, no
  background or detached runs, no job control. Principle VII.
- **Shell syntax in any form** — pipes, redirection, `&&`, globbing, variable expansion,
  multi-command scripts, interactive stdin. D9.
- **An operator-configured executable allowlist / `--run-dir`.** Named in D8 as the next slice; not
  built here.
- **Arbitrary environment injection.** The child gets a fixed minimal block; `RunParams` has no
  `env`.
- **Memory and process-count Job limits.** D5, with the win32job API fact (20) that forces it.
- **Loopback exemptions** (`NetworkIsolationSetAppContainerConfig`, `CheckNetIsolation`,
  `Add-AppModelLoopbackException`). Constitution II is NON-NEGOTIABLE, and the whole point of V2 is
  that loopback stays blocked.
- **`--allow-run` on `skein chat`.** D11.
- **Deriving `ToolAccess` from MCP tool annotations.** Classification stays operator configuration.
- **Slice 016's TOCTOU residual.** Unchanged and still open; `Sandbox::create` inherits it.
- **`crates/skein-silo/`, `crates/skein-core/`, `crates/skein-gateway/`, `crates/skein-mcp/`,
  `spikes/` (ADR-0004 D2), `.github/`, `rust-toolchain.toml`.**
