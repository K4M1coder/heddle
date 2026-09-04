# Plan — slice 034: a release build, a distributable bundle, and a one-command quickstart

**Spec:** `specs/034-release-quickstart/spec.md` · **Branch:** `034-release-quickstart`, reset onto
`dev` at `ae3a5a9` · **No PR** (no real remote; `origin` is a local bare mirror existing only for
worktree isolation) · Conventional Commits.

This slice introduces **no Rust logic**, so Constitution III's red-before-green applies to nothing
in it. D12 states what "tested" means instead, and `tasks.md` makes it a set of recorded live runs.

---

## 0. Read this first — what is in the tree, measured this run

### 0.1 The baseline

```
git fetch origin dev && git reset --hard origin/dev
git log -1 --oneline      → ae3a5a9  refactor(skein-core): share one secret buffer across a run's redactors
```

This step is load-bearing and not ceremony: this project's worktrees have repeatedly started from
`d364405`, seventy commits behind, where `crates/skein-cli/src/sandbox.rs` does not exist, `specs/`
stops at 020, and `README.md`'s *Current status* is the stale one the new Quickstart section would
have had to sit beside. Slice 029's `plan.md` §0.1 opens with the identical finding.

**Two claims in the work order are refuted by the tree, and one file it names does not exist:**

1. *"specs 003-033 complete"* — `specs/` on `dev` holds **29** directories, 001–029. Branches
   `origin/030-…` through `origin/033-…` are merged into `dev` but landed as fixes and docs without
   creating `specs/03x-…` directories. `034` is still the correct next number; the count is not 33.
2. *"AGENTS.md"* — no such file at any commit reachable here. The engineering discipline meant lives
   in `.specify/memory/constitution.md`, which this plan cites.
3. The `sandbox` subcommand is present, as the work order says — `mod sandbox;`, `Command::Sandbox`,
   `SandboxCommand::{List, Prune}` in `crates/skein-cli/src/main.rs`.

### 0.2 The three gates, run unmodified as the control baseline

| gate | result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --workspace` | pass, every suite `ok` |

### 0.3 `cargo build --workspace --release` — measured, not assumed

| fact | value |
|---|---|
| exit | 0 |
| compiler warnings | **0** |
| wall time (warm cargo cache, cold release profile) | 1 m 20 s |
| `target/release/skein.exe` | **12,943,872 bytes** |
| `target/release/skein.pdb` | 7,630,848 bytes — MSVC keeps symbols *out* of the exe |
| no-op rebuild (`-p skein-cli`, already current) | 0.17–0.23 s |
| `skein --version` after D6 | `skein 0.1.0` |
| toolchain | `cargo 1.97.1` / `rustc 1.97.1`, from `rust-toolchain.toml` (`channel = "1.97"`) |

**There is no release-profile-only defect.** The three candidates — vendored `libgit2`, bundled
SQLite, and the `windows` / `win32job` FFI in `skein-sandbox` — all compiled clean with zero
warnings. `crates/` therefore needs no change on this account, and the 0.2 s no-op number is what
makes D3 correct.

### 0.4 Ollama reports tool capability as a first-class API fact

`ollama version 0.33.3`. `GET /api/tags` returns, per model, a `capabilities` array — measured on
this machine:

```
qwen3.8:27b      27.3B   completion,tools,thinking,vision
gemma4:latest    8.0B    completion,tools,thinking
lfm2.5:latest    8.5B    completion,tools,thinking
```

`GET /v1/models` — the OpenAI-compatible route Skein itself uses — returns only
`{id, object, created, owned_by}`. So the probe must use the **native** route at the server root,
which the script derives from the configured base URL by stripping a trailing `/v1`.

### 0.5 The CLI's own failure messages are already excellent — which decides what the script must not check

Provoked this run against the release binary built from this branch, verbatim:

```
$ skein chat … --base-url http://localhost:19999/v1
error: model provider: POST http://localhost:19999/v1/chat/completions failed: io: Connection
refused; is a local provider listening at http://localhost:19999/v1?

