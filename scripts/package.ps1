# Skein — assemble the distributable Windows bundle and its zip.
# The operator's tool, never the colleague's: it needs the checkout the
# quickstart it packages is meant to make unnecessary.
#   pwsh -File scripts/package.ps1

$ErrorActionPreference = "Stop"
function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Note($msg) { Write-Host "    $msg" }
function Fail($msg) { Write-Host "`npackage: $msg" -ForegroundColor Red; exit 1 }

if ($PSVersionTable.PSVersion.Major -lt 7) { Fail "needs PowerShell 7+; this host is $($PSVersionTable.PSVersion)." }
if (-not $IsWindows) { Fail "this builds the windows-x64 bundle and has to run on Windows." }

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
# Read, never retyped: the bundle's name and the binary's own --version come
# from the same [workspace.package] field, so they cannot disagree.
$version = (Select-String -Path (Join-Path $repoRoot 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
$bundleName = "skein-$version-windows-x64"
$distDir = Join-Path $repoRoot 'dist'
$bundleDir = Join-Path $distDir $bundleName
$zipPath = Join-Path $distDir "$bundleName.zip"

Step "Release build"
# --workspace and not just -p skein-cli: packaging must not be able to outrun
# the gate that the whole workspace still builds in release.
& cargo build --workspace --release --manifest-path (Join-Path $repoRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { Fail "the release build failed; nothing was packaged." }

Step "Assemble $bundleName"
if (Test-Path -LiteralPath $bundleDir) { Remove-Item -Recurse -Force -LiteralPath $bundleDir }
New-Item -ItemType Directory -Force -Path $bundleDir | Out-Null
# skein.pdb is deliberately absent: 7 MiB of separate debug symbols that serve
# nobody without the matching source tree, on a 13 MiB payload.
$payload = @(
  (Join-Path $repoRoot 'target/release/skein.exe'),
  (Join-Path $PSScriptRoot 'quickstart.ps1'),
  (Join-Path $repoRoot 'QUICKSTART.md'),
  (Join-Path $repoRoot 'README.md'),
  (Join-Path $repoRoot 'LICENSE')
)
foreach ($file in $payload) {
  if (-not (Test-Path -LiteralPath $file)) { Fail "missing from the payload: $file" }
  Copy-Item -LiteralPath $file -Destination $bundleDir
}

Step "Compress"
if (Test-Path -LiteralPath $zipPath) { Remove-Item -Force -LiteralPath $zipPath }
Compress-Archive -Path $bundleDir -DestinationPath $zipPath

Step "Done"
Get-ChildItem -LiteralPath $bundleDir | ForEach-Object { Note ("{0,12:N0}  {1}" -f $_.Length, $_.Name) }
Note ""
Note ("{0,12:N0}  {1}" -f (Get-Item -LiteralPath $zipPath).Length, "$bundleName.zip")
Note ""
Note "hand over either one:"
Note "    the folder  $bundleDir"
Note "    the zip     $zipPath"
Note "the colleague's instructions are QUICKSTART.md inside it."
