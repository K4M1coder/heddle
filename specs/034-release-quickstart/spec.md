# Feature Specification: a release build, a distributable bundle, and a one-command quickstart (v0 slice)

**Feature Branch:** `034-release-quickstart` · **Created:** 2026-09-04 · **Status:** Implemented
(v0 slice) · **Input:** `dev` at `ae3a5a9` is a complete, hardened v0 with no way to hand it to
anyone — no release build anywhere, no packaged artifact, no `LICENSE`, no version but the
never-published `0.0.0`, and no single command from a fresh machine to a working answer ·
Constitution I (**CLI is the authoritative client**), II (**local-first**, NON-NEGOTIABLE), VI
(**deny-by-default, confirmation for destructive actions**), VII (**YAGNI**) · ADR-0004 D3 (v0
scope), ADR-0006 (Windows-first sandbox).

Every slice from 001 to 029 built a capability and proved it with tests. None of them produced
something a person who is not the author can run. A colleague today must clone, discover on their
own that a C compiler is required, learn `--root` / `--silo` / `--model` / `--fs-root` out of source
or `--help`, invent a `HEDDLE_ROOT`, and independently work out that their Ollama model needs tool
support. Failure at any of those points is silent or cryptic, and the last one is not a failure at
all — it is a plausible answer with nothing of Heddle behind it.

## What this slice changes for a user

**There is one command.** `pwsh -File scripts/quickstart.ps1` from a clone, or the same file from an
extracted bundle, goes from nothing to a real model answer produced by a real `fs_read`, and then
prints the run's hash chain and verifies it. Measured on this machine: 69 seconds, 14 steps, `ok`.

**There is something to hand over.** `scripts/package.ps1` produces
`dist/heddle-0.1.0-windows-x64/` and its zip — 13 MB as a folder, 5.2 MB zipped — holding
`heddle.exe`, the same `quickstart.ps1`, `QUICKSTART.md`, `README.md` and `LICENSE`. Copy the folder
to a share or send the zip. Nothing else is required of the recipient but PowerShell 7 and a local
provider.

**`heddle --version` says `0.1.0`.** It said `0.0.0` — cargo's never-published placeholder — which is
what someone handed a folder called *release* would have read.

**The licence text exists.** `[workspace.package]` has claimed `license = "Apache-2.0"` since the
workspace did, and no commit ever carried the text.

**No product behaviour changes at all.** No new subcommand, flag, tool, connector, port, `StepKind`
or payload shape. The only edits under `crates/` are the nine manifest version fields.

## Five things a reader must know up front

1. **The check that carries this slice is the tool-capability check, and it exists because the
   failure it guards is silent.** A wrong port, an absent model, an `https://` base URL and a bad
   `--fs-root` all produce excellent CLI messages already, measured verbatim in `plan.md` §0.5 — the
   quickstart must not re-implement any of them. A model without tool support produces *no error*:
   it answers from its own weights, the chain has no `tool_call` step, and the colleague concludes
   Heddle works when it did not run. Tool capability is therefore read off Ollama's **native**
   `/api/tags`, which reports a `capabilities` array, and not the OpenAI-compatible `/v1/models`
   route Heddle itself uses, which does not.
2. **The smallest tool-capable model is chosen, not the first.** On this machine the first is
   `qwen3.8:27b` and it exhausts memory; `gemma4:latest` at 8B answers the demo prompt in about a
   minute. Ascending `parameter_size`, with an unparseable size sorting **last** so it can never win.
3. **In bundle mode `--fs-root` is the current working directory, never derived from the script's own
   location.** A parent-of-script-directory default resolves to the whole of `%TEMP%` for a bundle a
   mail client extracted there — measured. The current directory is the only defensible default when
   there is no project to point at, and `QUICKSTART.md`'s `cd` into the bundle is load-bearing
   because of it.
4. **`QUICKSTART.md` opens with `Unblock-File` and `-ExecutionPolicy Bypass` before it explains
   anything.** A default Windows box runs PowerShell under `RemoteSigned`, a `.ps1` arriving by zip
   or mail carries a Mark-of-the-Web, and the refusal talks about *digital signatures* while saying
   nothing about what to do. This is the one piece of hand-holding that cannot move into the script,
   because the script is the thing being blocked.
