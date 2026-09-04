# Pester tests for the pure logic in quickstart.ps1: where it points the agent,
# which model it picks, and which provider it probes.
#   Install-Module Pester -MinimumVersion 5.0 -Scope CurrentUser
#   Invoke-Pester -Path scripts/quickstart.Tests.ps1
#
# quickstart.ps1 ships as a single file inside the bundle, so the functions under
# test cannot live in a module beside it and the script cannot be dot-sourced
# without running the whole demo. The definitions are lifted out of the real file
# by the parser instead, which keeps the tests honest about what is shipped.
BeforeAll {
  $quickstart = Join-Path $PSScriptRoot 'quickstart.ps1'
  $ast = [System.Management.Automation.Language.Parser]::ParseFile($quickstart, [ref]$null, [ref]$null)
  foreach ($name in 'Resolve-Placement', 'Get-ParameterBillions') {
    $definition = $ast.Find(
      { param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name },
      $true)
    if (-not $definition) { throw "quickstart.ps1 defines no $name function" }
    . ([scriptblock]::Create($definition.Extent.Text))
  }

  # The local-provider default is four inline lines rather than a function, so
  # what gets lifted is the pair of literals it is built from: the environment
  # variable it reads and the string it falls back to. Both are pulled from the
  # one assignment that establishes the default, so moving that assignment is a
  # loud failure here rather than a quiet one.
  $baseUrlDefault = @($ast.FindAll(
    { param($node)
      $node -is [System.Management.Automation.Language.AssignmentStatementAst] -and
      $node.Left.Extent.Text -eq '$BaseUrl' },
    $true))
  if ($baseUrlDefault.Count -ne 1) {
    throw "quickstart.ps1 assigns `$BaseUrl $($baseUrlDefault.Count) times; this test lifts its default from exactly one"
  }
  $script:quickstartEnvVars = @($baseUrlDefault[0].Right.FindAll(
      { param($node)
        $node -is [System.Management.Automation.Language.VariableExpressionAst] -and
        $node.VariablePath.DriveName -eq 'env' },
      $true) |
    ForEach-Object { $_.VariablePath.UserPath -replace '^env:', '' } |
    Select-Object -Unique)
  $script:quickstartBaseUrls = @($baseUrlDefault[0].Right.FindAll(
      { param($node) $node -is [System.Management.Automation.Language.StringConstantExpressionAst] },
      $true) |
    ForEach-Object { $_.Value } |
    Select-Object -Unique)

  # Rust has no parser to reach for here, so the owning side is lifted by
  # regex. Every match is collected rather than the first taken: a second
  # environment variable or a second constant in wiring.rs would make a
  # first-match lift quietly test the wrong one.
  $wiring = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot '../crates/skein-cli/src/wiring.rs')
  $script:wiringEnvVars = @([regex]::Matches($wiring, 'std::env::var\("([^"]+)"\)') |
    ForEach-Object { $_.Groups[1].Value })
  $script:wiringBaseUrls = @([regex]::Matches($wiring, 'const DEFAULT_BASE_URL: &str = "([^"]+)"') |
    ForEach-Object { $_.Groups[1].Value })
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

Describe 'Get-ParameterBillions' {
  It 'orders by ascending billions, converting M to B, and sorts unparseable last' {
    $sizes = '27.3B', '500M', 'bogus', '8.0B'

    $ordered = $sizes | Sort-Object { Get-ParameterBillions $_ }

    $ordered | Should -Be @('500M', '8.0B', '27.3B', 'bogus')
  }

  It 'reads M as thousandths of a B and gives an unreadable size no size at all' {
    Get-ParameterBillions '8.0B' | Should -Be 8.0
    Get-ParameterBillions '500M' | Should -Be 0.5
    Get-ParameterBillions 'bogus' | Should -Be ([double]::PositiveInfinity)
  }
}

Describe 'the local-provider default' {
  # quickstart.ps1 probes the endpoint `skein chat` will reach for when the
  # operator passes neither -BaseUrl nor the environment variable, and it names
  # that endpoint in literals of its own. wiring.rs owns them; this is the only
  # thing crossing the Rust/PowerShell boundary, so without it a rename or a
  # second local-provider default diverges in silence and the probe checks a
  # provider the CLI will not use.
  It 'names the same environment variable and fallback URL as ModelArgs::endpoint' {
    $script:quickstartEnvVars | Should -Be $script:wiringEnvVars
    $script:quickstartBaseUrls | Should -Be $script:wiringBaseUrls
  }
}
