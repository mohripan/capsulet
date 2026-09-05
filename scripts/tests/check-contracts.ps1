Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$aggregate = Get-Content -LiteralPath (Join-Path $repoRoot "scripts\check-contracts.ps1") -Raw
foreach ($required in @("capsulet-xtask", "--gate claims", "--gate api-contracts", "--gate sdk")) {
  if (-not $aggregate.Contains($required)) {
    throw "Unified contract gate does not delegate $required"
  }
}

foreach ($wrapper in @("check-contracts.ps1", "check-openapi.ps1", "check-sdk-contracts.ps1")) {
  $content = Get-Content -LiteralPath (Join-Path $repoRoot "scripts\$wrapper") -Raw
  if (-not $content.Contains("capsulet-xtask") -or $content -match "cargo test|npm test|python -m") {
    throw "$wrapper is not a thin xtask delegate"
  }
}

# CI must decide pass/fail with the same gate a developer runs, whether it calls
# the orchestrator directly or through the wrapper script. What is forbidden is a
# workflow that reimplements a check in YAML, because that is a second
# definition of "green" that drifts from the first.
$workflows = Get-ChildItem -LiteralPath (Join-Path $repoRoot ".github/workflows") -Filter "*.yml"
$contractGates = @("--gate claims", "--gate api-contracts", "--gate sdk")
$runsContracts = @($workflows | Where-Object {
    $content = Get-Content -LiteralPath $_.FullName -Raw
    $content.Contains("scripts/check-contracts.ps1") -or
    @($contractGates | Where-Object { $content.Contains($_) }).Count -gt 0
  })
if ($runsContracts.Count -eq 0) {
  throw "no workflow invokes the unified contract gate"
}
foreach ($workflow in $runsContracts) {
  $content = Get-Content -LiteralPath $workflow.FullName -Raw
  if (-not $content.Contains("capsulet-xtask") -and -not $content.Contains("scripts/check-contracts.ps1")) {
    throw "$($workflow.Name) checks contracts without going through the gate"
  }
}

# No workflow may hand-roll a check a gate already owns.
foreach ($workflow in $workflows) {
  $content = Get-Content -LiteralPath $workflow.FullName -Raw
  foreach ($reimplemented in @("cargo fmt", "cargo clippy", "cargo test")) {
    if ($content.Contains("run: $reimplemented")) {
      throw "$($workflow.Name) runs '$reimplemented' directly; call the gate that owns it"
    }
  }
}

Write-Host "Unified contract wiring test passed."
