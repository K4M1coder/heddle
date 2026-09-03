# Tasks: a pinned, handle-relative `FsRoot` (v0 slice)

**Spec:** `specs/021-fs-root-handle-relative/spec.md` · **Plan:**
`specs/021-fs-root-handle-relative/plan.md` · TDD (red→green), branch `021-fs-root-handle-relative`
cut from `dev` at `8e61c64`.

## Constitution Check (ADR-0004 D1 solo-v0 bar)

- **I Headless core** ✅ no CLI of its own and no new flag. `skein chat` and `skein acp-agent` are
  unchanged and stay the authoritative clients; the containment rule stays in `skein-connectors`.
  `skein-cli/src/wiring.rs`'s three `FsRoot::new` call sites keep their signatures and now open and
  hold (or drop) a handle, which is the intended behaviour.
- **II Local-first** ✅ NON-NEGOTIABLE, and this slice is *about* it. Containment stops being a
  decision about a string that a caller then re-walks and becomes a single handle-relative walk in
  which the check and the open are the same operation. Nothing about network egress changes; no new
  network-capable code is compiled in (`cap_std::net` is never named, though `ipnet` rides along in
  the tree — recorded in `spec.md` point 4).
- **III Test-First** ✅ each step's outcome is recorded verbatim under `## Observed red`, and where a
  step had **no** red the entry says so and why rather than dressing one up. Step 3 is a genuine
  red. Step 4's red is not observable on this machine and the entry says exactly why. Step 5 has no
  red by design and carries an **unsandboxed positive control** instead, which is a stronger
  guarantee than a red because it keeps working after the fact.
- **IV Inverted coupling** ✅ `skein-core` gains nothing and depends on nothing new. No `cap_std`
  type appears in `skein-core`, in any `ToolTransport` signature, or in any public signature outside
  `FsRoot`; `cap_std::fs::File` and `ReadDir` are returned to `server.rs` and nowhere else.
  `crates/skein-connectors/Cargo.toml` records `src/fs.rs` as the only module that may name the
  dependency, the same boundary `git2` and `skein-sandbox` already get there.
- **V Traceability** ✅ unchanged machinery, unchanged shape: an `fs` call still lands `ToolCall` →
  `Approval` → `ToolResult` on the chain, and a refusal still verifies. No new `StepKind`, no change
  to `ToolGateway`, `Approval`, `Redactor` or `AcpPermissionTransport`.
- **VI Security** ✅ deny-by-default is unchanged and the containment it rests on is strictly
  stronger. Two holes are closed that were not races at all: `fs_write`'s unchecked leaf, and
  `fs_read`'s size-check/read split. Refusal wording is unchanged, and a genuine access denial is
  explicitly **not** relabelled as an escape.
- **VII Neutrality** ✅ three methods with three current callers, one deleted method, no new tool, no
  new flag and no new crate of our own. A general capability-based filesystem abstraction,
  `cap-tempfile`, `cap_std::net`, per-call `Dir` opening, and a hand-rolled `NtCreateFile`/`openat`
  resolver were each considered and rejected with a reason in `plan.md`.
- **VIII Loop discipline** ✅ NON-NEGOTIABLE and untouched. A refusal is still an `Err(String)` the
  model is told about and the run survives, proven end to end in `governed_fs_run.rs` and again by
  hand against the shipped binary (below).
- **Cross-platform** ✅ — and this is a `✅` where slices 019 and 020 carried `⚠️`. `cap-std` covers
  all three platforms behind one API, `src/fs.rs` gains **no `#[cfg]`**, and slice 016's "no `#[cfg]`
  in the containment code" invariant survives intact. Only the test *helpers* split, as they already
  did.

## Tasks

- [x] **T0** control baseline, re-measured rather than quoted; branch cut from `dev` at `8e61c64`
- [x] **T1** the dependency alone, in its own commit, with the measured 16-package cost in the
      manifest comment (`6d8acf7`)
