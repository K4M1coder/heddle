# Heddle

Heddle is an independent open-source, local-first agentic platform designed to unify chat, coding, cowork, governed workflows, multi-agent execution, tools, MCP connectors, local/cloud inference, memory, evidence and team collaboration in one adaptable product.

## Project ownership

Heddle is created and owned by **Cédric Thedrez**:

- GitHub: **[`kamicoder`](https://github.com/kamicoder)**
- Other public identity: **`cethgame`**

## Current status

v0 — the strict-local coding agent scoped by [ADR-0004 D3](docs/superpowers/adr/0004-solo-v0-calibration.md) — is implemented: a Heddle-owned native loop behind an ACP boundary, MCP tools for `fs` and `git` plus a Windows-first sandboxed `shell` connector ([ADR-0006](docs/superpowers/adr/0006-shell-connector-windows-first-sandbox.md)), one local model path via an Ollama-compatible gateway, a silo-backed event-sourced Ledger and OS-keychain `SecretProvider`, and the `heddle` CLI (`chat`, `acp-agent`, `ledger`, `secret`, `sandbox`). Each implemented slice has its own spec, plan and task record under [`specs/`](specs/), numbered in build order; the design docs below remain the source of intent behind them.

`heddle chat` now routes through **named providers**: an operator describes each provider once in a flat `providers.toml` — its address, its model and, optionally, a reference to a credential in the platform keychain — and later runs name it with `--provider <NAME>`. A provider declared `kind = "cloud"` is **refused before any socket is opened** unless the run passes `--allow-egress`, which is off by default; a provider declared `kind = "local"` keeps the loopback-only guarantee unconditionally. No TLS backend is compiled in, so reaching a real cloud endpoint is deliberately still not possible — see [`specs/035-model-gateway-routing/spec.md`](specs/035-model-gateway-routing/spec.md).

A first Phase-1 desktop slice now sits on top of that core: a Tauri Chat window (`ui/`) drives the same `heddle acp-agent` an editor would, so every UI action is a call the CLI already serves — see [the UI guide](docs/UI.md) for how to run it and what each button sends. The Code view and the settings/connector screens are deliberately not in it yet.

Key documents:

- [Master design](docs/superpowers/specs/2026-07-15-heddle-design.md)
- [BMAD PRD](_bmad-output/planning-artifacts/PRD.md)
- [BMAD architecture](_bmad-output/planning-artifacts/architecture.md)
- [Spec-Kit constitution](.specify/memory/constitution.md)
- [Design completeness policy](docs/DESIGN-COMPLETENESS-POLICY.md)
- [Platform landscape and reuse strategy](docs/research/agent-platform-landscape.md)
- [Development environment](docs/DEVELOPMENT.md)
- [Desktop UI](docs/UI.md)

## Development language policy

All persistent project content is written in English: source code, comments, documentation, specifications, architecture artifacts, examples, tests and commit messages.

## Quickstart

One command, from a clone, to one real answer produced by one real tool call on a chain you can
verify:

```powershell
pwsh -File .\scripts\quickstart.ps1
```

It builds the release binary, checks that a local provider is running and that one of its models
actually supports tools, runs a single read-only turn against this repository, and prints the run's
ledger and its verification. It installs nothing and pulls no model — what is missing is reported,
not fixed.

Without the repository, `pwsh -File scripts/package.ps1` produces `dist/heddle-0.1.0-windows-x64/`
and its zip: `heddle.exe`, the same `quickstart.ps1`, this README, the licence, and a
[`QUICKSTART.md`](QUICKSTART.md) that is the whole instruction set for someone who has never seen
Heddle. Copy the folder to a share or send the zip — 13 MB as a folder, 5 MB zipped. Windows only so
far, and
[`QUICKSTART.md`](QUICKSTART.md) says what that means.

Contributors want *Development bootstrap* below instead: the quickstart is a demo path and
deliberately installs none of the development dependencies.

## Development bootstrap

After cloning:

```powershell
# Windows
.\scripts\bootstrap.ps1
```

```bash
# macOS / Linux
./scripts/bootstrap.sh
```

These scripts prepare the languages, quality tools, BMAD, Spec-Kit and local development dependencies required by the current project stage.
