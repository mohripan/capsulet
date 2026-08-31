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

foreach ($workflow in @(".github\workflows\ci.yml", ".github\workflows\rust.yml")) {
  $content = Get-Content -LiteralPath (Join-Path $repoRoot $workflow) -Raw
  if (-not $content.Contains("scripts/check-contracts.ps1")) {
    throw "$workflow does not invoke the unified contract gate"
  }
}

Write-Host "Unified contract wiring test passed."
