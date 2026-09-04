# Skein — quickstart: from a clone or an extracted bundle to one real answer,
# produced by one real tool call, on a chain you can verify.
# Idempotent: safe to re-run; a second run appends to the same demo silo.
#   pwsh -File scripts/quickstart.ps1 [-Model NAME] [-BaseUrl URL] [-FsRoot PATH] [-Prompt TEXT]
param(
  [string]$Model,
  [string]$BaseUrl,
  [string]$FsRoot,
  [string]$Prompt
)

$ErrorActionPreference = "Stop"
function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Note($msg) { Write-Host "    $msg" }
function Warn($msg) { Write-Host "    $msg" -ForegroundColor Yellow }
function Fail($msg) { Write-Host "`nquickstart: $msg" -ForegroundColor Red; exit 1 }

# Before anything that could touch $IsWindows, which a 5.1 host does not define.
if ($PSVersionTable.PSVersion.Major -lt 7) {
  Write-Host "quickstart: needs PowerShell 7+, and this host is $($PSVersionTable.PSVersion)."
  Write-Host "Install it with 'winget install --id Microsoft.PowerShell -e', then re-run under pwsh."
  exit 1
}

# Two placements, one script. In a clone this file sits in scripts/ beside the
# workspace manifest and the crates it builds; in a bundle it sits beside
# skein.exe with neither. Both markers are checked, so a bundle extracted
# somewhere unlucky cannot be mistaken for a checkout.
$repoRoot = Join-Path $PSScriptRoot '..'
$SourceMode = (Test-Path (Join-Path $repoRoot 'Cargo.toml')) -and (Test-Path (Join-Path $repoRoot 'crates'))
$exeName = if ($IsWindows) { 'skein.exe' } else { 'skein' }

if (-not $BaseUrl) {
  # Mirrors ModelArgs::endpoint: the flag, else the environment, else Ollama's
  # own OpenAI-compatible endpoint. Passing --base-url unconditionally without
  # reading the variable first would silently override an operator who set it.
  $BaseUrl = if ($env:SKEIN_MODEL_BASE_URL) { $env:SKEIN_MODEL_BASE_URL } else { 'http://localhost:11434/v1' }
}

if (-not $FsRoot) {
  # In a clone the repo root is the obvious workspace, and being a git
  # repository it gets the git tools advertised as well as the fs tools. In a
  # bundle there is no project to point at, so the operator's current directory
  # is the only defensible default: deriving one from this script's own location
  # resolves to the whole of %TEMP% when the bundle is run from where a mail
  # client extracted it.
  $FsRoot = if ($SourceMode) { $repoRoot } else { (Get-Location).Path }
}
$FsRoot = (Resolve-Path -LiteralPath $FsRoot).Path

if (-not $Prompt) {
  $Prompt = 'Read the file README.md in the project root and answer in one short paragraph: what is Skein, and what is its current status?'
  if (-not (Test-Path (Join-Path $FsRoot 'README.md'))) {
    Fail "the demo prompt reads README.md, and $FsRoot has none. Run this from the folder you want the agent to read, name that folder with -FsRoot, or ask something else with -Prompt."
  }
}

# `parameter_size` is a human string ("8.0B", "27.3B", "500M"). Anything this
# cannot read sorts last rather than first, so an unparseable entry can never
# be chosen over a model whose size is known.
function Get-ParameterBillions($size) {
  if ($size -match '^\s*([\d.]+)\s*([BM])\s*$') {
    $n = [double]$Matches[1]
    return $(if ($Matches[2] -eq 'M') { $n / 1000 } else { $n })
  }
  return [double]::PositiveInfinity
}

Step "Placement"
if ($SourceMode) {
  Note "source checkout at $((Resolve-Path -LiteralPath $repoRoot).Path)"
} else {
  Note "bundle at $PSScriptRoot"
}

if ($SourceMode) {
  Step "Rust toolchain"
  if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Fail "cargo is not on PATH. Install the development dependencies first: pwsh -File $(Join-Path $PSScriptRoot 'bootstrap.ps1')"
  }
  # Channel read from rust-toolchain.toml and never hardcoded, so this script
  # cannot drift from the version the workspace pins. bootstrap.ps1 does the same.
  $channel = (Select-String -Path (Join-Path $repoRoot 'rust-toolchain.toml') -Pattern 'channel\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
  $rustcVersion = "$(rustc --version)"
  Note "$(cargo --version)"
  if ($rustcVersion -notmatch [regex]::Escape($channel)) {
    Warn "rustc reports '$rustcVersion' but rust-toolchain.toml pins $channel. With rustup installed cargo fetches the pinned toolchain anyway, and rust-version in the workspace manifest refuses a genuinely too-old compiler."
  }

  Step "Release build"
  # Always invoked, never guarded by a staleness check of our own: whether a
  # rebuild is needed is cargo's decision, and an already-current no-op costs
  # under a second.
  Note "cargo build --release -p skein-cli"
  & cargo build --release -p skein-cli --manifest-path (Join-Path $repoRoot 'Cargo.toml')
  if ($LASTEXITCODE -ne 0) {
    Fail @'
the release build failed. The likeliest cause on a fresh machine is a missing C toolchain: SQLite is
compiled from source by rusqlite's `bundled` feature and libgit2 by git2's `vendored-libgit2`, and
the failure surfaces deep inside a build script as a cc/link.exe error that names neither. See
"Machine prerequisites (not installed by the scripts)" in docs/DEVELOPMENT.md.
'@
  }
  $exe = Join-Path $repoRoot "target/release/$exeName"
} else {
  $exe = Join-Path $PSScriptRoot $exeName
}
if (-not (Test-Path -LiteralPath $exe)) { Fail "no Skein binary at $exe" }
$exe = (Resolve-Path -LiteralPath $exe).Path

