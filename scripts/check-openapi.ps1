Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$specPath = Join-Path $PSScriptRoot "..\crates\api\openapi.json"
$spec = Get-Content -LiteralPath $specPath -Raw | ConvertFrom-Json

$requiredPaths = @(
  "/v1/auth/me",
  "/v1/service-accounts",
  "/v1/service-accounts/{id}/revoke",
  "/v1/jobs/runs",
  "/v1/jobs/runs/{id}/logs/stream",
  "/v1/workflow-runs/{id}/logs/stream",
  "/v1/audit-events",
  "/metrics"
)

foreach ($path in $requiredPaths) {
  if (-not $spec.paths.PSObject.Properties.Name.Contains($path)) {
    throw "OpenAPI spec is missing $path"
  }
}

if ($spec.openapi -notmatch "^3\.") {
  throw "OpenAPI version must be 3.x"
}

Write-Host "OpenAPI contract check passed for $($requiredPaths.Count) required paths."