5. **This slice adds no Rust logic, so Constitution III's red-before-green applies to nothing in
   it.** Its equivalent bar is a **recorded live run** — seven of them, transcribed verbatim in
   `tasks.md`: the green path, four provoked prerequisite failures, an idempotency re-run and the
   bundle run from a temp extraction. A fifth failure is not producible on this machine and says so
   with its counterfactual. That is the form slices 024–029 already use for live verification.

## Requirements

- **FR-001** `cargo build --workspace --release` MUST succeed from the repository root with zero
  compiler warnings, and this MUST be recorded as a measured gate rather than assumed.
- **FR-002** `heddle --version` MUST print `heddle 0.1.0`, with the version declared once on
  `[workspace.package]` and inherited by all eight crate manifests.
- **FR-003** The repository MUST carry the Apache-2.0 licence text the manifests already claim, and
  the distributable MUST ship it.
- **FR-004** One script MUST serve both placements — a source checkout and a bundle beside
  `heddle.exe` — distinguishing them by probing for the workspace manifest **and** `crates/`, so a
  bundle extracted somewhere unlucky cannot be mistaken for a checkout.
- **FR-005** In source mode the script MUST invoke cargo unconditionally rather than implement a
  staleness check of its own, and MUST name the C-compiler prerequisite on a build failure.
- **FR-006** The script MUST refuse to run against a model that does not advertise tool support, and
  MUST distinguish *"the provider reported capabilities and none includes `tools`"* from *"the
  provider reported no `capabilities` field at all"*, naming the provider version in the second case.
- **FR-007** With no `-Model`, the script MUST select the **smallest** tool-capable model by
  parameter count, announce the choice, and list the alternatives. It MUST never select silently and
  MUST never pull or install a model.
- **FR-008** The unreachable-provider message MUST NOT surface the caught exception's own text: a
  refused connection is reported by `Invoke-RestMethod` as an `HttpClient` timeout it never waited
  out, which is false.
- **FR-009** In bundle mode `--fs-root` MUST default to the current working directory, and MUST NOT
  be derived from the script's own location.
- **FR-010** With the default prompt, the script MUST verify `README.md` exists under the resolved
  `--fs-root` before asking a model to read it, and MUST name `-FsRoot` and `-Prompt` when it does
  not.
- **FR-011** After the turn, the script MUST print the run's ledger and verify the chain, and MUST
  say so in as many words when the chain holds no `tool_call` step.
- **FR-012** The demo MUST be read-only and MUST NOT use `--allow-run`. `QUICKSTART.md` MUST name
  `--allow-run`, what it grants, that it is Windows-only in v0, and `heddle sandbox list|prune` as
  its undo.
- **FR-013** Re-running the script MUST be safe: the build no-ops, the silo is reused, a second run
  appends, and `ledger verify` reports every run `ok`.
- **FR-014** The script MUST leave behind exactly one directory and MUST print both its path and the
  command that removes it.
- **FR-015** `scripts/package.ps1` MUST build `--workspace --release` before assembling, MUST read
  the version out of `[workspace.package]` rather than have it retyped, and MUST exclude
  `heddle.pdb`.
- **FR-016** The documentation MUST state that this path is verified on Windows only, that
  `heddle.exe` is **unsigned**, and that `cargo install --path` is documented but not exercised.

## Rejected alternatives

