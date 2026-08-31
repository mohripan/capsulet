Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

& (Join-Path $PSScriptRoot "tests\check-contracts.ps1")
& (Join-Path $PSScriptRoot "tests\check-product-claims.ps1")
& (Join-Path $PSScriptRoot "check-product-claims.ps1")
& (Join-Path $PSScriptRoot "check-openapi.ps1")
& (Join-Path $PSScriptRoot "check-sdk-contracts.ps1")

Write-Host "Unified product contract gate passed."