$ skein chat … --model no-such-model:latest
error: model provider: http://localhost:11434/v1 returned 404: {"error":{"message":"model
'no-such-model:latest' not found","type":"not_found_error","param":null,"code":null}}

$ skein chat … --base-url https://api.openai.com/v1
error: model provider: base URL "https://api.openai.com/v1" is not a local provider: scheme "https"
is refused; Skein v0 talks to local providers over http only, and no TLS backend is compiled in

$ skein chat … --fs-root .
ope
error: tool transport: fs root .
ope: Le fichier spécifié est introuvable. (os error 2)
```

The last one carries the same OS-localized text §0.6 does, from the same source. It is the CLI's
message and not this slice's to change.

**So the quickstart must not re-implement these.** Its checks earn their place only where the CLI
*cannot* diagnose the problem — above all the tool-capability case (D4).

### 0.6 Two more measured facts that shape decisions

- **`skein sandbox list` prints 1,640 profiles on this machine**, all `unrecorded` — historical
  AppContainer leaks from pre-024 test runs. This is the concrete reason D8 keeps `--allow-run` out
  of the demo: it is the flag that mints them.
- **`Invoke-RestMethod`'s own exception text must never be surfaced for this probe — and the
  measured reason is not the one this slice's planning assumed.** Planning recorded it misreporting a
  refused connection as *"The request was canceled due to the configured HttpClient.Timeout of 3
  seconds elapsing"*, which is false; nothing was listening and it failed instantly. **Re-measured
  this run at a 5 s timeout, that did not reproduce:**

  ```
  TYPE : System.Net.Http.HttpRequestException
  TEXT : Aucune connexion n'a pu être établie car l'ordinateur cible l'a expressément refusée. (localhost:19999)
  ```

  Accurate — and in French, because it comes from the OS display language, inside a project whose
  *Development language policy* makes every piece of persistent content English. So the conclusion
  holds on a reason this run actually measured, with the timeout misreport as a second,
  timing-dependent one behind it. The script substitutes its own message; the S6(a) transcript
  asserts the substitute appears and neither original does.

### 0.7 Anchors verified on `dev` at `ae3a5a9`

| anchor | file | fact relied on |
|---|---|---|
| `ChatArgs` / `Command::Chat` | `crates/skein-cli/src/main.rs` | the exact flag set the demo invokes |
| `SiloArgs::root` | `crates/skein-cli/src/main.rs` | `--root`, else `$SKEIN_ROOT`, else a loud refusal — so passing `--root` makes the demo independent of ambient env |
| `LedgerCommand::{Log,Verify}` | `crates/skein-cli/src/main.rs` | `--run` narrows `log`; `verify` without `--run` covers every run in the silo |
| `DEFAULT_BASE_URL` | `crates/skein-cli/src/wiring.rs` | `"http://localhost:11434/v1"`, and `--base-url` else `$SKEIN_MODEL_BASE_URL` else that — the precedence the script mirrors |
| `ModelArgs::model` | `crates/skein-cli/src/wiring.rs` | `--model` is **required**, deliberately: *"defaulting to a model the machine may not have produces a 404 that looks like a bug"* |
| `ModelArgs::timeout_secs` | `crates/skein-cli/src/wiring.rs` | default 120 s, whole-request — what D5's 180 raises |
| `ToolArgs::chat_policy` | `crates/skein-cli/src/wiring.rs` | `chat` gets `fs_read`/`fs_list` (+ git when the root is a repo) and **never** `fs_write`: the demo is read-only by construction |
| `ToolArgs::git_tools` | `crates/skein-cli/src/wiring.rs` | `git_status`/`git_log` appear only when `--fs-root` is a git repository |
| `RunArgs::allow_run` | `crates/skein-cli/src/wiring.rs` | *"Grants this run's AppContainer identity an inheritable entry on that directory's ACL, which is a real and lasting change"* — D8 quotes this |
| `chat` / `minted_run_id` | `crates/skein-cli/src/chat.rs` | run id to **stderr**, answer to **stdout**; a supplied `--run-id` bypasses minting, which is why D5 supplies one |
| `sandbox::{list,prune}` | `crates/skein-cli/src/sandbox.rs` | the documented undo `QUICKSTART.md` points at |
| `LocalEndpoint::parse` | `crates/skein-gateway/src/lib.rs` | loopback-only, `http://` only, no TLS backend compiled in |
| `Silo::open` | `crates/skein-silo/src/lib.rs` | `create_dir_all`s the silo dir; the id must be one component of `[A-Za-z0-9._-]` |
| `fs_read` / `fs_list` descriptions | `crates/skein-connectors/src/server.rs` | `path` is relative to the root — so the demo prompt names `README.md`, not an absolute path |
| `record.rs` profile dir | `crates/skein-sandbox/src/record.rs` | `%LOCALAPPDATA%\Packages\<profile>\` — so `%LOCALAPPDATA%\skein\` is free for the demo silo root |
| bootstrap scripts | `scripts/bootstrap.{ps1,sh}` | dependency installation only, **no build and no run step**; both read the pinned channel out of `rust-toolchain.toml` rather than hardcoding it — a pattern this slice copies |
| CI | `.github/workflows/core.yml` | `fmt` / `clippy -D warnings` / `test`, tri-OS, **debug only — nothing in CI has ever built `--release`** |
| C-compiler prerequisite | `docs/DEVELOPMENT.md` §*Machine prerequisites (not installed by the scripts)* | the section D3's hint text points at rather than inventing one |
| `.gitignore` | `.gitignore` | already ignores `target/`, **`dist/`** and `Cargo.lock` — so D9 needs no ignore change, and there is no committed lockfile to pin a release build |
| crate versions | all 8 `crates/*/Cargo.toml` | every one `version = "0.0.0"`; `[workspace.package]` declares `edition`, `rust-version`, `license` and **no `version`** |
| licence text | repository root | `license = "Apache-2.0"` is declared, and **there is no `LICENSE` file at any commit** |

### 0.8 Machine facts the documentation depends on

- PowerShell **7.6.5**, Core edition, `$IsWindows = True`, `Compress-Archive` present.
- `Get-ExecutionPolicy -List` → `LocalMachine = RemoteSigned`; every other scope `Undefined`, and
  `Process = Bypass` only because the shell this run measured from sets it. **So on a default box a
  `.ps1` arriving with a Mark-of-the-Web is refused**, with an error about signatures that says
  nothing about what to do. D11 addresses it.
- `%LOCALAPPDATA%` = `D:\Users\cthedrez\AppData\Local`; `%LOCALAPPDATA%\skein` did not exist.

---

## 1. Problem

See `spec.md`. In one line: `dev` at `ae3a5a9` is a complete v0 with no way to hand it to anyone.

## 2. Approach

### D1 — One script, two placements, not two scripts

`scripts/quickstart.ps1` is the whole quickstart, in two modes told apart by one probe:

```powershell
$SourceMode = (Test-Path (Join-Path $repoRoot 'Cargo.toml')) -and (Test-Path (Join-Path $repoRoot 'crates'))
```

Both markers, not one, so a bundle extracted into an unlucky parent cannot be mistaken for a
checkout.

| | source mode (in a clone) | bundle mode (beside `skein.exe`) |
|---|---|---|
| binary | built by `cargo build --release -p skein-cli`, then `target/release/skein.exe` | `$PSScriptRoot/skein.exe`, as-is |
| toolchain check | required | **skipped entirely** |
| `--fs-root` | the repo root — a git repo, so `git_status`/`git_log` are advertised too | the **current working directory** (D9a) |

Rejected alternatives 1 and 2 in `spec.md` record why this is not three scripts and not a
`bootstrap.ps1` extension.

### D2 — Windows/PowerShell only in this slice; no `quickstart.sh`, stated plainly

`scripts/bootstrap.{ps1,sh}` establishes a pairing precedent, and the demo path is genuinely
cross-platform — the core loop, `fs`, `git` and Ollama all are, and only `--allow-run` is
Windows-only per ADR-0006, which D8 keeps out. **Nevertheless this slice ships PowerShell only**,
because D12's standard of proof for a script is a recorded live run and this machine is Windows.

Two consequences honoured rather than improved on:

- The script uses portable PowerShell 7 where portability is free — `Join-Path`, no `winget` calls,
  no `Get-ExecutionPolicy`, `$IsWindows` guarding only the silo-root default and the binary's name.
- **It does not claim to work off Windows.** `QUICKSTART.md` and `tasks.md` both record: verified on
  Windows 11 with PowerShell 7.6.5; macOS and Linux untested; a `quickstart.sh` with a real recorded
  run behind it is a named residual. The Constitution's cross-platform constraint is met by
  `core.yml`'s tri-OS gate on the *product*, not by this script.

### D3 — Always invoke cargo; never hand-roll a staleness check

Source mode runs `cargo build --release -p skein-cli` unconditionally. §0.3 measured the no-op at
0.17–0.23 s, so "build if not already built" is a decision cargo makes faster and more correctly
than a timestamp comparison over `src/**` would. `-p skein-cli` and not `--workspace`: the
quickstart needs one binary; `--workspace --release` is the *gate* (§4) and `package.ps1`'s job,
which is different.

On a non-zero exit the script prints one targeted hint before failing — the C-compiler
prerequisite, pointing at `docs/DEVELOPMENT.md`'s *Machine prerequisites* section, because
`rusqlite`'s `bundled` and `git2`'s `vendored-libgit2` fail deep inside a build script with a
`cc`/`link.exe` error that names neither. It does **not** pre-probe for MSVC (rejected alternative
5). This prerequisite is exactly what bundle mode removes.

### D4 — Prerequisite checks: exactly four, each earning its place

Each fails naming what is missing *and* the command that fixes it:

1. **PowerShell 7+** — checked before anything that touches `$IsWindows`, which a 5.1 host does not
   define.
2. **Toolchain** (*source mode only*) — `cargo` on `PATH`; missing → names `scripts/bootstrap.ps1`.
   Present but `rustc --version` not matching `rust-toolchain.toml`'s channel → a **warning**, not a
   failure: with rustup installed the toolchain file makes cargo fetch the right one anyway, and
   `rust-version = "1.97"` is the hard backstop. The channel is *read from the file*, never
   hardcoded — both bootstrap scripts do exactly this.
3. **Provider reachable** — `GET {root}/api/tags`, 5 s timeout, `{root}` being the base URL with a
   trailing `/v1` stripped. On any exception: a message naming the URL, the loopback-only rule, and
   `ollama serve` — and **never** the caught exception's text, per §0.6.
4. **A tool-capable model** — filter `/api/tags` on `capabilities -contains 'tools'`.

Check 4 carries the slice: the other three fail loudly by themselves, and **this one does not**.
`spec.md` reader note 1 states the failure mode. The filter also distinguishes *"capabilities were
reported and none includes `tools`"* from *"no model reported a `capabilities` field at all"*, and in
the second case names the Ollama version rather than blaming the models — that field comes from the
server and an older one omits it.

### D5 — Model selection: chosen loudly, and smallest-first

- `-Model <name>` given → verify it is installed **and** tool-capable; refuse otherwise, listing the
  tool-capable models that *are* present.
- Not given → take the **smallest** tool-capable model by `parameter_size` ascending, print
  `using model <name> (<size>, tool-capable) — the smallest tool-capable model installed`, and list
  the others behind `-Model`.
- **None tool-capable** → fail, naming the gap and a concrete pull command with its download size.

**Smallest, not first, and this is the one place this slice departs from its own first design.**
Independent manual testing this session measured `qwen3.8:27b` — the first entry `/api/tags` returns
here — OOM-ing on this machine, while `gemma4:latest` at 8B completes the demo. First-found would
have shipped a quickstart that reliably fails on the machine it was written on. An unparseable
`parameter_size` sorts to `+∞` so it can never win the choice.

The invariant forbids picking *silently*, not picking. A quickstart that refuses to run until you
name a model is not a one-command quickstart; one that announces its choice and shows the
alternatives is. Skein's own `--model` stays required — `wiring.rs` records why — and the script
supplies the value rather than changing the CLI. It suggests `ollama pull gemma4` with its ~9.6 GB
size; it never runs it.

**`--timeout-secs 180` over the CLI's own 120.** Manual testing measured an 8B model close enough to
120 s on a read-then-answer turn for the default to cut off a turn about to succeed, and measured a
prompt requiring *many* sequential tool calls exceeding it outright. So the prompt is fixed to the
single-`fs_read` one that was measured reliable, and the budget is given margin. `-Prompt` exists for
anyone who wants another.

### D6 — `version = "0.1.0"` on `[workspace.package]`; every crate takes `version.workspace = true`

`skein --version` printed `skein 0.0.0` — cargo's never-published placeholder, and what someone
handed a folder called *release* would read. One declaration on `[workspace.package]`, and
`version.workspace = true` in all eight `crates/*/Cargo.toml`, matching how `edition`, `rust-version`
and `license` are already inherited. Verified safe: nothing under `crates/` references
`CARGO_PKG_VERSION` or the literal `0.0.0` outside the manifests, so no test asserts on it. It also
makes the bundle's name derivable rather than invented. Rejected alternatives 7 and 8.

### D7 — No CHANGELOG in this slice, and the reason is recorded rather than left implicit

Rejected alternative 9. **Scoped explicitly for the future:** a CHANGELOG becomes warranted at the
first release with a *predecessor a reader might be upgrading from* — `0.2.0`. That slice starts the
file with `0.2.0` and a one-line `0.1.0 — first packaged build (specs 001–029)` stub pointing at
`specs/`. Deciding it here is what stops a later implementer from silently back-filling one.

### D8 — The demo is `--fs-root` only; `--allow-run` is documented, never exercised

`RunArgs`'s own doc comment: *"Grants this run's AppContainer identity an inheritable entry on that
directory's ACL, which is a real and lasting change to the directory's permissions."* §0.6 measured
what that costs in practice: 1,640 leftover profiles here. A first-contact demo must not add to that
pile on a colleague's laptop, nor require them to understand `skein sandbox prune` before they have
seen a single answer.

Read-only is structural rather than promised: `ToolArgs::chat_policy` omits `fs_write` from `skein
chat`'s allowlist entirely, with the comment explaining that a non-interactive command has nobody to
confirm a destructive action to. The demo therefore *cannot* write, whatever the model tries.

`QUICKSTART.md` gets a *Going further* section naming `--allow-run`, what it grants, that it is
Windows-only in v0 (ADR-0006), and `skein sandbox list` / `prune` as the undo — with the suggestion
to run `list` once after first trying it, so the cost is seen rather than described.

### D9 — "Distributable" = a folder and its zip, from `scripts/package.ps1`

```
dist/skein-0.1.0-windows-x64/
    skein.exe        12,943,872   from target/release (§0.3)
    quickstart.ps1       10,732   the identical file from scripts/, copied (D1)
    QUICKSTART.md         5,365   the colleague's whole instruction set (D11)
    README.md             3,292   project context *and* the demo's fs_read target
    LICENSE              11,346   (D10)
