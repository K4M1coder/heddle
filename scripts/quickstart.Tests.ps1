# Pester tests for the placement logic in quickstart.ps1.
#   Install-Module Pester -MinimumVersion 5.0 -Scope CurrentUser
#   Invoke-Pester -Path scripts/quickstart.Tests.ps1
#
# quickstart.ps1 ships as a single file inside the bundle, so the function under
# test cannot live in a module beside it and the script cannot be dot-sourced
# without running the whole demo. The definition is lifted out of the real file
# by the parser instead, which keeps the test honest about what is shipped.
BeforeAll {
  $quickstart = Join-Path $PSScriptRoot 'quickstart.ps1'
  $ast = [System.Management.Automation.Language.Parser]::ParseFile($quickstart, [ref]$null, [ref]$null)
  $definition = $ast.Find(
    { param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Resolve-Placement' },
    $true)
  if (-not $definition) { throw "quickstart.ps1 defines no Resolve-Placement function" }
  . ([scriptblock]::Create($definition.Extent.Text))
}

Describe 'Resolve-Placement' {
  BeforeEach {
    # A bundle extracted where a mail client put it: no workspace manifest in it
    # or above it, and an operator sitting somewhere else entirely.
    $script:bundleDir = Join-Path ([System.IO.Path]::GetTempPath()) "skein-placement-bundle-$(New-Guid)/skein-0.0.0-windows-x64"
    $script:operatorCwd = Join-Path ([System.IO.Path]::GetTempPath()) "skein-placement-cwd-$(New-Guid)"
    New-Item -ItemType Directory -Force -Path $script:bundleDir, $script:operatorCwd | Out-Null
    Set-Content -LiteralPath (Join-Path $script:operatorCwd 'marker.txt') -Value 'the folder the operator ran from'
  }

  AfterEach {
    Remove-Item -Recurse -Force -LiteralPath (Split-Path -Parent $script:bundleDir), $script:operatorCwd -ErrorAction SilentlyContinue
  }

  It 'defaults the fs root to the working directory in bundle mode, never to the bundle or its parent' {
    Push-Location -LiteralPath $script:operatorCwd
    try {
      $placement = Resolve-Placement -ScriptDirectory $script:bundleDir
    } finally {
      Pop-Location
    }

    $expected = (Resolve-Path -LiteralPath $script:operatorCwd).Path
    $placement.SourceMode | Should -BeFalse
    $placement.FsRoot | Should -Be $expected
    # Named apart from the equality above because handing the agent %TEMP% is
    # the measured bug this test exists to keep out: a parent-of-script default
    # reads as merely "wrong path" until you see which path it is.
    $placement.FsRoot | Should -Not -Be (Resolve-Path -LiteralPath (Split-Path -Parent $script:bundleDir)).Path
  }

  It 'defaults the fs root to the repo root in a source checkout' {
    $repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
    Push-Location -LiteralPath $script:operatorCwd
    try {
      $placement = Resolve-Placement -ScriptDirectory $PSScriptRoot
    } finally {
      Pop-Location
    }

    $placement.SourceMode | Should -BeTrue
    $placement.FsRoot | Should -Be $repoRoot
  }

  It 'honours an explicit fs root in either placement' {
    Push-Location -LiteralPath $script:operatorCwd
    try {
      $placement = Resolve-Placement -ScriptDirectory $script:bundleDir -FsRoot $script:bundleDir
    } finally {
      Pop-Location
    }

    $placement.FsRoot | Should -Be (Resolve-Path -LiteralPath $script:bundleDir).Path
  }
}
