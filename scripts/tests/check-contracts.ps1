Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$aggregate = Get-Content -LiteralPath (Join-Path $repoRoot "scripts\check-contracts.ps1") -Raw
foreach ($required in @(
  "tests\check-product-claims.ps1",
  "check-product-claims.ps1",
  "check-openapi.ps1",
  "check-sdk-contracts.ps1"
)) {
  if (-not $aggregate.Contains($required)) {
    throw "Unified contract gate does not invoke $required"
  }
}

foreach ($workflow in @(".github\workflows\ci.yml", ".github\workflows\rust.yml")) {
  $content = Get-Content -LiteralPath (Join-Path $repoRoot $workflow) -Raw
  if (-not $content.Contains("scripts/check-contracts.ps1")) {
    throw "$workflow does not invoke the unified contract gate"
  }
}

Write-Host "Unified contract wiring test passed."
