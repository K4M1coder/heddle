# Tasks: a Windows-only sandboxed process launcher and one `proc_run` tool (v0 slice)

**Spec:** `specs/019-shell-connector-windows/spec.md` · **Plan:**
`specs/019-shell-connector-windows/plan.md` · TDD (red→green), branch `019-shell-connector-windows`
cut from `dev` at `b82f37a`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the launcher is a library crate with no CLI of its own and the tool is a method
  on the existing embedded server; `skein acp-agent` gains one flag and stays the authoritative
  client · II Local-first ✅ NON-NEGOTIABLE and **strengthened**: the AppContainer profile carries
  zero capability SIDs and the launch passes `CapabilityCount: 0`, so WFP has no permit filter to
  match on and the sandboxed child reaches no network at all — proven hermetically against a
  loopback listener in the test process, with an unsandboxed positive control
- III Test-First ✅ every step's red was observed and recorded verbatim under `## Observed red`
  before its green. T4 is deliberately the earliest behavioural step because it validates the whole
  DACL/traversal model at once, and its red had to be an unwritten-code red rather than an ACL one
  · IV Inverted coupling ✅ `skein-core` gains nothing and depends on nothing new. `skein-sandbox`
  depends on no Skein crate at all — it is a leaf — and `skein-connectors` reaches it the way it
  reaches `git2`: one module, one `#[cfg]`, no type of the dependency in any public signature
- V Traceability ✅ unchanged machinery, newly exercised: a `proc_run` call lands `ToolCall` →
  `Approval` → `ToolResult` on the chain like any other, and both governed end-to-end runs are read
  back **in a second process** through `skein ledger verify` at 12 and 11 steps. No new `StepKind`
- VI Security ✅ **the principle this slice is shaped by.** Deny-by-default is structural and not
  merely policy: two independent opt-ins (`--fs-root` *and* `--allow-run`), a server route disabled
  unless both are present, a CLI allowlist that must agree with it, and then the per-call human
  approval slice 018 proved live. The containment claim is stated exactly as narrow as it is
  provable — cannot *write* outside the root, not cannot *read* anything outside it
- VII Neutrality ✅ one crate, one tool, one flag. `proc_kill`, `proc_status`, streaming output, a
  cross-OS trait, a shell-binary blocklist, a `--run-dir` allowlist and two Job limits were each
  considered and rejected with a reason in `spec.md`
- VIII Loop discipline ✅ NON-NEGOTIABLE and untouched. Every refusal and every timeout is an
  `Err(String)` the model is told about and the run survives; a nonzero exit is an `Ok` because the
  process ran and the model needs the output
- Cross-platform ⚠️ **This slice is intentionally Windows-only.** ADR-0006 authorizes shipping
  `shell` on one OS first; the Constitution's "no OS-specific call without `#[cfg]` + an equivalent"
  is met on the `#[cfg]` and **not** on the equivalent, which is deferred to a Linux (Landlock) and
  a macOS (Seatbelt) slice each. On the macOS and Linux CI legs `skein-sandbox` compiles to a crate
  whose only reachable behaviour is a loud refusal, and `proc_run` is absent from every catalogue —
  verified by a `#[cfg(not(windows))]` test that runs on two of the three legs.

## Tasks
- [x] **T0** `specs/019-shell-connector-windows/{spec.md,plan.md,tasks.md}`; branch
      `019-shell-connector-windows` cut from `dev` at `b82f37a`