dist/skein-0.1.0-windows-x64.zip  5,192,500
```

`dist/` is already ignored, so the artifact is never committed. `package.ps1` runs
`cargo build --workspace --release` first, so packaging cannot outrun the gate; the version is read
out of `[workspace.package]` and never retyped, so the folder name and the binary's `--version`
cannot disagree. `skein.pdb` is excluded: 7.3 MiB of separate debug symbols on a 13 MB payload,
serving nobody without the matching source tree. Rejected alternatives 10–13 record why this is not
cargo-dist, not an MSI, not `cargo install` as the primary path, and not a bare `.exe`.

**How the operator hands it over:** copy the folder to a network share and send the path, or send the
zip. Both are stated literally in `QUICKSTART.md` and in the README section.

### D9a — In bundle mode `--fs-root` is the current working directory

**A real bug, found by manual testing this session and not to be reintroduced.** A default derived
from the script's own location — parent-of-script-directory — resolves, for a bundle run from where
a mail client extracted it, to the entire `%TEMP%` directory. Measured this run: the parent of the
bundle folder under a temp staging directory is that staging directory, and extracted straight into
`%TEMP%` it is `%TEMP%` itself.

So: source mode defaults `--fs-root` to the repo root, bundle mode to `(Get-Location).Path`. Because
the default prompt names `README.md`, the script also verifies that file exists under the resolved
root before asking a model to read it, and names `-FsRoot` and `-Prompt` when it does not — which is
also what makes the `cd` in D11's opening lines load-bearing rather than cosmetic.

### D10 — Ship a `LICENSE` file

`[workspace.package]` asserts `license = "Apache-2.0"` and there was **no licence text at any
commit**. Handing someone a binary whose own metadata claims a licence the artifact does not carry is
a defect of the distributable, not a nicety. The standard Apache-2.0 text at the repository root,
appendix copyright naming Cédric Thedrez (the owner `README.md` already names), shipped in the
bundle. This asserts nothing new — it supplies the text for a claim the manifest already makes.

### D11 — `QUICKSTART.md` leads with Mark-of-the-Web, because that is what kills the first run

`LocalMachine = RemoteSigned` on a default box (§0.8). A `.ps1` arriving by zip, mail or share
carries a Mark-of-the-Web and is refused with a message about *digital signatures* that tells the
reader nothing. The binary is unsigned too, so SmartScreen may also interpose.

`QUICKSTART.md` therefore opens with the literal commands, before any explanation:

```powershell
Expand-Archive .\skein-0.1.0-windows-x64.zip -DestinationPath .
Get-ChildItem -Recurse .\skein-0.1.0-windows-x64 | Unblock-File
cd .\skein-0.1.0-windows-x64
pwsh -ExecutionPolicy Bypass -File .\quickstart.ps1
```

and *then* explains. This is the one piece of hand-holding that cannot move into the script, because
the script is the thing being blocked. The `cd` is the fourth line rather than three because of D9a:
in a bundle the agent is pointed at the current directory, and being inside the bundle folder is what
gives it the `README.md` the demo reads.

### D12 — What "tested" means here, and the honest Constitution Check

No Rust logic is added (D6 changes manifest fields only), so there is no red to write. The
equivalent, and the bar `tasks.md` clears, is **a recorded live run on this machine, transcribed
verbatim** — the discipline slices 024 and 026–029 already use for live verification. Seven runs of
the real script: the green path, four provoked prerequisite failures, an idempotency re-run, and the
bundle run from a temp extraction. A fifth failure — a model installed but not tool-capable — is not
producible on a machine whose every model is tool-capable, and `tasks.md` records the counterfactual
instead of faking it.

Two Constitution rows are **⚠️ with a reason**, and must not be rounded up:

- **Cross-platform** ⚠️ — the quickstart is verified on Windows only, by D2's stated choice; a `.sh`
  sibling with a real run behind it is a named residual. The *product* remains tri-OS green via
  `core.yml`; only this onboarding script is not.
- **Per-OS code signing** ⚠️ — *Additional Constraints* requires Authenticode for an agent that
  drives the PC, and this bundle ships an **unsigned** `skein.exe`. No certificate exists. Recorded
  as a deviation with its consequence (SmartScreen, D11) rather than passed over — and it is the
  strongest reason an MSI would be worse than a zip today: a signed installer is the real fix, and it
  needs a certificate, not a build system.

Constitution I is *satisfied by construction*: the quickstart holds no capability, invents no flag,
and is a sequence of calls onto the existing CLI plus a rendering — the same relationship
`sandbox.rs` documents for itself.

## 3. Steps

- **S0** reset onto `dev` at `ae3a5a9`; confirm the tree; run the three gates as the control
  baseline.
- **S1** verify `cargo build --workspace --release` and record exit, warnings, time and size.
- **S2** the version bump (D6); `skein --version` → `skein 0.1.0`.
- **S3/S4** `scripts/quickstart.ps1` — mode detection, the four checks, model selection, then the
  demo turn and the ledger.
- **S5** `scripts/package.ps1` and the bundle (D9).
- **S6** the provoked-failure transcripts.
- **S7** the green run, the idempotency re-run, and the bundle run.
- **S8** `LICENSE` (D10).
- **S9** `QUICKSTART.md`, the README Quickstart section, and this spec triple.

## 4. Validation

The project's three gates, unchanged, at S0 and again at the end:

| gate | command |
|---|---|
| format | `cargo fmt --all -- --check` |
| lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| tests | `cargo test --workspace` |

Plus the two this slice adds:

| gate | command | expectation |
|---|---|---|
| release build | `cargo build --workspace --release` | exit 0, **0 warnings**, `target/release/skein.exe` |
| the quickstart itself | `pwsh -File scripts/quickstart.ps1` | a real answer, then a `tool_call`/`approval`/`tool_result` triple on a chain `ledger verify` reports `ok` |

**No new Rust tests.** The slice adds no Rust behaviour to prove, and a test asserting a PowerShell
script exists would be padding. The behaviour-proving evidence is `tasks.md`'s verbatim transcripts —
D12's substitute for red-before-green, and the form slices 024–029 already use.

`core.yml` is **not** extended with a release job (rejected alternative 15). Named here so a later
implementer does not add one reflexively.

## 5. Risks and rollback

- **A quickstart that "works" without calling a tool.** The failure D4's fourth check exists for.
  Mitigated twice — the capability filter before the run, and the ledger after it, where a missing
  `tool_call` step is both visible and called out in words. If a transcript ever shows a chain
  without that triple, the run did **not** demonstrate Skein.
- **An unusable network error.** §0.6: the probe's own exception text is either localized to the OS
  display language (measured) or blames a timeout for an instant refusal (measured at a shorter
  timeout, not reproduced here). Mitigated by never surfacing it; the S6(a) transcript asserts the
  substitute message and the absence of both originals.
- **`capabilities` is provider-version-dependent.** Verified on Ollama 0.33.3. An older server omits
  the field, which would make the filter yield nothing on a machine that has a tool-capable model —
  so the check distinguishes the two cases and names the version in the second.
- **A model that is tool-capable but too large or too weak.** Both measured: `qwen3.8:27b` OOMs, and
  `gemma4:latest` answered the README prompt well but shallowly on a `Cargo.toml` prompt. Mitigated
  by smallest-first selection (D5), by fixing the prompt to the one measured good, and by `-Prompt`.
- **A bundle defaulting `--fs-root` too broadly.** D9a, with the measurement.
- **Blast radius.** Two new scripts, two new docs, one new `LICENSE`, one README section, nine
  manifest version fields. **No crate source changes** — §0.3 found no release-profile defect.
- **Rollback** is `git revert` of the slice's commits: the scripts and `dist/` are additive (and
  `dist/` is ignored), and D6 is nine one-line manifest reversions. No product behaviour changes, so
  nothing downstream can break.

## 6. Out of scope

See `spec.md`'s *Out of scope*.