Step "Local model provider"
# Tool capability is not on the OpenAI-compatible /v1/models route Skein itself
# talks to, only on Ollama's native /api/tags — so the probe strips /v1 back off.
$providerRoot = $BaseUrl -replace '/v1/?$', ''
Note "GET $providerRoot/api/tags"
try {
  $tags = Invoke-RestMethod -Uri "$providerRoot/api/tags" -TimeoutSec 5
} catch {
  # Deliberately not $_.Exception.Message. Against a refused port that text
  # arrives in the OS display language — measured French on this machine, in a
  # project whose content is English — and at a shorter timeout it has been
  # measured to blame an HttpClient timeout for a connection refused instantly.
  Fail "nothing answered at $providerRoot. Skein only ever talks to a provider on this machine over http, so this has to be a local one: start it with 'ollama serve' (install with 'winget install --id Ollama.Ollama -e'), or point elsewhere with -BaseUrl."
}

$models = @($tags.models)
if ($models.Count -eq 0) {
  Fail "$providerRoot is running but has no models. Pull a tool-capable one: 'ollama pull gemma4' (8B, about 9.6 GB on disk)."
}
$described = @($models | Where-Object { $null -ne $_.capabilities })
if ($described.Count -eq 0) {
  $ollamaVersion = try { (Invoke-RestMethod -Uri "$providerRoot/api/version" -TimeoutSec 5).version } catch { 'unknown' }
  Fail "$providerRoot listed $($models.Count) model(s) but reported a 'capabilities' field for none of them, so tool support cannot be checked here. That field comes from the server rather than the models: Ollama $ollamaVersion is too old to report it. Upgrade Ollama and re-run."
}
# Ascending parameter count, because the demo has to finish: on a given machine
# the largest tool-capable model installed is the one that exhausts its memory.
$toolCapable = @($described |
  Where-Object { $_.capabilities -contains 'tools' } |
  Sort-Object @{ Expression = { Get-ParameterBillions $_.details.parameter_size } }, size)
$toolCapableNames = if ($toolCapable.Count -gt 0) { $toolCapable.name -join ', ' } else { '(none)' }

Step "Model"
if ($Model) {
  $named = @($models | Where-Object { $_.name -eq $Model })[0]
  if (-not $named) {
    Fail "-Model '$Model' is not installed at $providerRoot. Installed and tool-capable: $toolCapableNames."
  }
  if ($named.capabilities -notcontains 'tools') {
    Fail "-Model '$Model' is installed but does not advertise tool support, and this demo is only worth something if a tool runs. Tool-capable here: $toolCapableNames."
  }
  Note "using model $Model (tool-capable), as asked"
} else {
  if ($toolCapable.Count -eq 0) {
    Fail "none of the $($models.Count) model(s) at $providerRoot advertises tool support, and this demo needs a tool call to mean anything. Pull one: 'ollama pull gemma4' (8B, about 9.6 GB on disk)."
  }
  $Model = $toolCapable[0].name
  Note "using model $Model ($($toolCapable[0].details.parameter_size), tool-capable) — the smallest tool-capable model installed"
  if ($toolCapable.Count -gt 1) {
    Note "pass -Model to choose another: $(@($toolCapable | Select-Object -Skip 1).name -join ', ')"
  }
}

Step "One real turn"
$siloRoot = if ($IsWindows) { Join-Path $env:LOCALAPPDATA 'skein/quickstart-demo' } else { Join-Path $HOME '.local/state/skein/quickstart-demo' }
New-Item -ItemType Directory -Force -Path $siloRoot | Out-Null
$siloRoot = (Resolve-Path -LiteralPath $siloRoot).Path
# Minted here and passed as --run-id so the ledger commands below need nothing
# parsed out of stderr, where `skein chat` prints the id it would otherwise mint.
$runId = "quickstart-$(Get-Date -Format 'yyyyMMddHHmmss')"
Note "silo root : $siloRoot"
Note "fs root   : $FsRoot"
Note "run id    : $runId"
Note "prompt    : $Prompt"
Write-Host ""
# --timeout-secs above the CLI's own default of 120: an 8B model that has to
# read a file and then answer from what it read has been measured close enough
# to 120s for the default to cut off a turn that was about to succeed.
& $exe chat --root $siloRoot --silo demo --run-id $runId `
  --model $Model --base-url $BaseUrl `
  --fs-root $FsRoot --timeout-secs 180 --prompt $Prompt
if ($LASTEXITCODE -ne 0) { Fail "the turn failed; the message above is the CLI's own." }

Step "The chain that turn wrote"
$log = & $exe ledger log --root $siloRoot --silo demo --run $runId
if ($LASTEXITCODE -ne 0) { Fail "reading the chain back failed." }
$log | ForEach-Object { Note $_ }
if (($log -join "`n") -notmatch 'tool_call') {
  # The one failure mode nothing else here can catch: a model without tool
  # support answers plausibly and errors nowhere, so the absence of this step
  # is the only evidence that Skein did not do the work.
  Warn "this chain has no tool_call step, so the model answered from its own weights and no Skein tool ran. Whatever it said above, this run did not demonstrate Skein."
}
Write-Host ""
& $exe ledger verify --root $siloRoot --silo demo
if ($LASTEXITCODE -ne 0) { Fail "the chain did not verify." }

Step "Done"
Note "the tool_call / approval / tool_result triple above is the agent reading a file through a"
Note "governed tool, and 'ledger verify' is the hash chain recomputed over every run in the silo."
Note ""
Note "re-running this script appends another run to the same silo, and verify checks them all."
Note "that silo is the only thing left behind; remove it with:"
Note "    Remove-Item -Recurse -Force '$siloRoot'"