- [x] **T1** control baseline: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace`, each re-measured rather than quoted
- [x] **T2** manifests, before any behaviour: `windows` and `win32job` in `[workspace.dependencies]`,
      `crates/skein-sandbox` with the D6 signatures and `todo!()` Windows bodies
- [x] **T3** RED→GREEN — the AppContainer profile and the ACL grant (`tests/profile.rs`)
- [x] **T4** RED→GREEN — the launcher walking skeleton (`tests/launch.rs`)
- [x] **T5** RED→GREEN — the escape reproductions (`tests/escape.rs`)
- [x] **T6** RED→GREEN — argv quoting (`src/argv.rs`'s own test module; see **Deviations**)
- [x] **T7** RED→GREEN — the tool (`skein-connectors`, `tests/run_server.rs`)
- [x] **T8** RED→GREEN — the absence gates (`tests/connector.rs`)
- [x] **T9** RED→GREEN — `skein-cli` wiring
- [x] **T10** RED→GREEN — the governed end-to-end pair (`cli_acp_agent.rs`)
- [x] **T11** one `#[ignore]`d live-model test gated on `SKEIN_LIVE_MODEL`
      (`tests/governed_proc_run.rs`)
- [x] **T12** gates, dependency drift, control diff, close-out
- [ ] **T13** hand-verification against live Ollama — **not part of the implementation run**, and
      not performed. `tests/governed_proc_run.rs::a_live_model_calls_a_real_proc_run` is the
      repeatable form of it; run it with `$env:SKEIN_LIVE_MODEL` set and record the result under a
      `## Live verification` heading here.

## Control baseline (T1)

On `019-shell-connector-windows` @ `b82f37a`, working tree clean, Windows 11 Pro 10.0.26200,
toolchain 1.97, 2026-09-03, before any edit:

- `cargo fmt --all --check` — clean, no output, exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, `Finished dev profile`, exit 0.
- `cargo test --workspace` — **193 passed, 0 failed, 3 ignored**: `acp_session` 16,
  `cli_acp_agent` 10, `cli_chat` 12, `cli_ledger` 8, `cli_secret` 2, `connector` 6, `fs_root` 10,
  `fs_server` 7, `git_root` 5, `git_server` 13, `governed_fs_run` 4 (+1 ignored), `governed_git_run`
  4 (+1 ignored), `core` 19, `native_loop` 25, `tool_gateway` 14, `governed_run` 2, `openai_compat`
  15 (+1 ignored), `rmcp_gateway` 9, `silo_ledger` 7, `silo_secret` 5. Every `src/lib.rs` and
  `src/main.rs` unit target reports 0.

Slice 018's close records 191 at `4eeea42`; the delta of +2 is slice 018's own two tests, which is
the expected figure and is why the baseline is re-measured rather than quoted.

## Observed red

**T3** — `cargo test -p skein-sandbox --test profile`, both tests:

```
thread 'a_sandbox_derives_an_appcontainer_sid_and_grants_it_the_root' panicked at
crates\skein-sandbox\src\lib.rs:98:9:
not yet implemented: T3
test result: FAILED. 0 passed; 2 failed
```

An **unwritten-code** red — the `todo!("T3")` T2 deliberately left in `Sandbox::create` — and not an
ACL or a Win32 one. That distinction is the whole reason T2 exists as its own step. Getting there
took two compile errors in the *test's* own Win32 helper, both fixed before the red was recorded:
`HLOCAL` implements neither `From<*mut u16>` nor `From<*mut c_void>` in windows 0.61, so the frees
are written `HLOCAL(text.0 as *mut c_void)` rather than `.into()`.

**T4** — `cargo test -p skein-sandbox --test launch`, both tests:

```
thread 'a_sandboxed_process_reads_a_file_in_its_granted_root' panicked at
crates\skein-sandbox\src\lib.rs:153:9:
not yet implemented: T4
test result: FAILED. 0 passed; 2 failed
```

The plan makes this step's red the slice's one stop condition — *"if this step is red for an ACL or
traversal reason rather than an unwritten-code reason, stop and fix the model"*. It was the
`todo!("T4")`, so the model stood and the fallback (a traverse-only `FILE_TRAVERSE`/`NO_INHERITANCE`
ACE on each ancestor) was **not needed**: an AppContainer token retains
`SeChangeNotifyPrivilege`, so one inheritable ACE on the root is enough to reach a file inside a
`TempDir` under the user profile. D7 stands as written.

