# Tasks: a release build, a distributable bundle, and a one-command quickstart (v0 slice)

**Spec:** `specs/034-release-quickstart/spec.md` · **Plan:** `specs/034-release-quickstart/plan.md` ·
branch `034-release-quickstart` reset onto `dev` at `ae3a5a9`. **No Rust logic is added**, so
Constitution III's red→green applies to nothing here; the substitute bar is plan D12's — a recorded
live run, transcribed verbatim, and every transcript below was produced on this machine this run.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the quickstart holds **no capability**: it is a sequence of calls onto
  `heddle chat` and `heddle ledger log|verify` plus a rendering, and it invents no flag. The CLI
  remains the authoritative client and this is a client *of* the client. `package.ps1` is the same
  relationship to `cargo` · II Local-first ✅ NON-NEGOTIABLE and untouched: no network egress is
  added, the provider probe is loopback (`/api/tags` on the same host `LocalEndpoint::parse` already
  restricts the run to), nothing is downloaded, installed or pulled. A missing Ollama or a missing
  model is **detected and reported**, never fixed
- III Test-First ⚠️ **and the ⚠️ is the honest mark, not a shortfall being dressed down.** This
  slice adds no Rust behaviour: D6 edits nine manifest version fields, and the rest is two
  PowerShell scripts, three documents and a licence. There is no unit under test and no red to
  observe, and a Rust test asserting that a `.ps1` exists would be padding. The substitute is plan
  D12's: **seven** runs of the real script, transcribed below under `## The recorded runs` — the
  green path, four provoked failures, an idempotency re-run and the bundle run — plus the packaging
  run. That is the form slices 024 and 026–029 already use for their live-verification tasks. **A
  fifth failure, a model installed but not tool-capable, could not be produced on this machine** and
  says so, with the measured counterfactual, rather than being faked
- IV Inverted coupling ✅ nothing crosses a boundary that did not already exist. No crate gains a
  dependency, no trait changes, and `Cargo.lock` remains uncommitted and unpinned. Zero new
  third-party packages on every target
- V Traceability ✅ unchanged machinery — no new `StepKind`, no change to `ToolGateway`, the
  `Redactor` or the chain — and the slice's whole demo *is* the traceability, printed: the
  `tool_call` / `approval` / `tool_result` triple and then `ledger verify` recomputing the hash chain.
  Two consecutive runs into one silo both verify `ok`, measured below
- VI Security ✅ deny-by-default holds and is strengthened by omission. The demo passes `--fs-root`
  and **never** `--allow-run`, so no AppContainer profile is minted and no ACL is changed — the flag
  is documented in `QUICKSTART.md` with what it grants and with `sandbox list|prune` as its undo, and
  never executed. Read-only is structural rather than promised: `ToolArgs::chat_policy` has no
  `fs_write` on it at all. No secret is read, written, passed on a command line or logged; the
  scripts take no credential of any kind. `--fs-root` in bundle mode is the operator's current
  directory and never a directory derived from the script's own location — plan D9a, with the
  measured `%TEMP%` blast radius that alternative had
- VII Neutrality ✅ two scripts, two documents, one licence, nine one-line manifest edits. No new
  crate, subcommand, flag, config file or CI job. cargo-dist, WiX/MSI, a CHANGELOG, a third script, a
  `bootstrap.ps1` merge, `[profile.release]` tuning and an MSVC pre-probe were each considered and
  rejected with a reason in `spec.md`'s table. The provider probe is Ollama-shaped because Ollama is
  what `bootstrap.ps1 -WithOllama` installs, and it degrades to a named, version-attributed refusal
  against a server that does not report capabilities rather than pretending to know
- VIII Loop discipline ✅ NON-NEGOTIABLE and untouched. The demo turn runs inside `NativeLoop` under
  its existing externally-enforced budget; the script adds no loop of its own and no retry. It raises
  one budget knob — `--timeout-secs 180` over the CLI's 120 — which is an operator supplying a value
  to an existing externally-enforced bound, not the model deciding when to stop. Its reason is
  measured (plan D5)