- [x] **T2** the fixture drop-order fix, landed **before** the `Dir` handle exists (`0c1d134`)
- [x] **T3** RED — the impostor root (`tests/fs_root.rs`)
- [x] **T4** the unchecked leaf on the write path (`tests/fs_root.rs`) — **skips on this machine**
- [x] **T5** the reparse-point mechanism, with an unsandboxed positive control — **no red, by
      design** (`tests/fs_root.rs`, `tests/fs_server.rs`)
- [x] **T6** GREEN — `FsRoot` holds a `cap_std::fs::Dir`; `resolve_new` deleted (`031036b`)
- [x] **T7** GREEN — the three `fs` tools take handles, not paths (`d364405`, `031036b`)
- [x] **T8** the error discriminator, both arms pinned (`tests/fs_root.rs`)
- [x] **T9** end to end — a governed run refuses an escape planted after the server exists
      (`a1e7f8b`)
- [x] **T10** `specs/021-fs-root-handle-relative/{spec.md,plan.md,tasks.md}`
- [x] **T11** hand-verification against the shipped binary — **part of this run**, unlike slices 019
      and 020. See `## Live verification` below.

## Control baseline (T0)

On `dev` @ `8e61c64`, working tree clean, Windows 11 Pro 10.0.26200, toolchain 1.97, 2026-09-04,
before any edit:

- `cargo fmt --all --check` — clean, no output, exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, `Finished dev profile`, exit 0.
- `cargo test --workspace` — **228 passed, 0 failed, 5 ignored**: `acp_session` 16, `cli_acp_agent`
  16, `cli_chat` 12, `cli_ledger` 8, `cli_secret` 2, `connector` 9, `fs_root` 11, `fs_server` 7,
  `git_root` 5, `git_server` 13, `governed_fs_run` 4 (+1 ignored), `governed_git_run` 4 (+1
  ignored), `governed_proc_run` 0 (+2 ignored), `run_server` 10, `core` 19, `native_loop` 25,
  `tool_gateway` 14, `governed_run` 2, `openai_compat` 15 (+1 ignored), `rmcp_gateway` 9,
  `skein-sandbox` `src/lib.rs` unit target 4, `escape` 4, `launch` 4, `profile` 3, `silo_ledger` 7,
  `silo_secret` 5. Every other unit and doc target reports 0.

## Close (T9)

`cargo test --workspace` — **234 passed, 0 failed, 5 ignored**. The delta of **+6** is exactly this
slice's new tests: `fs_root` 11 → 15 (impostor root, leaf symlink, reparse swap, `explain`'s two
arms), `fs_server` 7 → 8 (reparse swap at tool level), `governed_fs_run` 4 → 5 (the governed
refusal). Every other target is unchanged. `cargo fmt --all --check` and
`cargo clippy --workspace --all-targets -- -D warnings` are clean.

## Dependency tree (T1)

`cargo tree -e normal --target x86_64-pc-windows-msvc --prefix none`, deduplicated and diffed
against the same command at `8e61c64`. **16 added, 0 removed**, exactly the set the plan predicted:

```
ambient-authority 0.0.2   cap-primitives 4.0.3    cap-std 4.0.3       fs-set-times 0.20.3
io-extras 0.19.0          io-lifetimes 2.0.4      io-lifetimes 3.0.1  ipnet 2.12.1
maybe-owned 0.3.4         winx 0.36.4             windows-sys 0.59.0  windows-sys 0.60.2
windows-targets 0.52.6    windows-targets 0.53.5  windows_x86_64_msvc 0.52.6
windows_x86_64_msvc 0.53.1
```

Record this set so the next slice can diff it: the three `windows-sys` majors come from
`fs-set-times` / `winx` (0.59) and `io-extras` (0.60), not from `cap-std` directly.

## Observed red

**T3 — the impostor root.** `cargo test -p skein-connectors --test fs_root`, the one new test, run
against the pre-`cap-std` mechanism:

```
thread 'an_impostor_at_the_roots_name_is_not_the_root' panicked at
crates\skein-connectors\tests\fs_root.rs:336:18:
a file planted at the root's vacated name must not be reachable: "planted"
test result: FAILED. 13 passed; 1 failed
```

The expected red, and it says exactly the right thing: the old mechanism **returned the
attacker-planted file's contents**. `resolve("impostor.txt")` canonicalized to a path that still
started with the stored root prefix, so the prefix check passed on a directory the operator never
named. After T6, this machine takes the other arm of the `match` — the rename itself is refused:

```
RENAME-OUTCOME: Err(Os { code: 32, kind: Uncategorized,
message: "Le processus ne peut pas accéder au fichier car ce fichier est utilisé par un autre
processus." })
```

measured with a temporary `eprintln!` that was removed again. That is the plan's predicted `os
error 32`, reproduced, and it is what `spec.md` states as a user-visible consequence.

**T4 — the unchecked leaf on the write path: no observable red on this machine, and the reason is a
fact about the machine.**

The test needs a **file** symlink, and there is no privilege-free equivalent — a junction only names
a directory. This machine (Windows 11 Pro 10.0.26200, Developer Mode off, non-elevated) cannot
create one. Verified independently rather than assumed:

```
PS> New-Item -ItemType SymbolicLink -Path $env:TEMP\slink-probe.txt -Target $env:TEMP
REFUSED: Cette opération nécessite un privilège d'administrateur.
```

and the test's own skip fired:

```
running 1 test
this machine does not permit creating file symlinks; skipping
test result: ok. 1 passed; 0 failed
```

So this test's red is observable on Unix or on a Developer-Mode Windows box and **not here**. Said
plainly rather than dressed up. The same fact is why slice 016's
`a_symlink_pointing_outside_the_root_is_refused` had been silently skipping since it was written —
that one is now re-pointed at a junction, needs no privilege, and executes on Windows for the first
time. The property T4 covers is carried on this machine by T5 and by the live verification's write
half, both of which do run here.

**T5 — the reparse-point mechanism: no red, by design, and the reason is worth more than a red.**

Today's `resolve` canonicalizes at call time, so it *also* refuses this swap; there was nothing
unimplemented for the test to fail against, and both the `fs_root.rs` and `fs_server.rs` tests
passed on their first run against the pre-`cap-std` mechanism. What they prove is the **mechanism** —
that a reparse point out of the root is refused by the handle walk rather than by a
canonicalize-then-hope — and they keep proving it after the fact, which a red does not.

What makes each a guarantee rather than a tautology is the **in-test unsandboxed positive control**,
which is asserted and not assumed:

```
assert_eq!(
    std::fs::read_to_string(swapped.join("secret.txt")).expect("the swap really escapes"),
    "not yours",
    "positive control: without containment this path reads the outside file"
);
```

It passes, so the junction genuinely escapes the root and the three refusals below it are about
containment rather than about a path that never worked. This is slice 020's T4/T5 precedent.

**T8 — the error discriminator: no red.** It pins behaviour that `explain` introduces in the same
step, so there was no earlier mechanism for it to fail against. Its value is that a future
`cap-primitives` change to `escape_attempt()` becomes a failing assertion naming the exact
behaviour, rather than a silently wrong refusal message told to a model. Both arms were measured
here: an escape is `PermissionDenied` with `raw_os_error() == None`; reading a directory as a file
is `PermissionDenied` with `Some(5)`.

**T9 — the governed refusal: no red.** T6/T7's green is the mechanism it exercises. It carries the
same positive control, and what it adds over `fs_server.rs` is the *ordering*: the junction is
planted after the connector — and so its root handle — already exists, which is precisely the window
the old mechanism left open.

## Deviations from the plan, stated

1. **The plan claimed "`tests/connector.rs` and the three `governed_*` tests hold locals rather than
   fixture structs, so they are already correct — verify, do not change."** The code contradicts it:
   `governed_fs_run.rs` and `governed_git_run.rs` each hold a `Harness` struct declaring
   `_dir: TempDir` **first**, with a `LocalConnector` over an `FsRoot` after it. Both were fixed
   with the other four, so T2 covers six fixtures rather than four. `connector.rs` and
   `governed_proc_run.rs` do hold locals, which drop in reverse, and were verified and left alone.
2. **T6/T7's signatures landed one commit early**, as `d364405`, a compile-and-green refactor
   introducing `open_file`/`create_file`/`read_dir` over the *existing* string mechanism before the
   `Dir` handle existed. The plan's literal ordering would have made T3's red a **compile error**
   rather than a behavioural failure, and — decisively — would have made T5's tests unrunnable
   against the old mechanism, which is the only way to verify T5's own "no red, because today's
   `resolve` also refuses this swap" claim. Slice 020's T2 established exactly this discipline
   ("types and signatures, no new behaviour ... a compile-and-green refactor with a stop
   condition"). Every pre-existing assertion stayed green and unreworded across that commit.
3. **Three pre-existing tests were rewritten**, as the plan authorised, because they named the
   deleted `resolve_new`: `an_absolute_argument_is_refused_on_the_write_path_too` (same assertion,
   now through `create_file`), `a_new_file_whose_parent_does_not_exist_is_refused` (same), and
   `a_new_file_under_the_root_resolves_through_its_parent`, renamed to
   `a_new_file_under_the_root_is_created_where_it_was_named` because `create_file` creates where
   `resolve_new` only resolved. No other assertion text changed anywhere; the plan's stop condition
   never fired.
4. **`Cargo.lock` is not committed** — this repository `.gitignore`s it, so T1's commit is the two
   manifests only.

## Live verification (T11)

Run on 2026-09-04 against the **shipped binary** (`target/debug/skein.exe`), on this machine, with a
scripted stub provider standing in for a local model and a real junction planted in a real
`--fs-root`. Nothing below the client is a double: the binary, the connector, the root handle and
the filesystem are the real article. The driver scripts were scratch files under this run's
artifacts and were removed afterwards; nothing was added to the repository.

**Read half — `skein chat`.** Root holding `notes.txt`, a sibling `outside/` holding `secret.txt`,
and `root/sub` a junction to `outside`. The stub answers the first turn with a `fs_read` tool call
for `sub/secret.txt`.

```
positive control (plain read through the junction) = 'STOLEN'
exit code = 0

