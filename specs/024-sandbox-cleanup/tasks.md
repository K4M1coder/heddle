# Tasks: `skein sandbox list` / `skein sandbox prune` (v0 slice)

**Spec:** `specs/024-sandbox-cleanup/spec.md` · **Plan:** `specs/024-sandbox-cleanup/plan.md` ·
TDD (red→green), branch `024-sandbox-cleanup` reset onto `dev` at `12c14f5`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the capability is two public free functions on `skein-sandbox`
  (`grants`, `prune`); `skein sandbox list|prune` is a call onto them plus a rendering, holding no
  logic of its own. `main.rs`'s own docstring authorises the direct edge · II Local-first ✅
  NON-NEGOTIABLE and untouched: no network, no new capability SID, no change to how a profile is
  created or a process launched. `prune` only ever removes
- III Test-First ⚠️ every behavioural step's outcome is recorded under `## Observed red`, and a
  step with no red says so and why rather than dressing one up. **Two of them are short of the
  bar**: T3's and T4's reds were observed but not transcribed, and cannot be reconstructed
  faithfully after the fact, so each carries a measured counterfactual instead. T6's red is
  unobtainable on this machine at all. Both are stated there rather than papered over
- IV Inverted coupling ✅ `skein-sandbox` remains a leaf depending on no Skein crate. `skein-cli`
  gains an edge to it directly rather than through `skein-connectors`, because `prune` is not an
  MCP tool and putting it behind the tool crate would hide a non-tool capability behind the tool
  boundary. Zero new third-party packages on every target
- V Traceability ✅ unchanged machinery: no new `StepKind`, no change to `ToolGateway`, `Approval`,
  `Redactor` or `AcpPermissionTransport`. Profile cleanup is machine-state maintenance, not a step
  in an agent's run, and putting it on a run's chain would attribute an operator's action to a model
- VI Security ✅ this is the slice that makes a destructive-by-nature capability safe by
  construction rather than by care. Ownership is proven in three structural layers — name gate,
  `REVOKE_ACCESS` on one `TRUSTEE_IS_SID`, live DACL read-back — and the record is deliberately
  **not** one of them, so even a tampered record could at worst revoke a `skein-<hash>` SID's own
  ACE. The destructive command requires an explicit selector, so a bare `prune` is a usage error
  rather than a machine-wide delete
- VII Neutrality ✅ one subcommand group, two functions, one plain-text record file. No new crate,
  no new `windows` feature, no serde. A silo-scoped manifest, a `%LOCALAPPDATA%\skein\` state
  directory, encoding the root in the profile's display name, `GetAppContainerFolderPath`,
  `CheckNetIsolation.exe`, `--json` and `--dry-run` were each considered and rejected with a reason
  in `plan.md`
- VIII Loop discipline ✅ NON-NEGOTIABLE and untouched. Nothing here runs inside a loop; `prune` is
  an operator command with a fixed, finite work list
- Cross-platform ⚠️ **Windows-only in substance, and it inherits rather than amends ADR-0006's
  scope.** The Constitution's "no OS-specific call without `#[cfg]` + an equivalent" is met on the
  `#[cfg]` and **not** on the equivalent — deferred to a Linux (Landlock) and a macOS (Seatbelt)
  slice each, exactly as slices 019 and 020 record. What is new here and worth stating: **there is
  nothing to clean up on those platforms**, because no backend there creates a profile or writes an
  ACE. The refusal is not a stub standing in for missing work; it is the honest answer. The CLI
  subcommand is nonetheless **present** on all three legs, for spec point 6's reason.

## Tasks
- [x] **T0** `specs/024-sandbox-cleanup/{spec.md,plan.md,tasks.md}`; branch `024-sandbox-cleanup`
      reset onto `dev` at `12c14f5`
