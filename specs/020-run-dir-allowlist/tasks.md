# Tasks: an operator-configured `--run-dir` allowlist for `proc_run` (v0 slice)

**Spec:** `specs/020-run-dir-allowlist/spec.md` · **Plan:** `specs/020-run-dir-allowlist/plan.md` ·
TDD (red→green), branch `020-run-dir-allowlist` cut from `dev` at `09d61f8`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ no CLI of its own; `skein acp-agent` gains one repeatable flag and stays the
  authoritative client. The rule that decides which executable a `command` may name stays in
  `skein-connectors`, and the ACL grant stays in `skein-sandbox` · II Local-first ✅ NON-NEGOTIABLE
  and untouched: the AppContainer profile still carries zero capability SIDs and the launch still
  passes `CapabilityCount: 0`, so WFP has no permit filter to match on. A run directory is granted
  read and execute; nothing about the network changes, and slice 019's loopback gate with its
  unsandboxed positive control still passes
- III Test-First ✅ every step's outcome is recorded verbatim under `## Observed red`, and where a
  step had **no** red the entry says so and why rather than dressing one up. T3, T6, T7 and T8 are
  genuine reds. T2 is deliberately a compile-and-green refactor with a stop condition. T4 and T5
  had none because T3's green is the mechanism they exercise, so each carries an **in-test
  ungranted control** plus a measured counterfactual instead — which is a stronger guarantee than a
  red, because it keeps working after the fact
- IV Inverted coupling ✅ `skein-core` gains nothing and depends on nothing new. `skein-sandbox`
  remains a leaf depending on no Skein crate; `skein-connectors` still reaches it through one
  `#[cfg]`-gated module with no type of the dependency in any public signature. `RunDirs` lives in
  `skein-connectors` because it must exist on all three platforms, and `Sandbox` takes it as
  `&[PathBuf]`
- V Traceability ✅ unchanged machinery, unchanged shape: a `proc_run` call still lands `ToolCall` →
  `Approval` → `ToolResult` on the chain. No new `StepKind`, no change to `ToolGateway`,
  `Approval`, `Redactor` or `AcpPermissionTransport`
- VI Security ✅ deny-by-default stays structural: `--run-dir` is a **third** opt-in that clap
  refuses without the second (`--allow-run`), which itself needs the first (`--fs-root`), and the
  allowlist is unrepresentable without run access because it rides inside `RunAccess::Allowed`. The
  grant is narrowed from the fs-root's `GENERIC_ALL` to `GENERIC_READ | GENERIC_EXECUTE`, and that
  narrowing is proven twice — as an ACL read-back and as an effect. Nothing is auto-discovered
- VII Neutrality ✅ one flag, one newtype, no new tool and no new crate. An environment fallback,
  `--allow-cargo` / `--allow-node` convenience flags, a config file, a third `with_run` parameter, a
  `%PATH%` search, `.cmd` shim support and a run-dir listing tool were each considered and rejected
  with a reason in `plan.md`
- VIII Loop discipline ✅ NON-NEGOTIABLE and untouched. A refusal is still an `Err(String)` the
  model is told about and the run survives; a nonzero exit is still an `Ok`
- Cross-platform ⚠️ **This slice is intentionally Windows-only, and it inherits rather than amends
  ADR-0006's scope.** ADR-0006 authorizes shipping `shell` on one OS first; the Constitution's "no
  OS-specific call without `#[cfg]` + an equivalent" is met on the `#[cfg]` and **not** on the
  equivalent, which is deferred to a Linux (Landlock) and a macOS (Seatbelt) slice each. On the
  macOS and Linux CI legs `skein-sandbox` still compiles to a crate whose only reachable behaviour
  is a loud refusal, `proc_run` is still absent from every catalogue, and `RunDirs` compiles there
  because it is plain path validation in `skein-connectors` rather than anything Win32.

## Tasks
- [x] **T0** `specs/020-run-dir-allowlist/{spec.md,plan.md,tasks.md}`; branch
      `020-run-dir-allowlist` cut from `dev` at `09d61f8`