- Cross-platform ⚠️ **the onboarding script is Windows-only, by stated choice, and the product is
  not.** `core.yml`'s tri-OS gate on `crates/` is unchanged and still the thing that carries the
  Constitution's cross-platform requirement. The quickstart itself is written in portable PowerShell
  7 where portability is free — `Join-Path`, no `winget` invocation, `$IsWindows` guarding only the
  silo-root default and the binary's name — but it was **verified on Windows 11 / PowerShell 7.6.5
  only**, and no `quickstart.sh` ships. Plan D2 records why: this slice's standard of proof for a
  script is a recorded live run, this machine is Windows, and an unverified onboarding script fails
  in a colleague's hands with the project's name on it. A `quickstart.sh` with a real run behind it is
  a named residual below. `package.ps1` is Windows-only in substance and refuses elsewhere with a
  reason, because the artifact it names is `windows-x64`
- Per-OS code signing ⚠️ **the bundle ships an unsigned `heddle.exe`, and the Constitution's
  *Additional Constraints* require Authenticode for an agent that drives the PC.** No certificate
  exists, so this is a deviation recorded with its consequence rather than passed over: SmartScreen
  may interpose on first run, and `QUICKSTART.md` says so in those words and tells a reader who finds
  that unacceptable to stop rather than work around it. It is also the strongest reason an MSI would
  be *worse* than a zip today — a signed installer is the real fix, and it needs a certificate, not a
  build system

## Tasks
- [x] **S0** `git fetch origin dev && git reset --hard origin/dev` → `ae3a5a9`, **the first action of
      the run and load-bearing**: this project's worktrees have repeatedly started seventy commits
      behind at `d364405`, where `crates/heddle-cli/src/sandbox.rs` does not exist and `README.md`'s
      *Current status* is the stale one the new Quickstart section sits beside. Then the three gates
      unmodified as the control baseline
- [x] **S1** `cargo build --workspace --release` measured: exit, warning count, wall time, binary and
      `.pdb` sizes, `heddle --version`
- [x] **S2** the version bump (D6) — `version = "0.1.0"` on `[workspace.package]`,
      `version.workspace = true` in all eight `crates/*/Cargo.toml`
- [x] **S3/S4** `scripts/quickstart.ps1` — placement probe, the four prerequisite checks, smallest
      tool-capable model selection, the demo turn, the ledger and its verification
- [x] **S5** `scripts/package.ps1` and `dist/heddle-0.1.0-windows-x64/` plus its zip
- [x] **S6** the provoked prerequisite failures, transcribed — four produced, one not producible
- [x] **S7** the green run, the idempotency re-run, and the bundle run from a temp extraction
- [x] **S8** `LICENSE` — the Apache-2.0 text the manifests have always claimed
- [x] **S9** `QUICKSTART.md`, the README *Quickstart* section, and this spec triple

## Control baseline (S0)

```
git log -1 --oneline                                          → ae3a5a9
cargo fmt --all -- --check                                    → FMT_OK
cargo clippy --workspace --all-targets -- -D warnings         → CLIPPY_OK
cargo test --workspace                                        → TEST_OK   (exit 0, every suite ok)
```

## The release build (S1, S2)

```
cargo build --workspace --release
    Finished `release` profile [optimized] target(s) in 1m 20s
EXIT=0
SECONDS=81
WARNINGS=0
-rwxr-xr-x  12943872  target/release/heddle.exe
-rw-r--r--   7630848  target/release/heddle.pdb
$ ./target/release/heddle.exe --version
heddle 0.1.0
```

No release-profile-only defect: vendored `libgit2`, bundled SQLite and the `windows`/`win32job` FFI
in `heddle-sandbox` all compiled clean. Nothing in `crates/` changed but the nine version fields.

**Deviation from the plan's step order:** the plan ran S1 before S2 and again after. This run built
once, after the version bump, because the bump edits manifest fields only and cannot change a
compilation outcome except the version string — and building after it proves both facts, the gate
and `heddle 0.1.0`, from one measurement instead of asserting the second.