- [x] **T1** control baseline: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace`, plus the `%LOCALAPPDATA%\Packages\skein-*` count
      before and after a suite run, each re-measured rather than quoted
- [x] **T2** types and signatures, no behaviour: `GrantKind`, `GrantState`, `GrantedDir`, `Grant`,
      `Pruned`, `grants()`, `prune()`, the new `record` module, and `skein-sandbox` on
      `skein-cli`'s dependency list
- [x] **T3** RED→GREEN — the record is written, and a second create unions rather than replaces
      (`tests/record.rs`)
- [x] **T4** RED→GREEN — `grants()` reports the live DACL (`tests/record.rs`)
- [x] **T5** RED→GREEN — `prune()` removes its own ACE and only its own (`tests/prune.rs`), after
      extracting the DACL read-back helpers into `tests/dacl/mod.rs`
- [x] **T6** RED→GREEN — the non-Windows refusal (`tests/absent.rs`)
- [x] **T7** RED→GREEN — the CLI (`main.rs`, `src/sandbox.rs`, `tests/cli_sandbox.rs`)
- [x] **T8** the test suite stops leaking profiles (severable)
- [x] **T9** gates, dependency drift, control diff, close-out
- [ ] **T10** hand-verification against a real session — **not part of this run.** See
      `## Live verification (T10)` below for the recorded commands and pass condition.

## Control baseline (T1)

On `024-sandbox-cleanup` @ `12c14f5` (reset onto `dev`), working tree clean, Windows 11 Pro
10.0.26200, toolchain 1.97, 2026-09-04, before any edit:

- `cargo fmt --all --check` — clean, no output, exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, `Finished dev profile`, exit 0.
- `cargo test --workspace` — **250 passed, 0 failed, 6 ignored**: `acp_session` 16,
  `cli_acp_agent` 16, `cli_chat` 12, `cli_ledger` 8, `cli_secret` 2, `connector` 9, `fs_root` 15,
  `fs_server` 8, `git_root` 5, `git_server` 13, `governed_fs_run` 6 (+2 ignored), `governed_git_run`
  4 (+1 ignored), `governed_proc_run` 0 (+2 ignored), `run_server` 10, `core` 19, `native_loop` 28,
  `tool_gateway` 14, `governed_run` 2, `openai_compat` 17 (+1 ignored), `rmcp_gateway` 9,
  `skein-sandbox` `src/lib.rs` unit target 4 (`argv`), `escape` 4, `launch` 4, `profile` 3,
  `silo_ledger` 7, `silo_secret` 5. Every other `src/lib.rs` and `src/main.rs` unit target reports 0.

Slice 020's close records 217 at `09d61f8`; the delta of +33 is slices 021 and 022, which is why the
baseline is re-measured rather than quoted.

### The leak, measured (T1)

```
$ ls -d "$LOCALAPPDATA/Packages"/skein-* | wc -l
1009                       # at this run's start
$ cargo test --workspace   # one full run
$ ls -d "$LOCALAPPDATA/Packages"/skein-* | wc -l
1036                       # +27
```

Measured twice with the same result. **27 AppContainer profiles per `cargo test --workspace`**, one
per distinct `TempDir` root reaching `Sandbox::create` across
`skein-sandbox/tests/{profile,launch,escape}.rs` and
`skein-connectors/tests/{connector,run_server}.rs`. This is T8's control: it must become 0.

The plan's 955 was the same quantity read in an earlier session. Nothing contradicts it; the number
grows by 27 per suite run and this run measured 1009.

### The record's location, re-measured (T1)

`icacls` on a real profile folder and its `AC` subfolder, this machine, output in French:

| Object | Trustees |
|---|---|
| `…\Packages\skein-000986f7bfe73a3d` | `AUTORITE NT\Système (I)(OI)(CI)(F)`, `BUILTIN\Administrateurs (I)(OI)(CI)(F)`, `WINE\cthedrez (I)(OI)(CI)(F)` — **no AppContainer SID** |
| `…\Packages\skein-000986f7bfe73a3d\AC` | the unresolvable AppContainer SID at `(OI)(CI)(CR)(F)`, plus those three, plus `Niveau obligatoire faible:(OI)(CI)(NW)` |

D1.3 and D3 rest on that difference, and it reproduces exactly as the plan recorded it.

## Observed red

**T3 and T4** — **not transcribed when they were observed**, and this file's own Test-First claim
was false until now. The section was left empty by the earlier part of this run, and the reds
cannot be faithfully reconstructed after the fact: T3 also added `Sandbox::profile()`, so T2's tree
(`624bc75`) does not compile T3's tests, and a red produced by checking that tree out would be a
missing-accessor compile error rather than the unwritten-code red the plan asked for. Rather than
dress one up, both steps were re-grounded the way slice 020 grounded its own no-red steps — with a
**measured counterfactual**, which keeps working after the fact where a transcript does not.