| # | alternative | why not |
|---|---|---|
| 1 | three scripts — quickstart + package + a bundle-side `demo.ps1` | the prerequisite logic is identical in both placements; two copies drift, and the bundle's copy is the one nobody runs during development. Packaging is genuinely separate and stays separate: it is the operator's tool, never the colleague's |
| 2 | teach `bootstrap.ps1` to build and demo | it is eight steps of installing *development* dependencies — BMAD, Spec-Kit, LiteLLM, pre-commit — none of which a colleague running a demo needs. Merging them would make the demo path pull `uv`, `npx` and a Spec-Kit init. The quickstart merely *points at* bootstrap when the toolchain is missing |
| 3 | ship a `quickstart.sh` alongside | this slice's standard of proof for a script is a recorded live run, and this machine is Windows. A `.sh` could be written but not run, and an unverified onboarding script is worse than none — it fails in a colleague's hands with the project's name on it. Named as a residual |
| 4 | a staleness check over `src/**` before building | measured: an already-current `cargo build --release -p heddle-cli` returns in 0.17–0.23 s. Cargo already makes that decision, faster and more correctly than a timestamp comparison would |
| 5 | pre-probe for a usable MSVC toolchain | reliably detecting one is fiddly and would be a second thing to keep right, while the failure it guards is loud and rare — and now annotated with the `docs/DEVELOPMENT.md` section that explains it |
| 6 | take the **first** tool-capable model | measured: the first on this machine is 27B and exhausts its memory. Smallest-first is what makes the demo finish |
| 7 | leave `0.0.0` and name the bundle by short SHA | fewer edits, but it puts `heddle-v0-ae3a5a9` on the folder while the binary inside insists it is `0.0.0`. The confusion is moved somewhere less visible, not removed |
| 8 | `1.0.0` | ADR-0004 D3 scopes this as v0. `0.1.0` says pre-1.0 first packaged build, which is exactly true |
| 9 | a `CHANGELOG.md` | there is no history to write that would not be fabricated, and no reader: `specs/001…029/` already is this project's change record in far more depth, and the README points at it. Deferred to `0.2.0` — the first release with a predecessor a reader might be upgrading from — with the criterion written down so nobody back-fills one silently |
| 10 | cargo-dist | its entire output is a GitHub Release workflow plus installer scripts served from a URL. `origin` here is a local bare mirror; every one of those artifacts is inert. It also adds a generated CI workflow and a config surface to maintain for a hand-off that is a file copy |
| 11 | an MSI / WiX installer | needs a code-signing certificate to not be worse than the zip, adds a build-time toolchain, and installs machine-wide state — the opposite of what a first-contact demo should do |
| 12 | `cargo install --path crates/heddle-cli` as the primary path | good for a colleague who already has Rust and a C compiler, so it is documented — but it reimposes the toolchain and the cold build the bundle exists to remove, which is most of the onboarding gap |
| 13 | a bare `.exe` with no folder | the recipient then has no instructions, no licence, and no `README.md` for the demo to read. The folder is the minimum coherent unit |
| 14 | `--allow-run` in the demo | it grants a lasting ACL entry on the target directory, and this machine already carries 1,640 leftover AppContainer profiles from pre-024 test runs. A first-contact demo must not add to that pile, nor require understanding `sandbox prune` before a first answer |
| 15 | a release job in `.github/workflows/core.yml` | CI has no Ollama, so it could build the binary and never run the demo, and a release build is not a merge gate for a repository with no remote to release from |
| 16 | `[profile.release]` tuning (`strip`, `lto`) | MSVC already keeps symbols out of the exe in a separate 7.3 MiB `.pdb`; measured 12.94 MB without tuning. It would buy little and cost build time and debuggability |

## Out of scope

- **A `quickstart.sh`** — rejected alternative 3, recorded as a named residual.
- **Any new CLI subcommand, flag, tool or config file** — Constitution VII. The quickstart supplies
  values to flags that already exist and holds no capability of its own.
- **`CHANGELOG.md`** — rejected alternative 9, with the criterion for when it becomes warranted.
- **Code signing, MSI/WiX, cargo-dist, an auto-updater, a release workflow** — rejected
  alternatives 10, 11, 15. The unsigned binary is recorded as a Constitution deviation, not passed
  over.
- **Installing or bundling Ollama, or pulling a model** — Constitution II. Absence is detected and
  reported.
- **`crates/` source** — the release build was measured clean, so there is no release-profile defect
  to fix. Only the nine manifest version fields change.
- **`spikes/`** — untouched, ADR-0004 D2.
- **Backfilling `specs/030`–`033`** — those branches merged into `dev` without spec directories.
  Whether that was intended is not this slice's question, and inventing them retroactively is the
  same error rejected alternative 9 forbids.