## The recorded runs

### S6(a) — provider unreachable

```
$ pwsh -NoProfile -File .\scripts\quickstart.ps1 -BaseUrl http://localhost:19999/v1

==> Placement
    source checkout at D:\...\worktrees\034-release-quickstart

==> Rust toolchain
    cargo 1.97.1 (c980f4866 2026-06-30)

==> Release build
    cargo build --release -p heddle-cli
    Finished `release` profile [optimized] target(s) in 0.23s

==> Local model provider
    GET http://localhost:19999/api/tags

quickstart: nothing answered at http://localhost:19999. Heddle only ever talks to a provider on this
machine over http, so this has to be a local one: start it with 'ollama serve' (install with
'winget install --id Ollama.Ollama -e'), or point elsewhere with -BaseUrl.

EXIT=1
```

Three things this asserts. The no-op rebuild is **0.23 s**, which is plan D3's whole justification.
The trailing `/v1` was stripped for the native route. And **neither of `Invoke-RestMethod`'s own
texts appears** — not the French `Aucune connexion n'a pu être établie…` measured this run, and not
the `HttpClient.Timeout` misreport planning measured at a shorter timeout (plan §0.6).

### S6(b) — `-Model` not installed

```
$ pwsh -NoProfile -File .\scripts\quickstart.ps1 -Model no-such-model
…
==> Release build
    Finished `release` profile [optimized] target(s) in 0.19s

==> Local model provider
    GET http://localhost:11434/api/tags

==> Model

quickstart: -Model 'no-such-model' is not installed at http://localhost:11434. Installed and
tool-capable: gemma4:latest, lfm2.5:latest, qwen3.8:27b.

EXIT=1
```

The listing order is itself evidence: `8.0B, 8.5B, 27.3B` — the ascending-`parameter_size` sort of
plan D5, not `/api/tags`' own order, which puts the 27B model first.

### S6(c) — a model that is installed but not tool-capable: **not producible on this machine**

All three models here advertise `tools`, and pulling a non-tool model to provoke it would download
gigabytes to exercise one `Where-Object`. So, per plan D12's own allowance, the filter is exercised
directly against a synthetic entry alongside the real ones, and the real `capabilities` arrays are
recorded:

```
  qwen3.8:27b      27.3B   capabilities: completion,tools,thinking,vision
  gemma4:latest    8.0B    capabilities: completion,tools,thinking
  lfm2.5:latest    8.5B    capabilities: completion,tools,thinking
  synthetic non-tool model excluded by the filter: True
  -Model refusal branch predicate ($named.capabilities -notcontains 'tools'): True
  ollama version: 0.33.3
```

Both branches that depend on the predicate are covered: the selection filter drops the entry, and
the `-Model` refusal fires on it. What is **not** covered is the end-to-end path through the CLI with
such a model named, and that is stated rather than implied.

### S6(d) — `cargo` off `PATH`, source mode

```
$ $env:PATH = (($env:PATH -split ';') | Where-Object { $_ -and (-not (Test-Path (Join-Path $_ 'cargo.exe'))) }) -join ';'
$ pwsh -NoProfile -File .\scripts\quickstart.ps1

==> Placement
    source checkout at D:\...\worktrees\034-release-quickstart

==> Rust toolchain

quickstart: cargo is not on PATH. Install the development dependencies first: pwsh -File
D:\...\worktrees\034-release-quickstart\scripts\bootstrap.ps1

EXIT=1
```

It fails before the build and names the script that fixes it, with a resolved absolute path.

### S6(e) — the default prompt with no `README.md` under the resolved root

Not in the plan; added because plan D9a's default makes it reachable. Run from the *parent* of an
extracted bundle rather than from inside it:

```
$ pwsh -NoProfile -File <bundle>\quickstart.ps1     # cwd = the parent, no README.md

quickstart: the demo prompt reads README.md, and D:\...\Temp\heddle-bundle-test-9190979 has none. Run
this from the folder you want the agent to read, name that folder with -FsRoot, or ask something
else with -Prompt.

EXIT=1
```