T3's counterfactual, `record::append` replaced by `Ok(())` in `profile::create`, everything else
green:

```
---- a_created_sandbox_records_its_root_and_run_dirs_beside_its_profile stdout ----
D:\Users\cthedrez\AppData\Local\Packages\skein-d6db9923d5c88a60\skein-grants:
Le fichier spécifié est introuvable. (os error 2)
---- a_created_profile_is_listed_with_its_directories_and_their_live_state stdout ----
a profile created by this slice carries a record
test result: FAILED. 1 passed; 4 failed
```

T4's counterfactual, `cleanup::grants` returning `Ok(Vec::new())`, the record writer restored:

```
---- a_created_profile_is_listed_with_its_directories_and_their_live_state stdout ----
the profile just created must be listed, got []
---- a_profile_with_no_record_is_listed_with_no_directories stdout ----
a recordless profile must still be listed, got []
---- nothing_but_a_skein_hash_name_is_listed stdout ----
this machine has created profiles, so an empty listing means the scan found nothing
test result: FAILED. 2 passed; 3 failed
```

The one test that survives T4's counterfactual is
`a_second_create_over_one_root_unions_rather_than_replaces` — correctly, since it reads the record
file directly and never calls `grants()`.

**T5** — `cargo test -p skein-sandbox --test prune`, all four new tests, against `prune`'s
`todo!("T5")` body:

```
thread 'prune_refuses_a_name_it_could_not_have_created' panicked at
crates\skein-sandbox\src\cleanup.rs:107:5:
not yet implemented: T5
test result: FAILED. 0 passed; 4 failed; 0 ignored
```

The unwritten-code red the plan's T2 was written to produce: the name gate, the revoke and the
delete are all unwritten, and the four tests fail on the same line.

**T6** — **no red is obtainable on this machine**, under the standing caveat slice 019 recorded for
its own absence gates: `tests/absent.rs` is `#![cfg(not(windows))]`, so it compiles to nothing here
and runs on two of three CI legs. What was verified instead:

- `cargo check -p skein-sandbox --target x86_64-unknown-linux-gnu --all-targets` — clean, so the
  file compiles where it will run.
- `cargo clippy -p skein-sandbox --target x86_64-unknown-linux-gnu --all-targets -- -D warnings` —
  clean.
- The two substrings it asserts on (`Windows-only`, `list or prune`) are both in `NO_CLEANUP`, which
  this step also had to **repair**: the literal had been collapsed onto one line with its
  line-continuation dropped, so the message an operator would have read carried twenty-six spaces
  in the middle of a sentence.

`cargo check -p skein-cli --target x86_64-unknown-linux-gnu` is **not** available on this machine —
that crate's graph needs a Linux C toolchain (`x86_64-linux-gnu-gcc`), which is what slice 019's
note implies by attributing the cross-check's availability to `skein-sandbox`'s Skein-free and
C-free graph. So `cli_sandbox.rs`'s `#[cfg(not(windows))]` arm was type-checked by temporarily
swapping the two `cfg`s in that file and running `cargo check -p skein-cli --all-targets`, which
passed; the swap was reverted and is in no commit.

**T7** — `cargo test -p skein-cli --test cli_sandbox`, the three new tests, before `main.rs` knew
the subcommand:

```
thread 'prune_without_a_selector_is_a_usage_error' panicked at
crates\skein-cli\tests\cli_sandbox.rs:74:9:
the refusal must name the selectors it wanted: error: unrecognized subcommand 'sandbox'

Usage: skein <COMMAND>
test result: FAILED. 0 passed; 3 failed
```

**T8** — a measurement rather than a red, because the leak is not an assertion that fails but a
count that grows. The control is T1's:

```
$ ls -d "$LOCALAPPDATA/Packages"/skein-* | wc -l
1468
$ cargo test --workspace          # 252 passed, 0 failed, 6 ignored
$ ls -d "$LOCALAPPDATA/Packages"/skein-* | wc -l
1468                              # +0, against T1's measured +27
```

