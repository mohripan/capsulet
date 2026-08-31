Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Push-Location $repoRoot
try {
  & python -m unittest discover -s sdk/python/tests -p "test_*.py"
  if ($LASTEXITCODE -ne 0) {
    throw "Python SDK unit/OpenAPI contract tests failed."
  }

  Push-Location (Join-Path $repoRoot "dashboard")
  try {
    & npm test
    if ($LASTEXITCODE -ne 0) {
      throw "Dashboard unit/OpenAPI contract tests failed."
    }
    & npx tsc --noEmit
    if ($LASTEXITCODE -ne 0) {
      throw "Dashboard TypeScript contract check failed."
    }
  }
  finally {
    Pop-Location
  }
  Write-Host "SDK/client contracts passed for Python and dashboard transports."
}
finally {
  Pop-Location
}