- [x] **T1** control baseline: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace`, each re-measured rather than quoted
- [x] **T2** types and signatures, no new behaviour: `RunDirs`, `RunAccess::Allowed(RunDirs)`,
      `Sandbox::create(root, run_dirs)`, `Sandbox::run_dirs()`, `grant`'s `access` parameter,
      `resolve_exe`'s ignored new argument, and every call site
- [x] **T3** RED→GREEN — the narrower grant, read back off the directory (`tests/profile.rs`)
- [x] **T4** RED→GREEN — a binary in a named run directory executes (`tests/launch.rs`)
- [x] **T5** RED→GREEN — the run directory is not writable from inside (`tests/escape.rs`)
- [x] **T6** RED→GREEN — resolution, ordering and the refusal (`tests/run_server.rs`)
- [x] **T7** RED→GREEN — the advertisement (`tests/connector.rs`)
- [x] **T8** RED→GREEN — the CLI (`cli_acp_agent.rs`, `fs_root.rs`)
- [x] **T9** the live test (`tests/governed_proc_run.rs`), gates, dependency drift, control diff
- [ ] **T10** hand-verification against live Ollama — **not part of this run.** See
      `## Live verification (T10)` below for the recorded command and pass condition.

## Control baseline (T1)

On `020-run-dir-allowlist` @ `09d61f8`, working tree clean, Windows 11 Pro 10.0.26200, toolchain
1.97, 2026-09-04, before any edit:

- `cargo fmt --all --check` — clean, no output, exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, `Finished dev profile`, exit 0.
- `cargo test --workspace` — **217 passed, 0 failed, 4 ignored**: `acp_session` 16,
  `cli_acp_agent` 13, `cli_chat` 12, `cli_ledger` 8, `cli_secret` 2, `connector` 8, `fs_root` 10,
  `fs_server` 7, `git_root` 5, `git_server` 13, `governed_fs_run` 4 (+1 ignored), `governed_git_run`
  4 (+1 ignored), `governed_proc_run` 0 (+1 ignored), `run_server` 7, `core` 19, `native_loop` 25,
  `tool_gateway` 14, `governed_run` 2, `openai_compat` 15 (+1 ignored), `rmcp_gateway` 9,
  `skein-sandbox` `src/lib.rs` unit target 4 (`argv`), `escape` 3, `launch` 3, `profile` 2,
  `silo_ledger` 7, `silo_secret` 5. Every other `src/lib.rs` and `src/main.rs` unit target reports 0.

Slice 019's close records 193 at `b82f37a`, before its own tests landed; the delta of +24 is slice
019's own additions, which is why the baseline is re-measured rather than quoted.

## Observed red

**T3** — `cargo test -p skein-sandbox --test profile`, the one new test:

```
thread 'a_run_dir_is_granted_read_and_execute_and_the_root_is_not' panicked at
crates\skein-sandbox\tests\profile.rs:185:5:
the run directory's DACL must name the AppContainer SID at all, got
[("S-1-5-21-1203453866-3760803099-1050353712-1008", 1245631),
 ("S-1-5-21-1411561155-2461164688-2535433281-4238526846", 1245631),
 ("S-1-5-18", 2032127), ("S-1-5-32-544", 2032127),
 ("S-1-5-21-4080930094-269924791-1978800073-2222", 2032127)]
test result: FAILED. 2 passed; 1 failed
```

The expected red and not an ACL or a Win32 one: T2 threaded the list through `Sandbox::create` and
deliberately granted nothing with it, so the run directory carries its five ordinary ACEs and no
AppContainer one. The mask dump in the failure message is the helper doing its job — those five are
the two users, SYSTEM, Administrators and the owner, at `0x1301FF` and `0x1F01FF`, all normalised.

One **stated deviation** from the plan's control diff, made to get this red at all: the test needs
`FILE_GENERIC_READ`, `FILE_READ_DATA`, `WRITE_DAC` and their neighbours, which live behind the
`windows` crate's `Win32_Storage_FileSystem` feature. That feature was added to `skein-sandbox`'s
existing `[target.'cfg(windows)'.dev-dependencies]` `windows` entry, beside the `Win32_UI_Shell` the
argv oracle already needs. **No product dependency and no product feature changed** — the plan's
"`skein-sandbox`'s dependency set is untouched" holds for `[dependencies]`. The rejected alternative
was hand-copied hex constants in a security test, which is exactly the kind of second source of
truth this codebase refuses.

**T4** — no unwritten-code red, and the reason is worth more than the red would have been.

The plan orders T3 before T4, and T3's green *is* the mechanism T4 exercises, so there was nothing
left unimplemented for it to fail against. Written as the plan specifies — copy `cmd.exe` to
`<run dir>\toolchain.exe`, `Sandbox::create(root, &[toolbin])`, `sandbox.run(&tool, …)` — it passed
on its first run. So the grant was removed from the fixture and it was run again:

```
test a_binary_in_an_allowlisted_run_dir_executes_and_its_stdout_comes_back ... ok
```