Measured per crate on the way there, which is how the call sites the plan does not name were found:
after guarding only the files the plan lists, `skein-sandbox` and `skein-connectors` were each at
+0 and `skein-cli` was still at **+2**.

## Deviations from the plan

1. **T1's recorded total of 250 does not match its own per-target enumeration**, which sums to
   **240**. Today's run reproduces that enumeration target for target with every pre-existing count
   unchanged, and adds exactly the slice's 12 new tests (`record` 5, `prune` 4, `cli_sandbox` 3),
   for **252**. The per-target numbers were right and the total was an arithmetic slip; the
   baseline text above is left as it was measured rather than edited retroactively.

2. **`dev` moved during this run.** It was `12c14f5` when this branch was reset onto it and is
   `e0fa57a` now — slice 023 merged while this slice was being implemented. T9's control diff is
   therefore taken against **`12c14f5`**, this branch's base and the commit the plan names, not
   against a `dev` that has advanced. Against that base the control directories are byte-identical.

   The predicted conflict does **not** exist: `git diff 12c14f5 dev -- crates/skein-cli/` touches
   only `tests/cli_acp_agent.rs` and `tests/cli_chat.rs`, so slice 023 added no `Command` variant.
   The real overlap is `tests/cli_acp_agent.rs`, which both slices edit — 023 in its stub-provider
   harness, this slice with two `PrunedOnDrop` lines and a `mod guard;`. A merge-time question, not
   a scope one.

3. **`granted_masks` moved back out of `tests/dacl/mod.rs`** into `tests/profile.rs`. The plan's T5
   extracted all four helpers, but each test binary compiles that module for itself, so a helper
   only `profile.rs` calls is dead code in `prune.rs`'s binary and `-D warnings` refuses it. The
   alternatives were a new `#[allow(dead_code)]`, which T9 forbids, or a helper nobody calls. Only
   what **both** files read now lives in the shared module. `profile.rs`'s assertions are unchanged
   and `cargo test -p skein-sandbox --test profile` is 3 passed before and after.

4. **`record::read` collapses an empty record to `None`.** Not in the plan. A file that parses to
   no directories says exactly what no file says, and letting the two shapes differ would have put
   a profile in `skein sandbox list` with no line of its own — invisible in the listing an operator
   uses to decide what to prune.

5. **T8 covers three call sites the plan does not name.** The plan lists
   `skein-sandbox/tests/{profile,launch,escape}.rs` and `skein-connectors/tests/connector.rs`;
   `skein-sandbox/tests/record.rs`, `skein-connectors/tests/run_server.rs` and
   `skein-cli/tests/cli_acp_agent.rs` also mint profiles, and without them the measured leak stops
   at +2 rather than +0. `connector.rs`'s own `Sandbox::create` is inside its
   `#[cfg(not(windows))]` test and mints nothing here; what leaks there is
   `local_connector_with_run` in its three Windows tests.

   Two guard shapes, not one: `skein-sandbox`'s tests hold the `Sandbox` and key on
   `sandbox.profile()`, while the connectors' and the CLI's never see one — the server owns it
   privately and the ACP session is a subprocess — so those key on the fs-root and find the profile
   through `grants()`. The second shape is duplicated in two crates because a `tests/` module
   cannot cross a crate boundary and a shared dev-only crate is out of scope.

6. **The risk the plan flagged did not bite.** `DeleteAppContainerProfile` removes the package
   folder even with the `skein-grants` file inside it, measured by
   `a_real_grant_is_listed_then_pruned_and_the_ace_is_gone_from_the_dacl`. No `remove_dir_all`
   follow-up was needed, so D1.1's lifetime coupling holds as written.

7. **`SetNamedSecurityInfoW` does not duplicate inherited ACEs on the way back.**
   `pruning_leaves_every_ace_it_did_not_write` asserts the fs-root's allow-ACE set is *exactly*
   what it was before `Sandbox::create` ran, and it passes — so rewriting a DACL that was read with
   its inherited entries reinstates them rather than converting them to explicit ones. Worth
   recording because a false assumption here would have been silently destructive.

## Close-out (T9)

