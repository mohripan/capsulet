param(
    [string]$RegistryPath = (Join-Path $PSScriptRoot "..\docs\contracts\product-claims.json"),
    [string]$GeneratedPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$defaultRegistry = (Join-Path $repositoryRoot "docs\contracts\product-claims.json")
$schemaPath = Join-Path $repositoryRoot "docs\contracts\product-claims.schema.json"
$registryFullPath = (Resolve-Path -LiteralPath $RegistryPath).Path
$raw = Get-Content -LiteralPath $registryFullPath -Raw

$testJson = Get-Command Test-Json -ErrorAction SilentlyContinue
if ($testJson) {
    $schemaValid = $raw | Test-Json -SchemaFile $schemaPath -ErrorAction SilentlyContinue
    if (-not $schemaValid) { throw "product claim registry does not satisfy product-claims.schema.json" }
}

$registry = $raw | ConvertFrom-Json
foreach ($requiredProperty in @("schema_version", "public_surfaces", "claims")) {
    if (-not $registry.PSObject.Properties.Name.Contains($requiredProperty)) {
        throw "registry is missing required property '$requiredProperty'"
    }
}
if ($registry.schema_version -ne 1) { throw "unsupported product claim schema_version '$($registry.schema_version)'" }

$validKinds = @("positioning", "capability", "guarantee", "limitation", "compatibility")
$validMaturities = @("implemented", "experimental", "planned")
$surfacePaths = @($registry.public_surfaces | ForEach-Object { $_.path })

$duplicateSurface = $surfacePaths | Group-Object | Where-Object Count -gt 1 | Select-Object -First 1
if ($duplicateSurface) { throw "duplicate public surface '$($duplicateSurface.Name)'" }

$claimIds = @($registry.claims | ForEach-Object { $_.id })
$duplicateId = $claimIds | Group-Object | Where-Object Count -gt 1 | Select-Object -First 1
if ($duplicateId) { throw "duplicate claim ID '$($duplicateId.Name)'" }

foreach ($claim in $registry.claims) {
    foreach ($requiredProperty in @("id", "kind", "maturity", "area", "statement", "public_surfaces", "evidence")) {
        if (-not $claim.PSObject.Properties.Name.Contains($requiredProperty)) {
            throw "claim '$($claim.id)' is missing required property '$requiredProperty'"
        }
    }
    if ($claim.id -notmatch '^CAP-[A-Z][A-Z0-9-]*-[0-9]{3}$') { throw "invalid claim ID '$($claim.id)'" }
    if ($validKinds -notcontains $claim.kind) { throw "claim '$($claim.id)' has invalid kind '$($claim.kind)'" }
    if ($validMaturities -notcontains $claim.maturity) { throw "claim '$($claim.id)' has invalid maturity '$($claim.maturity)'" }
    if (@($claim.public_surfaces).Count -eq 0) { throw "claim '$($claim.id)' must name at least one public surface" }

    foreach ($surface in $claim.public_surfaces) {
        if ($surfacePaths -notcontains $surface) { throw "claim '$($claim.id)' references undeclared public surface '$surface'" }
    }

    $testEvidence = @($claim.evidence | Where-Object type -eq "test")
    if ($claim.maturity -eq "implemented" -and $claim.kind -in @("capability", "guarantee") -and $testEvidence.Count -eq 0) {
        throw "implemented $($claim.kind) '$($claim.id)' requires executable test evidence"
    }

    foreach ($evidence in $claim.evidence) {
        $evidencePath = Join-Path $repositoryRoot $evidence.path
        if (-not (Test-Path -LiteralPath $evidencePath -PathType Leaf)) {
            throw "claim '$($claim.id)' referenced path does not exist: $($evidence.path)"
        }
        if ($evidence.type -eq "test") {
            if (-not $evidence.PSObject.Properties.Name.Contains("command") -or -not $evidence.command) {
                throw "test evidence for '$($claim.id)' is missing command"
            }
            if (-not $evidence.PSObject.Properties.Name.Contains("selector") -or -not $evidence.selector) {
                throw "test evidence for '$($claim.id)' is missing selector"
            }
            $evidenceContent = Get-Content -LiteralPath $evidencePath -Raw
            if (-not $evidenceContent.Contains([string]$evidence.selector)) {
                throw "test selector was not found for '$($claim.id)' in $($evidence.path): $($evidence.selector)"
            }
        }
    }
}

foreach ($surface in $registry.public_surfaces) {
    $surfacePath = Join-Path $repositoryRoot $surface.path
    if (-not (Test-Path -LiteralPath $surfacePath -PathType Leaf)) {
        throw "public surface referenced path does not exist: $($surface.path)"
    }
    $surfaceClaims = @($registry.claims | Where-Object { @($_.public_surfaces) -contains $surface.path })
    if ($surfaceClaims.Count -eq 0) { throw "unregistered public surface '$($surface.path)'" }
    if ($surface.marker_required) {
        $surfaceContent = Get-Content -LiteralPath $surfacePath -Raw
        foreach ($claim in $surfaceClaims) {
            if (-not $surfaceContent.Contains([string]$claim.id)) {
                throw "public surface '$($surface.path)' is missing claim marker '$($claim.id)'"
            }
        }
    }
}

if (-not $PSBoundParameters.ContainsKey("GeneratedPath") -and $registryFullPath -eq $defaultRegistry) {
    $GeneratedPath = Join-Path $repositoryRoot "docs\contracts\product-claims.md"
}
if ($GeneratedPath) {
    $temporaryPath = Join-Path ([System.IO.Path]::GetTempPath()) "capsulet-product-claims-$PID.md"
    try {
        & (Join-Path $PSScriptRoot "render-product-claims.ps1") -RegistryPath $registryFullPath -OutputPath $temporaryPath
        if (-not (Test-Path -LiteralPath $GeneratedPath -PathType Leaf) -or
            (Get-Content -LiteralPath $GeneratedPath -Raw) -cne (Get-Content -LiteralPath $temporaryPath -Raw)) {
            throw "generated Markdown is stale; run scripts/render-product-claims.ps1"
        }
    }
    finally {
        Remove-Item -LiteralPath $temporaryPath -Force -ErrorAction SilentlyContinue
    }
}

Write-Host "Product claim registry is valid ($($registry.claims.Count) claims, $($registry.public_surfaces.Count) public surfaces)."