**It passes with no grant at all.** The plan's premise for that test — *"A `TempDir` carries no
`ALL APPLICATION PACKAGES` ACE, so a pass is attributable to the new grant and nothing else"* — does
not hold, and the reason generalises. See `## Finding: the grant is not what makes a run directory
launchable` below. The test now asserts what is actually attributable, with its ungranted control in
the same test, following `escape.rs`'s recorded discipline.

**T5** — no unwritten-code red, for T4's structural reason: T3's green is the mechanism this
exercises. Its value is the two controls, and both were measured rather than assumed.

The in-test control passes — a `copy` into the fs-root lands — so the refusal below is about the run
directory's mask and not about a mistyped `copy`. And the counterfactual was run: with
`RUN_DIR_ACCESS` temporarily widened to `GENERIC_ALL`, the same test fails, which is what makes the
green attributable to the narrower mask rather than to anything else in the container:

```
a run directory is for reaching an executable, not for writing to; it wrote
D:\Users\cthedrez\AppData\Local\Temp\.tmpsoIZjg\escaped.txt
test result: FAILED. 0 passed; 1 failed
```

`profile.rs` reads the absent write bit off the descriptor; this reads it off the filesystem. Two
independent proofs of one claim, which is what `escape.rs` already documents about itself.

**T6** — `cargo test -p skein-connectors --test run_server`, the two new resolution tests:

```
thread 'a_bare_name_in_an_allowlisted_run_dir_resolves_and_runs' panicked at
crates\skein-connectors\tests\run_server.rs:207:10:
a bare name in a named run directory is not a refusal: "toolchain.exe is in neither
C:\windows\System32 nor C:\windows; %PATH% is deliberately not searched, so name an executable in
one of those two directories or a path relative to the configured root"
test result: FAILED. 8 passed; 2 failed
```

The expected red: T2's `resolve_exe` ignores its `run_dirs` argument, so the message is still the
two-directory one. `system32_still_wins_over_a_run_dir_that_shadows_it` passed in the same run and
is a regression guard rather than a red — it asserts an order that already held and must keep
holding.

After the green, `a_command_that_resolves_nowhere_names_both_places_it_looked` still passes with its
assertion text **byte-identical**: it pins `System32` and `PATH`, both of which the new enumerating
message carries. FR-012's stop condition never fired.

**T7** — `cargo test -p skein-connectors --test connector`:

```
thread 'the_advertised_description_names_the_allowlisted_directories' panicked at
crates\skein-connectors\tests\connector.rs:380:5:
a model cannot ask for what it is not told it can reach: Run one program inside a Windows sandbox
over the configured root, … Each output stream is truncated at 16384 bytes with a note saying how
much was dropped.
test result: FAILED. 8 passed; 1 failed
```

The static `#[tool(description = …)]` string, reaching the model unchanged — which is the whole
point of the step. rmcp 2.2.0's `ToolRouter::map` and `ToolRoute::attr` are `pub` as the plan's fact
12 records, and mutating an already-registered route's description worked first time.

**T8** — `cargo test -p skein-cli --test cli_acp_agent run_dir`, all three:

```
thread 'run_dir_without_allow_run_is_an_exit_code_naming_both_flags' panicked at
crates\skein-cli\tests\cli_acp_agent.rs:1369:5:
the refusal must name both flags: error: unexpected argument '--run-dir' found
Usage: skein acp-agent --silo <ID> --model <NAME> --root <PATH> --fs-root <PATH>
test result: FAILED. 0 passed; 3 failed
```

Clap refusing a flag that does not exist yet — the expected red, and against the **real binary**
rather than the parser in-process.

`a_run_dir_that_is_not_a_directory_is_a_loud_refusal` in `fs_root.rs` had **no** red: T2 built
`RunDirs` with a real body rather than a `todo!()`, deliberately, because it is plain path
validation and a panic there would have made every later red ambiguous. That is the plan's own T2
decision and this is its consequence, stated rather than hidden.

One thing the plan does not mention and the non-Windows CI legs would have caught: `RunDirs` is only
named by `RunArgs::resolve`'s Windows arm, so its import needs the same `#[cfg(windows)]` or the
Linux and macOS legs fail `clippy -D warnings` on an unused import. The
`--run-dir`-without-`--allow-run` check is deliberately **outside** the `#[cfg]`, because that
refusal is right on every platform.

## Finding: the grant is not what makes a run directory launchable

**This contradicts a premise the plan's D4 and D7 rest on, and a claim slice 019's `spec.md` already
shipped. It is reported rather than worked around, and the D4 question it opens is left open.**