Three genuine code defects surfaced between that red and green, each measured rather than reasoned
about, and each recorded because the failure mode names nothing like the cause:

1. **`ERROR_INVALID_PARAMETER` (0x80070057).** `HANDLE_FLAG_INHERIT` had been cleared on all three
   pipe *read* ends, but `stdin_read` is the end the **child** needs — and `CreateProcessW` refuses a
   launch outright when a handle named by `STARTF_USESTDHANDLES` is not inheritable. `Pipes` is now
   split by *who ends up owning each end* rather than by which direction it points, and the three the
   parent keeps (`stdin_write`, `stdout_read`, `stderr_read`) are the three made private.
2. **`ERROR_INVALID_PARAMETER` again, after that fix.** `UpdateProcThreadAttribute` **stores the
   pointer it is given and does not copy the value**, so the stack-local `SECURITY_CAPABILITIES` was
   dangling by the time `CreateProcessW` read it. It is now a `Box` held by `Attributes`, so moving
   that struct — which happens as soon as it is returned — does not move the memory the list points
   at.
3. **`ERROR_ENVVAR_NOT_FOUND` (0x800700CB).** Bisected against the parent environment: the launch
   needs **`LOCALAPPDATA`** in the block handed to the child, because an AppContainer's per-package
   state lives under `%LOCALAPPDATA%\Packages\<profile name>\` and process creation resolves that
   path from the child's own environment. Sorting the block case-insensitively is required too — a
   block is searched, not scanned. Neither is in the plan; both are now in the code's own comments.

**T5** — `cargo test -p skein-sandbox --test escape -- --test-threads=1`:

```
test a_sandboxed_process_cannot_reach_the_network ... ok
test a_sandboxed_process_cannot_write_outside_its_root ... ok
test the_job_object_kills_the_tree_when_the_clock_runs_out ... FAILED
```

**Stated plainly rather than dressed up: two of the three gates were green on their first run.** T4's
green is what implements the mechanism they exercise, and the plan orders T4 before T5, so there was
no unwritten code left for them to fail against. Their value is not a red — it is that each has its
unsandboxed positive control passing in the same test, so neither can go green for the wrong reason
later.

The third was a real red, for a reason worth keeping:

```
a 60-second ping under a 2-second limit must be refused: Run { exit_code: 1,
  stdout: "Impossible de contacter le pilote IP. Défaillance générale." }
```

**`ping.exe` inside an AppContainer fails immediately — ICMP is capability-gated exactly as TCP is.**
So the plan's own V5 command exits in milliseconds instead of running for a minute, and its stated
V2 fallback (*"swap V2's client to `ping.exe`"*) would have proven denial but never a timeout. The
stopwatch is now a `cmd.exe`-launching-`cmd.exe` counting loop, which needs no network and puts a
real grandchild in the job. `timeout.exe` was rejected for a different reason: it refuses to run with
redirected input, and every stream here is a pipe.

Verified separately that the kill is real and not merely fast: `tasklist /FI "IMAGENAME eq cmd.exe"`
counts **16 before the suite and 16 after**, so no descendant outlives the job.

**T6** — folded into T4's commit rather than run as a separate step, because the argv builder is what
`Sandbox::run` calls first: `launch.rs` could not compile without it. The four tests are
`#[cfg(test)] mod tests` inside `src/argv.rs` rather than the `tests/argv.rs` the plan names — see
**Deviations** below — and all four passed on their first run, against the real
`CommandLineToArgvW` as the oracle.

**T7** — `cargo test -p skein-connectors --test run_server`:

```
error[E0432]: unresolved imports `skein_connectors::RunAccess`, `skein_connectors::RunParams`,
              `skein_connectors::RUN_OUTPUT_BYTE_CAP`
error[E0432]: unresolved import `skein_sandbox`
error[E0599]: no associated function or constant named `with_run` found for struct `EmbeddedServer`
error[E0599]: no method named `proc_run` found for reference `&EmbeddedServer`
```

A compile red naming all five things the step adds. One assertion then failed for a **fixture**
reason worth keeping: `..\escape.exe` was refused with *"the specified file cannot be found"*, not
*"resolves outside the root"* — the containment check never ran, because `FsRoot::resolve`
canonicalizes first and a nonexistent path fails there. The fixture now puts the root in a
**subdirectory** with a real `outside.exe` beside it, which is `fs_server.rs`'s own shape for its own
reason. A test that had accepted either message would not have noticed containment breaking.

**T8** — no red, and the reason is structural rather than an omission: the plan orders T8 after T7,
and T8's Windows tests name `local_connector_with_run`, so they cannot compile — let alone fail an
assertion — before T7's green exists. What matters here was measured instead, and it is FR-016's
stop condition: **`cargo test -p skein-connectors` immediately after T7's green, before T8 was
written, passed 6/6 in `connector` and 7/7 in `fs_server` with every assertion byte-identical to
`dev`** — including `the_connector_lists_the_three_tools_with_their_derived_schemas` and
`the_connector_lists_the_git_tools_only_when_the_root_is_a_repository`. The gate holds; no
pre-existing expectation had to move.

Writing T8's `#[cfg(not(windows))]` test did surface one real defect by review, since it cannot be
compiled on this machine (see **Deviations**): `Result::expect_err` requires `T: Debug`, and neither
the off-Windows `Sandbox` — an uninhabited type — nor `EmbeddedServer` derives it. The test matches
on the result through a local helper instead of asking the product for a trait only a test wants.

**T9** — `cargo test -p skein-cli --test cli_acp_agent acp_agent_documents`:

```
test acp_agent_documents_the_fs_root_flag ... ok
test acp_agent_documents_the_allow_run_flag_and_chat_does_not ... FAILED
  an opt-in capability an operator cannot discover is not opt-in, got:
test result: FAILED. 1 passed; 1 failed
```

The one assertion that can be red before the flag exists, and it was. After the wiring, both pass —
and `--allow-run` appears in `skein acp-agent --help` on **every** platform, deliberately: the flag
is present and refuses loudly on Linux and macOS rather than being silently absent, which is what
"fail clearly, never silently degrade" means for a flag.

**T10** — no red, for T8's structural reason: both tests name `run_answering_with_args` and
`--allow-run`, so neither could compile before T7's and T9's greens. What they contribute is the one
claim nothing else in the slice makes, and it is asserted on a real effect rather than a status
string: the model is told `[tool_result tool=proc_run status=ok]` **containing the seed file's own
bytes**, which only an actually-launched sandboxed process could have produced. The measured control
is that slice 018's two `fs_write` tests, which pass no `--allow-run`, stayed green throughout —
`cli_acp_agent` went 11/11 with the harness change and before either new test was added.

**T11** — no red and none possible: an `#[ignore]`d test asserts nothing until a human sets
`SKEIN_LIVE_MODEL`. Verified only that it compiles and is skipped.

## Gates (T12)

On `019-shell-connector-windows`, Windows 11 Pro 10.0.26200, toolchain 1.97, 2026-09-03:

- `cargo fmt --all --check` — clean, exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, exit 0. **No `#[allow]` was added
  anywhere in this slice**, which for a first-`unsafe` crate is worth stating.
