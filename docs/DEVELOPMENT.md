# Development Environment — setup from anywhere

Goal: **resume work from any machine** with one command right after `git clone` — languages, quality gates, test tooling, MCP connectors, and the method frameworks (BMAD, Spec-Kit, loop-engineering discipline) all in place.

## Quick start

```bash
git clone <repo> skein && cd skein
# Windows (PowerShell 7+):
pwsh -File scripts/bootstrap.ps1 -WithOllama
# macOS / Linux:
./scripts/bootstrap.sh --with-ollama
```

The scripts are **idempotent** (safe to re-run; each step checks before installing) and end with a verification table. Prefer the scripts over this doc; the doc explains *what* and *why*.

## What gets installed

| Layer | Tool | Why |
|---|---|---|
| Languages | **Rust 1.97** (rustup; pinned by `rust-toolchain.toml`) · **Node LTS** (npx) · **Python 3.11+ via uv** | core / installers / sidecar & LiteLLM |
| Quality | `rustfmt`, `clippy` (as toolchain components) · **pre-commit** | Constitution: `fmt` + `clippy -D warnings` green before merge |
| Tests | `cargo test` (built-in) · workspace dev-deps pulled by Cargo on first build: `tempfile`, `rusqlite`, `tokio`. CLI end-to-end tests need **no** harness crate — Cargo's own `CARGO_BIN_EXE_skein` + `std::process::Command` reach the real binary. (`wiremock` is used only by the excluded `spikes/` tree; nothing in the workspace uses `assert_cmd`.) | TDD (Constitution III) |
| Model gateway | **LiteLLM** (`uv tool install litellm`) · config `config/litellm.config.yaml` | single OpenAI-compat entry point (design §4.5) |
| Local inference | **Ollama** (optional flag) + `llama3.1` | Local mode, egress OFF (design §7.3). `skein chat --model <NAME>` reaches it **directly** at Ollama's own OpenAI-compatible endpoint — `--base-url`, else `$SKEIN_MODEL_BASE_URL`, else `http://localhost:11434/v1`. No LiteLLM sidecar is needed for this path. `skein-gateway` compiles `ureq` with no TLS backend, so **loopback `http://` is the only thing it can reach** (Constitution II) |
| Agent runtime | **Goose** (manual install; see script step 6) | evaluated by the **ADR 0001 spike** — integration path is `goosed` / embedded crate / **native loop** (ADR 0002 D1/D11) |
| Planning framework | **BMAD-METHOD** (`npx bmad-method install`) → `_bmad/`, `_bmad-output/`, skills `bmad-*` | bridge: BMAD plans (docs/METHODOLOGY.md) |
| Execution framework | **Spec-Kit** (`uvx specify init`) → `.specify/`, skills `speckit-*` | bridge: Spec-Kit executes, constitution-gated |
| Loop discipline | no install — **Constitution VIII** + `docs/research/loop-engineering.md` | every agentic loop: external termination + ground-truth verification |

## MCP connectors & their connections

- **Embedded, default full-local** (design §4.3): `fs`/`git`/`shell` work offline out of the box; **network connectors (Atlassian Jira/Bitbucket/Confluence, M365 Outlook/SharePoint/Teams) ship disabled**.
- Enabling one is a **scope-owner authorization** resolved through the hierarchy Silo ▸ (Team) ▸ Project ▸ Conversation, under the egress boundary (ADR 0002 D3/D4).
- For development, authenticate interactively: run `claude` in the repo and use `claude mcp` (OAuth flows). **Never** store tokens in files — secrets are references (`keychain://…`, design §7.13); put them in the OS keychain with `printf %s "$TOKEN" | skein secret set keychain://<service>/<account>`. The value is read from stdin only — there is no `--value` flag, because one would land in shell history and in process listings — and `skein` refuses to read a secret from an interactive terminal.

## Frameworks usage (after bootstrap)

- Open an interactive session in the repo: `claude`
- Spec-Kit gates: `/speckit-clarify` → `/speckit-checklist` → `/speckit-plan` → `/speckit-tasks` → `/speckit-analyze` → `/speckit-implement`
- BMAD: `bmad-validate-prd`, `bmad-create-story` (reads `_bmad-output/implementation-artifacts/sprint-status.yaml`), `bmad-dev-story`, `bmad-check-implementation-readiness`
- Method reference: `docs/METHODOLOGY.md` (bridge) · completeness rules: `docs/DESIGN-COMPLETENESS-POLICY.md`

## Verify / smoke

`scripts/bootstrap.{ps1,sh}` step 8 prints a check table. Once code exists: `cargo build --all && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test --all`. End-to-end smoke: `docs/superpowers/plans/phase0-smoke-test.md` (produced by Task 10).

## Machine prerequisites (not installed by the scripts)

- Windows 11 / macOS 13+ / recent Linux; ~10 GB free (toolchains + one local model)
- Windows: `winget` available (the script uses it) · macOS: Homebrew recommended · Linux: curl + build-essential
- **A C compiler**, on every platform: `skein-silo` takes `rusqlite` with the `bundled` feature,
  which compiles the SQLite amalgamation from source rather than linking a system library — so
  there is no per-OS SQLite prerequisite, at the cost of needing MSVC build tools (Windows),
  Xcode command-line tools (macOS) or `build-essential` (Linux). The three rows above already
  supply one.
- For cowork later (v3): macOS Accessibility/Screen-Recording permissions; Linux X11/Wayland portals (design §4.9)
