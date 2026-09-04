# Skein — quickstart

```powershell
Expand-Archive .\skein-0.1.0-windows-x64.zip -DestinationPath .
Get-ChildItem -Recurse .\skein-0.1.0-windows-x64 | Unblock-File
cd .\skein-0.1.0-windows-x64
pwsh -ExecutionPolicy Bypass -File .\quickstart.ps1
```

**The second and fourth lines are not optional, and skipping either is how a first attempt dies.** A
file that arrives by zip, mail or network share carries a Mark-of-the-Web, and a default Windows box
runs PowerShell under `RemoteSigned` — so `quickstart.ps1` is refused with an error about *digital
signatures* that says nothing about what to do. `Unblock-File` removes the mark; `-ExecutionPolicy
Bypass` covers the case where it is still refused. `skein.exe` is **not code-signed** either, so
SmartScreen may also interpose the first time; *More info → Run anyway* is the answer, and if that is
not acceptable on your machine, stop here rather than working around it.

The `cd` matters too: in a bundle the quickstart points the agent at your **current directory**, and
running it from inside the bundle folder is what gives it the `README.md` the demo reads.

If you are reading this file from inside the extracted folder, the first line is already done.

## What the script does

Nothing is installed, downloaded or changed on your machine. In order it:

1. Checks it is running under PowerShell 7+.
2. Finds `skein.exe` beside itself.
3. Asks your local model provider — Ollama on `http://localhost:11434/v1` unless you pass
   `-BaseUrl` — which models it has, **and which of them support tools**. This is the check that
   matters: a model without tool support does not fail, it answers from its own weights, and you
   would see a plausible paragraph and conclude Skein worked when nothing of Skein ran.
4. Picks the **smallest** tool-capable model installed, prints which one and what else was
   available, and takes `-Model` if you would rather choose. Smallest, not first: the largest model
   on a machine is the one that exhausts its memory mid-answer.
5. Runs one turn: `skein chat`, read-only, scoped to the current directory.
6. Prints the run's ledger — the `tool_call` / `approval` / `tool_result` triple is the agent
   actually reading a file through a governed tool — and then `skein ledger verify`, which
   recomputes the hash chain over the whole run.

It takes about a minute on an 8B model, most of it inference. Re-running is safe: a second run
appends another run to the same chain, and `verify` checks them all.

Flags: `-Model NAME`, `-BaseUrl URL`, `-FsRoot PATH`, `-Prompt TEXT`.

## What it needs from you

- **PowerShell 7+** — `winget install --id Microsoft.PowerShell -e`
- **A local model provider with a tool-capable model.** Ollama:
  `winget install --id Ollama.Ollama -e`, then `ollama serve`, then `ollama pull gemma4` (8B, about
  9.6 GB on disk). The script never installs or pulls anything; it detects what is missing and says
  so.

Skein talks to local providers over `http` only, on loopback. There is no TLS backend compiled into
the binary, so an `https://` base URL is refused rather than attempted — a build-time property, not
a setting.

## What it leaves behind

One directory: `%LOCALAPPDATA%\skein\quickstart-demo`, holding the demo silo and its ledger. The
script prints the path and the command to remove it:

```powershell
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\skein\quickstart-demo"
```

Nothing else. The demo turn is read-only by construction rather than by care: `skein chat`'s tool
allowlist has no `fs_write` on it at all, because a non-interactive command has nobody to confirm a
destructive action to.

## Going further

`skein --help` lists the whole CLI: `chat`, `acp-agent`, `ledger`, `secret`, `sandbox`.

The one capability the quickstart deliberately never exercises is **`--allow-run`** on
`skein acp-agent`, which offers the sandboxed `proc_run` tool. It is worth knowing what it costs
before you reach for it:

- It grants that run's AppContainer identity an inheritable entry on `--fs-root`'s ACL — a real and
  lasting change to that directory's permissions, not a process-lifetime one.
- It is **Windows-only** in v0. Elsewhere it is a refusal with a reason, not a missing flag.
- `skein sandbox list` shows every profile and grant on the machine, and `skein sandbox prune`
  revokes and removes one. That is the undo, and it is worth running once after you first try
  `--allow-run` so you can see what it left.

## Building it yourself instead

With the repository, Rust and a C compiler already in place, `scripts/quickstart.ps1` works
identically from a clone — it builds `skein.exe` first and points the agent at the repo root.

`cargo install --path crates/skein-cli` puts `skein` on your `PATH` instead of in a folder. That
form is documented here and **was not exercised** when this bundle was made: running it installs
into `~/.cargo/bin`, which is persistent state outside a build.

## What is verified, and where

This bundle and this script were verified on **Windows 11 with PowerShell 7.6.5**. macOS and Linux
are untested here and there is no `quickstart.sh` yet — the product itself is green on all three
platforms in CI, but this onboarding path is not, and saying otherwise would be a guess. Skein 0.1.0
is the first packaged build; `specs/` in the repository is its change record.