This is why `QUICKSTART.md`'s opening block has four lines and not three: the `cd` into the bundle is
load-bearing.

### S7 run 1 — the green path, from a deleted demo silo

```
$ Remove-Item -Recurse -Force "$env:LOCALAPPDATA\heddle\quickstart-demo"
$ pwsh -NoProfile -File .\scripts\quickstart.ps1

==> Placement
    source checkout at D:\...\worktrees\034-release-quickstart

==> Rust toolchain
    cargo 1.97.1 (c980f4866 2026-06-30)

==> Release build
    cargo build --release -p heddle-cli
    Finished `release` profile [optimized] target(s) in 0.17s

==> Local model provider
    GET http://localhost:11434/api/tags

==> Model
    using model gemma4:latest (8.0B, tool-capable) — the smallest tool-capable model installed
    pass -Model to choose another: lfm2.5:latest, qwen3.8:27b

==> One real turn
    silo root : D:\Users\cthedrez\AppData\Local\heddle\quickstart-demo
    fs root   : D:\...\worktrees\034-release-quickstart
    run id    : quickstart-20260904214020
    prompt    : Read the file README.md in the project root and answer in one short paragraph: what is Heddle, and what is its current status?

run quickstart-20260904214020
Heddle is an independent, open-source, local-first agentic platform designed to unify numerous
capabilities such as chat, coding, multi-agent execution, governed workflows, and team collaboration
into a single adaptable product. Its current status is **v0**, with a strict-local coding agent
implemented and functional. This version includes a native loop with an ACP boundary, MCP tools for
file system and git operations, a Windows-first sandboxed shell connector, and the necessary `heddle`
CLI tools for management, ledger access, and secret handling.

==> The chain that turn wrote
    quickstart-20260904214020	0	iteration_boundary	f30f1aecc58df69e8b63847cf5d201803af8cc64f9780675a08b9aa33ed3f668
    quickstart-20260904214020	1	llm_request	64bbe85a9989e7f1d031680d9d0c8f5c493783ff7eef0cc07e34ebb6dd76c3a7
    quickstart-20260904214020	2	wire_exchange	9b592fd957ee2e098045c8fbe7efd829c4b6f0307609b9395fdddb4b2056d20d
    quickstart-20260904214020	3	llm_response	2e87a29c56a0e12523a94e6a1f041c77b7d8f0f283032ca6eef512078efbef66
    quickstart-20260904214020	4	budget_spent	c5c90030a368b6fef51ebb7ae6ed40db9cd6b72ee2c3ede2d497d037010eabf1
    quickstart-20260904214020	5	tool_call	d0960c12502314b2a11bf57b8bac7a2e7cb1f8ebcb7402cf6449846b776c777a
    quickstart-20260904214020	6	approval	530693078a5ac04c6e5089bd08bfdfdc2cb0aeb1298c92d5f124fac301807e2b
    quickstart-20260904214020	7	tool_result	dd4ec27502551cd8cf632af90362c6783bafec9d13c1975dbd009b05f81b2243
    quickstart-20260904214020	8	iteration_boundary	6a7212288456f2b7c7a4c09b9fb4dd6925110e6290eca6af3d3c4ca766d1bb70
    quickstart-20260904214020	9	llm_request	3802f842b38477fbf01f2aac4295ee08cc727a1ee94a39adb1f38ee2474ff412
    quickstart-20260904214020	10	wire_exchange	42350bcb6634805cd3a22c8332955dcaff1cd6623766dbc1b8168b3074425e22
    quickstart-20260904214020	11	llm_response	3ac87ce0fc15d9b1b433e729f93bd4d2f1f873a6c6407feedffd6c27dda8e303
    quickstart-20260904214020	12	budget_spent	7ef2ff9e06097d84ef5086306d9b418174b729e0795b89d40f919e3871b48868
    quickstart-20260904214020	13	exit	e1d91786188772e92bef0bc8b3ba4646636e7e60f5499eb8cad4563c3833f482

quickstart-20260904214020	ok	14 steps

==> Done
    …
        Remove-Item -Recurse -Force 'D:\Users\cthedrez\AppData\Local\heddle\quickstart-demo'

EXIT=0  ELAPSED=69s
```