On `024-sandbox-cleanup`, working tree clean, Windows 11 Pro 10.0.26200, toolchain 1.97, 2026-09-04:

- `cargo fmt --all --check` — clean, exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, `Finished dev profile`, exit 0.
  **No new `#[allow]` anywhere in the slice.**
- `cargo test --workspace` — **252 passed, 0 failed, 6 ignored**. Every pre-existing target's count
  is unchanged against T1's enumeration; the additions are `record` 5, `prune` 4, `cli_sandbox` 3.
- `cargo check -p skein-sandbox --target x86_64-unknown-linux-gnu --all-targets` — clean.
- `%LOCALAPPDATA%\Packages\skein-*` before and after that suite run — **1468 and 1468**, against
  T1's measured +27. The new gate this slice introduces, and it is met.

**Dependency drift: zero, and proved by the lock file.** `git diff 12c14f5 -- Cargo.lock Cargo.toml`
is **empty**, so no third-party package was added, removed or moved on any target. The one
dependency edge this slice adds — `skein-cli` to `skein-sandbox` — is a path dependency already in
the workspace. `cargo tree --workspace --prefix none | sort -u` counts **184** packages on
`x86_64-pc-windows-msvc` and **164** on `x86_64-unknown-linux-gnu`. No new `windows` feature.

**Control diff**, against this branch's base `12c14f5` (see deviation 2):
`git diff 12c14f5 --stat -- crates/skein-core/ crates/skein-gateway/ crates/skein-silo/
crates/skein-acp/ crates/skein-mcp/ spikes/ rust-toolchain.toml Cargo.lock Cargo.toml` is
**empty**. `crates/skein-connectors/` shows `tests/connector.rs` (+5), `tests/run_server.rs` (+7)
and the new `tests/guard/mod.rs` — **nothing in its `src/`**.

## Live verification (T10)

Performed after rebase onto `dev` at `e0fa57a` (slice 023 merged), on this Windows machine.

`crates/skein-cli/tests/cli_sandbox.rs`'s own `windows::a_real_grant_is_listed_and_pruned_through_the_binary`
was re-run standalone and passed (`cargo test -p skein-cli --test cli_sandbox`: 3 passed, 0 failed) —
a real `Sandbox::create`, then the real `skein sandbox list`/`prune` subprocess, then a real DACL
read-back.

Independently of that test, a throwaway `skein-sandbox` example called `Sandbox::create` directly
over a real `--fs-root` and a real `--run-dir` (`D:\…\skein-t10-manual\{root,toolbin}`), removed
afterward and never committed:

```
created S-1-15-2-4293476591-1125327610-642153627-346045329-3553146693-3959665346-2037894141
```

`icacls` before pruning named the AppContainer trustee (rendered by this machine's localised icacls
as an unresolved "trust relationship" line) at `(F)` on `root` and `(RX)` on `toolbin` — confirming
the two different masks D4/D5 grant.

```
> skein sandbox list | grep 4293476591
skein-b04ff468c9ade33b  S-1-15-2-4293…  root     granted  …\skein-t10-manual\root
skein-b04ff468c9ade33b  S-1-15-2-4293…  run-dir  granted  …\skein-t10-manual\toolbin

> skein sandbox prune --profile skein-b04ff468c9ade33b
revoked …\skein-t10-manual\root
revoked …\skein-t10-manual\toolbin
deleted profile skein-b04ff468c9ade33b
```

After pruning: `skein sandbox list` no longer names the profile; `icacls` on both directories shows
only the pre-existing trustees (`EINSTEIN\CodexSandboxUsers`, `AUTORITE NT\Système`,
`BUILTIN\Administrateurs`, the user) with the AppContainer line gone; and
`%LOCALAPPDATA%\Packages\skein-b04ff468c9ade33b` no longer exists. Every trustee present before
`Sandbox::create` is present, unchanged, after `prune` — the acceptance criterion met on a directory
this session actually granted and revoked, not merely on `prune.rs`'s fixtures.

`prune --all` against the 900+ legacy profiles this machine had accumulated before this slice existed
was **not run** — an operator's own call, per the plan's step 6, and left for the operator to make
deliberately rather than swept up inside a verification pass.
