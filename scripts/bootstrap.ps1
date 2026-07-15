# Skein — development environment bootstrap (Windows, PowerShell 7+)
# Idempotent: safe to re-run. Run from the repo root right after `git clone`.
#   pwsh -File scripts/bootstrap.ps1 [-WithOllama]
param([switch]$WithOllama)

$ErrorActionPreference = "Stop"
function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Have($cmd) { return [bool](Get-Command $cmd -ErrorAction SilentlyContinue) }

Step "1/8 Core tooling (git, Rust, Node, Python/uv)"
if (-not (Have git))  { winget install --id Git.Git -e --silent }
if (-not (Have rustup)) { winget install --id Rustlang.Rustup -e --silent }
# Toolchain pinned by rust-toolchain.toml (1.79); ensure components
rustup toolchain install 1.79 --component rustfmt --component clippy
if (-not (Have node)) { winget install --id OpenJS.NodeJS.LTS -e --silent }
if (-not (Have uv))   { winget install --id astral-sh.uv -e --silent }

Step "2/8 Quality gates (pre-commit hooks)"
uv tool install pre-commit --force
if (Test-Path ".pre-commit-config.yaml") { pre-commit install }
else { Write-Host "  (no .pre-commit-config.yaml yet — will be added with first code commit)" }

Step "3/8 Model gateway (LiteLLM) + local inference"
uv tool install litellm --force
if ($WithOllama) {
  if (-not (Have ollama)) { winget install --id Ollama.Ollama -e --silent }
  ollama pull llama3.1
} else { Write-Host "  (skip Ollama; re-run with -WithOllama for local inference)" }

Step "4/8 Spec-Kit (execution framework)"
if (-not (Test-Path ".specify")) {
  uvx --from git+https://github.com/github/spec-kit.git specify init --here --integration claude --script ps --force --ignore-agent-tools
} else { Write-Host "  .specify/ present — OK" }

Step "5/8 BMAD-METHOD (planning framework)"
if (-not (Test-Path "_bmad")) {
  npx -y bmad-method@latest install --yes --modules bmm --tools claude-code --directory . --output-folder _bmad-output
} else { Write-Host "  _bmad/ present — OK" }

Step "6/8 Goose (agent runtime — evaluated by ADR 0001 spike)"
if (-not (Have goose)) {
  Write-Host "  Install per https://block-goose.mintlify.app/ (winget/scoop/release binary)." -ForegroundColor Yellow
  Write-Host "  Required for Task 0 spike; per ADR 0002 D1/D11 the integration path (goosed / embedded crate / native loop) is decided by the spike."
} else { goose --version }

Step "7/8 MCP connectors (embedded set; default = full local)"
Write-Host @"
  Offline connectors (fs/git/shell) need nothing. Network connectors (Atlassian, M365)
  are DISABLED by default (design 4.3). To develop against them, authenticate via:
    claude mcp   (interactive session)  — or the tool's own OAuth flow.
  Secrets are stored by reference only (keychain://...), never in files (design 7.13).
"@

Step "8/8 Verify"
$checks = @(
  @{n="git";      c={git --version}},
  @{n="rustc";    c={rustc --version}},
  @{n="cargo fmt";c={cargo fmt --version}},
  @{n="clippy";   c={cargo clippy --version}},
  @{n="node/npx"; c={node --version}},
  @{n="uv";       c={uv --version}},
  @{n="litellm";  c={litellm --version}},
  @{n="Spec-Kit"; c={Test-Path .specify}},
  @{n="BMAD";     c={Test-Path _bmad}}
)
$fail = 0
foreach ($k in $checks) {
  try { $out = & $k.c; Write-Host ("  [OK]   {0}: {1}" -f $k.n, ($out | Select-Object -First 1)) }
  catch { Write-Host ("  [FAIL] {0}" -f $k.n) -ForegroundColor Red; $fail++ }
}
if ($fail -eq 0) { Write-Host "`nBootstrap complete. See docs/DEVELOPMENT.md for next steps." -ForegroundColor Green }
else { Write-Host "`n$fail check(s) failed — fix and re-run (idempotent)." -ForegroundColor Yellow; exit 1 }