--- what the model was told about fs_read ---
[tool_result tool=fs_read status=ok]
{"content":[{"type":"text","text":"sub/secret.txt resolves outside the root
\\\\?\\D:\\Users\\cthedrez\\AppData\\Local\\Temp\\skein-021-t6qqgc6x\\root and is refused"}],
"isError":true}

--- did anything appear outside the root? ---
outside/ = ['secret.txt']
```

The positive control is the first line and it is load-bearing: an ordinary `open()` through that
junction really does read the outside file, so the refusal is containment and not a missing path.
`status=ok` is right and load-bearing too — the *transport* succeeded and the refusal is inside the
result, where the model can read it and the run can continue.

**Write half — `skein acp-agent`.** `skein chat` never offers `fs_write` (it has nobody to ask for a
confirmation), so the write path was driven over ACP with a client that **granted** the permission
request by selecting an offered `allow` option, the way a real editor does.

```
permission asked for: "fs_write"

--- stop reason ---
{ "stopReason": "end_turn" }

--- what the model was told about fs_write ---
[tool_result tool=fs_write status=ok]
{"content":[{"type":"text","text":"sub/planted.txt resolves outside the root
\\\\?\\D:\\Users\\cthedrez\\AppData\\Local\\Temp\\skein-021w-e_mzvcyu\\root and is refused"}],
"isError":true}

--- did anything appear outside the root? ---
outside/ = ['secret.txt']
```

The human said **yes** and the write was still refused, with nothing planted outside the root. That
is the point of the half: approval is a governance gate, and containment is not the same gate.

## Next slice

Carried in `plan.md`'s *Next slice*: the `git2` residual, hard-link containment, and the
`ipnet` / `cap_std::net` dead weight.