**Steps 5/6/7 are the point of the whole slice.** `tool_call` → `approval` → `tool_result` is the
agent reading `README.md` through a governed tool, and the answer above is grounded in what it read
rather than in the model's weights — which is exactly what a chain without that triple would not
prove, and what the script's own warning would have said.

### S7 run 2 — idempotency, same silo, nothing deleted

Same script, unchanged, immediately after. The build no-opped (0.21 s), the silo was reused, the run
appended, and **`ledger verify` reported both runs**:

```
    run quickstart-20260904214135
    …
    quickstart-20260904214135	5	tool_call	272203f71ffcd62281e784716c65069b7efaae42f590349979082b0e7d825fc0
    quickstart-20260904214135	6	approval	2a679aa95bc52d3af7168c25bd01e6edebc330528a572fb48fd075c4012a3dde
    quickstart-20260904214135	7	tool_result	4d08ea848564f37e453c55ff2e5ca5fe4f03bae91bb83f400ef0352b63b09f7b
    …
quickstart-20260904214020	ok	14 steps
quickstart-20260904214135	ok	14 steps

EXIT=0
```

### S5 — the bundle

```
$ pwsh -NoProfile -File .\scripts\package.ps1

==> Release build
    Finished `release` profile [optimized] target(s) in 0.19s

==> Assemble heddle-0.1.0-windows-x64

==> Compress

==> Done
          11 346  LICENSE
           5 365  QUICKSTART.md
          10 732  quickstart.ps1
           3 292  README.md
      12 943 872  heddle.exe

       5 192 500  heddle-0.1.0-windows-x64.zip

EXIT=0
```

Exactly the five files plan D9 names, no `heddle.pdb`, 13 MB as a folder and **5.19 MB zipped**. Plan
D9 estimated "roughly half"; measured it is 40 %, and the README says the measured pair.

### S7 run 3 — the bundle, extracted under `%TEMP%`, run exactly as `QUICKSTART.md` says

```
$ Expand-Archive .\heddle-0.1.0-windows-x64.zip -DestinationPath .
$ Get-ChildItem -Recurse .\heddle-0.1.0-windows-x64 | Unblock-File
$ cd .\heddle-0.1.0-windows-x64
$ pwsh -NoProfile -ExecutionPolicy Bypass -File .\quickstart.ps1

==> Placement
    bundle at D:\Users\cthedrez\AppData\Local\Temp\heddle-bundle-test-9190979\heddle-0.1.0-windows-x64

==> Local model provider
    GET http://localhost:11434/api/tags

==> Model
    using model gemma4:latest (8.0B, tool-capable) — the smallest tool-capable model installed

==> One real turn
    silo root : D:\Users\cthedrez\AppData\Local\heddle\quickstart-demo
    fs root   : D:\Users\cthedrez\AppData\Local\Temp\heddle-bundle-test-9190979\heddle-0.1.0-windows-x64
    run id    : quickstart-20260904214252
    …
    quickstart-20260904214252	5	tool_call	20b89218eb2999a9fed35bddaa7ebe795805ecbddcb46abfe8ab139d078f6bf8
    quickstart-20260904214252	6	approval	09b4c59ce20109e4b73013657d2999d09f48da3d185ed4ecdd0a6a2eb9867551
    quickstart-20260904214252	7	tool_result	b348c7e0b998f728d5eae00c1149d02451bfff613e77cc11f7a26b4c9d46078c
    …
quickstart-20260904214020	ok	14 steps
quickstart-20260904214135	ok	14 steps
quickstart-20260904214252	ok	14 steps

EXIT=0
```

