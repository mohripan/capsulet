Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Push-Location $repoRoot
try {
  & cargo test -p capsulet-api --test openapi_contract
  if ($LASTEXITCODE -ne 0) {
    throw "OpenAPI contract tests failed."
  }

  & cargo run -p capsulet-api --bin export-openapi -- --check
  if ($LASTEXITCODE -ne 0) {
    throw "The checked OpenAPI artifact is stale."
  }

  $specPath = Join-Path $repoRoot "crates\api\openapi.json"
  $spec = Get-Content -LiteralPath $specPath -Raw | ConvertFrom-Json
  $pathCount = @($spec.paths.PSObject.Properties).Count
  $operationCount = 0
  foreach ($path in $spec.paths.PSObject.Properties) {
    $operationCount += @($path.Value.PSObject.Properties | Where-Object {
      $_.Name -in @("get", "post", "put", "delete", "patch")
    }).Count
  }
  if ($pathCount -ne 90 -or $operationCount -ne 116) {
    throw "Expected 90 paths and 116 operations, found $pathCount paths and $operationCount operations."
  }
  Write-Host "OpenAPI contract check passed for $operationCount generated operations across $pathCount paths."
}
finally {
  Pop-Location
}