- `cargo test --workspace` — **216 passed, 0 failed, 4 ignored**, against the T1 baseline of 193/0/3:
  `acp_session` 16, `cli_acp_agent` 13 (+3), `cli_chat` 12, `cli_ledger` 8, `cli_secret` 2,
  `connector` 8 (+2), `fs_root` 10, `fs_server` 7, `git_root` 5, `git_server` 13, `governed_fs_run`
  4 (+1 ignored), `governed_git_run` 4 (+1 ignored), `governed_proc_run` 0 (+1 ignored, **new**),
  `run_server` 7 (**new**), `core` 19, `native_loop` 25, `tool_gateway` 14, `governed_run` 2,
  `openai_compat` 15 (+1 ignored), `rmcp_gateway` 9, `skein_sandbox` unit 4 (**new**, the argv
  module), `escape` 3 (**new**), `launch` 2 (**new**), `profile` 2 (**new**), `silo_ledger` 7,
  `silo_secret` 5. **+23 passed, +1 ignored, and every pre-existing count unchanged.**

## Dependency drift (T12), measured

`cargo tree --workspace --target <triple> --prefix none`, deduplicated, against `dev`:

- **`x86_64-pc-windows-msvc`: 155 to 168, so +13.** One is `skein-sandbox` itself; the other twelve
  are `win32job` 2.0.3, `windows` 0.61.3, and the ten crates `windows` decomposes into
  (`windows-core` 0.61.2, `windows-collections` 0.2.0, `windows-future` 0.2.1, `windows-implement`
  0.60.2, `windows-interface` 0.59.3, `windows-link` 0.1.3, `windows-numerics` 0.2.0,
  `windows-result` 0.3.4, `windows-strings` 0.4.2, `windows-threading` 0.1.0). Nothing was removed.
- **`x86_64-unknown-linux-gnu`: 153 to 154, so +1**, and that one is `skein-sandbox`, whose only
  dependency is `sha2` — already in the graph. **Zero third-party packages are added off Windows**,
  and `cargo tree --target x86_64-unknown-linux-gnu | grep -iE "^(windows|win32job)"` is empty.
- **An honest correction to D2's footprint claim.** Four of those twelve (`windows-collections`,
  `windows-future`, `windows-numerics`, `windows-threading`) arrive through `windows`'s **default**
  features, which `win32job` does not disable — and Cargo unions features across dependents, so this
  crate's own `default-features = false` cannot switch them off. What the 0.61 pin actually buys is
  therefore what matters and still holds: **one version of a very large generated crate in the tree
  rather than two.** Removing those four would take a patch to `win32job`, not a change here.

## Control diff (T12)

`git diff dev --stat -- crates/skein-silo/ crates/skein-core/ crates/skein-gateway/
crates/skein-mcp/ spikes/ .github/ rust-toolchain.toml` — **empty**. `Cargo.toml` is the one
exception the plan authorizes (the two `[workspace.dependencies]` entries).

## Deviations from the plan

Each is a place the plan said one thing and the tree says another, recorded rather than smoothed
over.

1. **Off Windows, `Sandbox` is uninhabited (`struct Sandbox(Infallible)`) and `Sandbox::run`'s body
   is `match self.0 {}`.** D6 specifies that `run` return the same refusal string as `create`. It
   cannot be reached to return anything — `create` is the only constructor and always fails — and
   the plan's own prose concedes this (*"unreachable, since no `Sandbox` can exist"*). Making the
   type uninhabited states that in the type system instead of in a comment, and the compiler
   verifies it. The refusal string still lives in `create`, which is the only reachable entry point
   and the only one a `#[cfg(not(windows))]` test can assert. This choice pays off twice: it also
   lets `EmbeddedServer`'s `sandbox: Option<Arc<Sandbox>>` field carry the platform gate with **no**
   `#[cfg]`, because the option can only ever be `None` there.
2. **`Sandbox` stores the string SID as a `String`, not the `Vec<u16>` D6 names**, and converts to
   wide per launch. Same `Send + Sync`-by-construction property, one fewer representation, and
   `string_sid()` — which the plan's own T3 test description requires — becomes a borrow instead of
   a re-encode.