Measured on this machine, Windows 11 Pro 10.0.26200, with a scratch integration test since deleted.
`icacls` confirms the plan's facts 15 and 16 exactly: neither
`D:\Users\cthedrez\.rustup\toolchains\1.97-x86_64-pc-windows-msvc\bin` nor `C:\Program Files\nodejs`
carries any `ALL APPLICATION PACKAGES` or AppContainer ACE. With
`Sandbox::create(root, &[])` — no run directory granted, nothing's ACL touched:

```
RAN   D:\Users\cthedrez\.rustup\toolchains\1.97-…\bin\cargo.exe -> exit 0 stdout "cargo 1.97.1 (c980f4866 2026-06-30)\n"
RAN   C:\Program Files\nodejs\node.exe                          -> exit 0 stdout "v24.14.0\r\n"
```

**The cause is structural, not machine-specific.** `Sandbox::run` calls `CreateProcessW` from the
*parent* process, whose token is the ordinary user. The image file is opened under the parent's
rights, before the AppContainer token exists. The AppContainer's DACL governs only what the child
does for itself. So slice 019's `spec.md` point 5 — *"`cargo`, `node` and `python` … would not launch
even if the search found them, for want of an `ALL APPLICATION PACKAGES` ACE"* — is false. What was
really keeping them unreachable was `resolve_exe`'s search list alone.

The grant is **not** inert, and that half is measured too. Everything the child does for itself does
need it:

```
UNGRANTED  cmd /c type <run dir>\secret.txt  -> exit 1  "Accès refusé."
GRANTED    cmd /c type <run dir>\secret.txt  -> exit 0  "UNGRANTED-BYTES"
```

**And the case that actually justifies writing an ACE at all** — a child finding and launching a
sibling through the `PATH` D8 puts the run directory on. Measured by neutering the grant loop in
`profile::create` while leaving the `PATH` entry in place, so the two differ in nothing else:

```
UNGRANTED  cmd /c toolchain.exe /c echo X  -> exit 1  "'toolchain.exe' n'est pas reconnu…"
GRANTED    cmd /c toolchain.exe /c echo X  -> exit 0  "X"
PATH-VALUE "C:\windows\System32;C:\windows;D:\…\Temp\.tmphpwo0z"   (identical in both)
```

Ungranted, the child cannot even *enumerate* the directory, so the name is not recognised however
correct the `PATH` is. This is the rustup-shim case of the plan's fact 20, and a linter invoking a
helper, and a compiler reading a library beside its own binary. All three of these are in
`launch.rs`'s test, with their ungranted controls in the same test.

One more measurement, recorded because it bounds what may be claimed: a child spawning a sibling by
**absolute path** is refused with *access denied* regardless of the mask — with
`GENERIC_READ|GENERIC_EXECUTE`, with `GENERIC_ALL` and with `FILE_ALL_ACCESS` — and it is refused
out of the **fs-root** too, which carries `GENERIC_ALL`. Since the same spawn by name through `PATH`
succeeds, that failure is about how `cmd.exe` handles a full path in `/c`, not about any ACE, and no
wider grant would change it.

**What this does and does not change.**

- D1, D2, D3, D5, D7, D8, D9 and D10 are untouched. `--run-dir`'s user-visible effect is that
  `resolve_exe` searches the directory, the child's `PATH` names it, and the advertisement
  enumerates it. All of it stands, and the search list alone is what makes `cargo` reachable.
- D4's **mask** stands and is proven twice, by T3's read-back and T5's effect.
- D4's **grant** stands, but **not for the reason the plan and slice 019 give.** It is not what makes
  a run-dir binary launchable — the parent-issued `CreateProcessW` never needed it. It is what makes
  the directory usable by the child once it is running, and D8's `PATH` is inert without it. The
  headline `cargo --version` case would work with no grant at all; a toolchain that has to reach
  anything beside itself would not.
- **The open question, left open:** the grant writes a persistent ACE on a directory outside the
  workspace, one that survives `git revert` and has to be removed with `icacls` by hand. That cost
  is now weighed against a narrower benefit than the plan assumed. Whether `--run-dir` should grant
  at all, or grant only on request, is a decision this run does not take.
- Slice 019's `spec.md` point 5 carries the false half of this and is left for its own record to
  correct — amending a shipped slice's spec is outside this one's scope.

## Close-out (T9)

On `020-run-dir-allowlist`, working tree clean:

- `cargo fmt --all --check` — clean, no output, exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, no diagnostic, exit 0.
- `cargo test --workspace` — **228 passed, 0 failed, 5 ignored**. Against T1's 217/4 that is
  **+11 tests and +1 ignored**, which is exactly this slice's additions and nothing else:
  `a_run_dir_is_granted_read_and_execute_and_the_root_is_not`,
  `a_binary_in_an_allowlisted_run_dir_executes_and_its_stdout_comes_back`,
  `a_sandboxed_process_cannot_write_into_a_run_dir`,
  `a_bare_name_in_an_allowlisted_run_dir_resolves_and_runs`,
  `a_bare_name_in_a_directory_that_was_not_named_names_every_place_it_looked`,
  `system32_still_wins_over_a_run_dir_that_shadows_it`,
  `the_advertised_description_names_the_allowlisted_directories`,
  `a_run_dir_that_is_not_a_directory_is_a_loud_refusal`,
  `acp_agent_documents_the_run_dir_flag_and_chat_does_not`,
  `run_dir_without_allow_run_is_an_exit_code_naming_both_flags`,
  `acp_agent_refuses_a_run_dir_that_does_not_exist_before_serving`, and the `#[ignore]`d
  `a_live_model_runs_a_real_toolchain_binary`.
- **No pre-existing assertion's text changed.** FR-012's stop condition never fired. Only
  constructor spellings moved, at T2.
- **Control diff empty** for `crates/skein-silo/`, `crates/skein-core/`, `crates/skein-gateway/`,
  `crates/skein-mcp/`, `spikes/`, `.github/` and `rust-toolchain.toml` — verified with
  `git diff --stat 09d61f8 -- …`, which produces no output.
- **`Cargo.lock` is untouched.** Two manifests changed, both dev-only and both stated:
  `crates/skein-connectors/Cargo.toml` gains `skein-silo` under `[dev-dependencies]` (the plan's one
  named exception), and `crates/skein-sandbox/Cargo.toml` adds the `Win32_Storage_FileSystem`
  feature to its existing dev-only `windows` entry (T3's recorded deviation). **No product
  dependency and no product feature changed anywhere**, and no new `unsafe` block was added — the
  mask parameter reuses `grant`'s existing one.

## Live verification (T10)

**Not performed in this run** — it was explicitly excluded from the implementation run's scope. The
command and its pass condition are recorded here so the hand-verification is repeatable rather than
re-derived.

Target: `D:\Users\cthedrez\.rustup\toolchains\1.97-x86_64-pc-windows-msvc\bin`, command
`cargo --version`. Chosen on measurement: that directory is user-owned with `FullControl` so the
grant succeeds **without elevation**; it holds the real `cargo.exe` and `rustc.exe` plus the
`rustc_driver-*.dll` and `std-*.dll` they load, all in the one directory one inheritable ACE covers;
and `cargo --version` was measured to exit 0 under the sandbox's exact five-variable environment
block. `node --version` is the documented fallback and needs an **elevated** skein, because
`C:\Program Files\nodejs` is owned by SYSTEM and carries no AppContainer ACE. `~\.cargo\bin` is
deliberately not the target: it holds the rustup **shim**, which re-executes the real cargo under a
toolchain `bin`.

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
`ToolResult` payload contains a real `cargo 1.97.…` line produced by a real `CreateProcessW` inside
the AppContainer. The `skein ledger show` output for that step is the evidence to paste here.

If the model declines to call the tool, that is a model-selection finding and not a defect — slice
019's live section says so and this one inherits the wording.

**Performed**, after merge to `dev` at `df32492`, on the same Windows machine:

```
test a_live_model_calls_a_real_proc_run ... ok
test a_live_model_runs_a_real_toolchain_binary ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 91.71s
```

The model chose `proc_run` with exactly `{"command":"cargo","args":["--version"]}` — the bare name,
picked up by the advertisement's appended sentence naming the allowlisted directory (D9). The chain,
read back in a second process:

```
$ skein ledger verify --root $env:TEMP\skein-live-020 --silo live020
run-live-020    ok      12 steps

$ skein ledger show --root $env:TEMP\skein-live-020 --silo live020 3b0b50605c884ea9650139f70137869cb9720fc86510e72fd6f5d6cd3eebe2a5
id      3b0b50605c884ea9650139f70137869cb9720fc86510e72fd6f5d6cd3eebe2a5
parent  55e6137c3365136d10f3fde20030d80073dac7fa675ccb07d06d5a0611f3ea83
run     run-live-020
seq     6
kind    tool_result
payload {"tool":"proc_run","content":"{\"content\":[{\"type\":\"text\",\"text\":\"exit 0\\n--- stdout ---\\ncargo 1.97.1 (c980f4866 2026-06-30)\\n--- stderr ---\\n\"}],\"isError\":false}"}
```

`cargo 1.97.1 (c980f4866 2026-06-30)` is the real toolchain's real version string, produced by a real
`CreateProcessW` inside the AppContainer against the operator-named run directory, exactly as this
section's pass condition names. The model's final answer quoted it back verbatim.
