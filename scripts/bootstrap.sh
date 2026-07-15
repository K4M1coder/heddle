#!/usr/bin/env bash
# Skein — development environment bootstrap (macOS / Linux)
# Idempotent: safe to re-run. Run from the repo root right after `git clone`.
#   ./scripts/bootstrap.sh [--with-ollama]
set -euo pipefail
WITH_OLLAMA="${1:-}"
step() { printf '\n\033[36m==> %s\033[0m\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }

step "1/8 Core tooling (git, Rust, Node, Python/uv)"
have git || { echo "Install git via your package manager first."; exit 1; }
have rustup || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
rustup toolchain install 1.79 --component rustfmt --component clippy
have node || { echo "Install Node.js LTS (nvm/brew/apt) then re-run."; exit 1; }
have uv || curl -LsSf https://astral.sh/uv/install.sh | sh

step "2/8 Quality gates (pre-commit hooks)"
uv tool install pre-commit --force
[ -f .pre-commit-config.yaml ] && pre-commit install || echo "  (no .pre-commit-config.yaml yet — added with first code commit)"

step "3/8 Model gateway (LiteLLM) + local inference"
uv tool install litellm --force
if [ "$WITH_OLLAMA" = "--with-ollama" ]; then
  have ollama || curl -fsSL https://ollama.com/install.sh | sh
  ollama pull llama3.1
else
  echo "  (skip Ollama; re-run with --with-ollama for local inference)"
fi

step "4/8 Spec-Kit (execution framework)"
if [ ! -d .specify ]; then
  uvx --from git+https://github.com/github/spec-kit.git specify init --here --integration claude --script sh --force --ignore-agent-tools
else echo "  .specify/ present — OK"; fi

step "5/8 BMAD-METHOD (planning framework)"
if [ ! -d _bmad ]; then
  npx -y bmad-method@latest install --yes --modules bmm --tools claude-code --directory . --output-folder _bmad-output
else echo "  _bmad/ present — OK"; fi

step "6/8 Goose (agent runtime — evaluated by ADR 0001 spike)"
if have goose; then goose --version; else
  echo "  Install per https://block-goose.mintlify.app/ (brew/release binary)."
  echo "  Required for Task 0 spike; ADR 0002 D1/D11: integration path (goosed / embedded crate / native loop) decided by the spike."
fi

step "7/8 MCP connectors (embedded set; default = full local)"
cat <<'EOF'
  Offline connectors (fs/git/shell) need nothing. Network connectors (Atlassian, M365)
  are DISABLED by default (design 4.3). To develop against them, authenticate via:
    claude mcp   (interactive session)  — or the tool's own OAuth flow.
  Secrets are stored by reference only (keychain://...), never in files (design 7.13).
EOF

step "8/8 Verify"
fail=0
check() { if out=$($2 2>&1 | head -1); then printf '  [OK]   %s: %s\n' "$1" "$out"; else printf '  [FAIL] %s\n' "$1"; fail=$((fail+1)); fi; }
check git        "git --version"
check rustc      "rustc --version"
check "cargo fmt" "cargo fmt --version"
check clippy     "cargo clippy --version"
check node       "node --version"
check uv         "uv --version"
check litellm    "litellm --version"
[ -d .specify ] && echo "  [OK]   Spec-Kit: .specify/" || { echo "  [FAIL] Spec-Kit"; fail=$((fail+1)); }
[ -d _bmad ]    && echo "  [OK]   BMAD: _bmad/"        || { echo "  [FAIL] BMAD"; fail=$((fail+1)); }
if [ "$fail" -eq 0 ]; then printf '\n\033[32mBootstrap complete. See docs/DEVELOPMENT.md for next steps.\033[0m\n'
else printf '\n\033[33m%s check(s) failed — fix and re-run (idempotent).\033[0m\n' "$fail"; exit 1; fi