3. **`RUN_ARG_COUNT_CAP` lives in `skein-sandbox` as `ARG_COUNT_CAP`, not in `skein-connectors`.**
   T7 lists it among the connector's constants, but T6's own tests are in `skein-sandbox` and the
   refusal is the launcher's: it is a fact about what `CreateProcessW` can be handed, not about what
   a tool chooses to offer. One number, one home. `RUN_OUTPUT_BYTE_CAP` and `RUN_TIMEOUT` stay in
   `skein-connectors`, because those genuinely are the tool's policy and are passed in as arguments.
4. **The argv tests are `#[cfg(test)] mod tests` inside `src/argv.rs`, not `tests/argv.rs`.**
   Keeping them at unit level is what keeps `command_line` `pub(crate)`. An integration test would
   have forced the quoting builder into the public API for a test's sake, which is the sort of
   speculative surface the slice is otherwise careful not to add. The oracle is unaffected: the
   `Win32_UI_Shell` dev-dependency feature applies to the unit-test target too, so the round trip
   still runs through the real `CommandLineToArgvW`.
5. **V5's fixture is a `cmd.exe`-launching-`cmd.exe` counting loop, not `ping -n 60`** — see T5's
   observed red. `ping.exe` cannot run inside the container at all.
6. **Two facts the plan could not have had, now load-bearing in the code**: the environment block
   must contain `LOCALAPPDATA` and must be sorted, and `UpdateProcThreadAttribute` retains the
   pointer it is given rather than copying the value. Both are in T4's observed red.
7. **No `tool_descriptions()`-style accessor was added.** The caps stated in `proc_run`'s
   description are asserted through `local_connector_with_run(…, Allowed).list()` in `connector.rs`
   — the real advertisement path — rather than by adding a method to `EmbeddedServer` that no
   product code would call.

## What could not be verified on this machine

Stated rather than left implicit, because it is the one gap in this slice's evidence and it is wider
here than in any prior slice — this is the first slice whose non-Windows legs run genuinely different
code.

- **`skein-sandbox`'s non-Windows arm IS compiled and clean**, for both other targets:
  `cargo check -p skein-sandbox --target x86_64-unknown-linux-gnu --all-targets` and
  `--target aarch64-apple-darwin --all-targets` both finish with no warnings. That covers the
  uninhabited `Sandbox`, the `NO_BACKEND` refusal and `run`'s `match self.0 {}`.
- **`skein-connectors`' and `skein-cli`'s non-Windows arms are reviewed but not compiled.**
  `cargo check --target x86_64-unknown-linux-gnu` fails in `libgit2-sys`/`rusqlite` with
  `error occurred in cc-rs: failed to find tool "x86_64-linux-gnu-gcc"`, and the macOS target fails
  the same way in `libz-sys` (`failed to find tool "cc"`). Both are missing cross C toolchains, not
  code faults — they come from the vendored-C dependencies `git2` and `rusqlite` bring in. The review
  did find one real defect this way, recorded under T8: `expect_err` needs `T: Debug`, which neither
  the off-Windows `Sandbox` nor `EmbeddedServer` has. Anything else in those two arms is unproven
  until this repository has a remote and CI runs the other two legs — the standing caveat of slices
  004–018, now carrying more weight.
- **T13 was not performed**, per the run's own scope.

## Next slice

- **An operator-configured `--run-dir` allowlist.** D8's stated cost is that `cargo`, `node` and
  `python` are unreachable, so `proc_run` cannot build this project. Closing it means a flag that
  both extends the executable search list and grants the AppContainer SID read+execute on each named
  directory. That is the single largest gap between what this slice ships and what an agent needs.
- **A Linux backend (Landlock) and a macOS backend (Seatbelt)**, each its own slice, each earning
  the trait extraction this slice deliberately did not do.
- **Profile and ACE cleanup.** Nothing removes either today, by design. A `skein sandbox prune`
  subcommand — or an explicit decision that the operator owns it — is a real question this slice
  answers only with a residual.