**No `==> Rust toolchain` and no `==> Release build`** — the placement probe took the bundle branch,
which is what makes the bundle useful to someone with no Rust. The third run appended to the same
silo and all three still verify.

**And the bug plan D9a exists for, measured as a counterfactual on the same extraction:**

```
  parent-of-script-dir : D:\Users\cthedrez\AppData\Local\Temp\heddle-bundle-test-9190979
  (extracted straight into %TEMP%, that is exactly D:\Users\cthedrez\AppData\Local\Temp)
  what the script uses  : the current working directory
```

A `--fs-root` derived from the script's own location hands an agent the operator's whole temp
directory. The `fs root` line in the transcript above shows what shipped instead: the bundle folder,
because that is where the operator was standing.

## Deviations from the plan

1. **One release build, after the version bump, not one before and one after.** Recorded under *The
   release build* above with the reason.
2. **Model selection is smallest-first, not first-found.** The plan's D5 said *"take the first"*;
   independent manual testing this session measured `qwen3.8:27b` — the first entry `/api/tags`
   returns here — OOM-ing on this machine. First-found would have shipped a quickstart that reliably
   fails on the machine it was written on. Plan D5 as committed records the corrected decision and
   why. The plan's *invariant* — never pick silently — is unchanged and met.
3. **`--timeout-secs 180`, which the plan did not specify.** Manual testing measured an 8B model
   close enough to the CLI's 120 s default on a read-then-answer turn for it to cut off a turn about
   to succeed, and measured a many-sequential-tool-calls prompt exceeding it outright. The prompt
   stays the single-`fs_read` one the plan measured good; the budget gets margin.
4. **A fifth check, and a fourth line in `QUICKSTART.md`'s opening block.** Both fall out of D9a's
   corrected `--fs-root` default: with the default prompt the script verifies `README.md` exists
   under the resolved root, and the reader is told to `cd` into the bundle. Transcript S6(e).
5. **`plan.md` §0.6's stated reason for not surfacing the network exception is not the one planning
   measured.** Re-measured here, `Invoke-RestMethod` reported the refusal accurately — and in French,
   from the OS display language. The conclusion is unchanged and now rests on a measurement from this
   run, with the timing-dependent timeout misreport recorded behind it.
6. **`specs/034-release-quickstart/{spec.md,plan.md,tasks.md}` did not exist at the start of this
   run.** The planning artifact the run order pointed at was read in full and its decisions D1–D12
   implemented; the spec triple is written here as S9, which is where the plan's own step list put it.
7. **S6(c) was not producible** and says so, with the counterfactual, above.

## Residuals

- **`scripts/quickstart.sh`, with a real recorded run behind it.** Plan D2. Until then the onboarding
  path is Windows-only and both `QUICKSTART.md` and the Constitution Check say so. The product itself
  stays tri-OS green through `core.yml`.
- **Code signing.** `heddle.exe` is unsigned; no certificate exists. Named in the Constitution Check
  with its consequence, and the reason an MSI is not an improvement today.
- **`CHANGELOG.md` at `0.2.0`**, not before — the first release with a predecessor a reader might be
  upgrading from. Plan D7 fixes the criterion and the stub line so nobody back-fills one silently.
- **The end-to-end path with a tool-incapable model named through the CLI.** S6(c); both predicate
  branches are covered, the full path is not.
- **`cargo install --path crates/heddle-cli`** is documented in `QUICKSTART.md` and **not exercised**:
  running it installs into `~/.cargo/bin`, persistent machine state outside a build. Marked as such
  where it is documented.
- **`specs/030`–`033`** still have no spec directories. Out of scope by `spec.md`; noted so the gap
  is not mistaken for this slice's.

## Gates (S9)

```
cargo fmt --all -- --check                             → pass
cargo clippy --workspace --all-targets -- -D warnings  → pass
cargo test --workspace                                 → pass
cargo build --workspace --release                      → exit 0, 0 warnings
pwsh -File scripts/quickstart.ps1                      → a real answer, a tool_call/approval/
                                                         tool_result triple, `ok 14 steps`
```
