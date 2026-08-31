param(
    [string]$RegistryPath = (Join-Path $PSScriptRoot "..\docs\contracts\product-claims.json"),
    [string]$GeneratedPath,
    [string]$LifecyclePath,
    [string]$PublicSurfacesPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$defaultRegistry = (Join-Path $repositoryRoot "docs\contracts\product-claims.json")
$defaultLifecycle = (Join-Path $repositoryRoot "docs\contracts\lifecycle-mapping.json")
$defaultPublicSurfaces = (Join-Path $repositoryRoot "docs\contracts\public-surfaces.json")
$schemaPath = Join-Path $repositoryRoot "docs\contracts\product-claims.schema.json"
$registryFullPath = (Resolve-Path -LiteralPath $RegistryPath).Path
$raw = Get-Content -LiteralPath $registryFullPath -Raw

$testJson = Get-Command Test-Json -ErrorAction SilentlyContinue
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
$testManifest = if ($registry.PSObject.Properties.Name.Contains("test_manifest")) {
    @($registry.test_manifest)
}
else {
    @()
}

$duplicateSurface = $surfacePaths | Group-Object | Where-Object Count -gt 1 | Select-Object -First 1
if ($duplicateSurface) { throw "duplicate public surface '$($duplicateSurface.Name)'" }

$claimIds = @($registry.claims | ForEach-Object { $_.id })
$duplicateId = $claimIds | Group-Object | Where-Object Count -gt 1 | Select-Object -First 1
if ($duplicateId) { throw "duplicate claim ID '$($duplicateId.Name)'" }

$duplicateCommand = $testManifest | Group-Object command | Where-Object Count -gt 1 | Select-Object -First 1
if ($duplicateCommand) { throw "duplicate collected evidence command '$($duplicateCommand.Name)'" }

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
    if ($claim.kind -in @("positioning", "compatibility")) {
        $acceptedDecision = @($claim.evidence | Where-Object {
            if ($_.type -ne "decision" -or $_.path -notlike "docs/adr/*.md") { return $false }
            $decisionPath = Join-Path $repositoryRoot $_.path
            if (-not (Test-Path -LiteralPath $decisionPath -PathType Leaf)) { return $false }
            $decisionContent = Get-Content -LiteralPath $decisionPath -Raw
            return $decisionContent -match '(?im)^Status:\s*Accepted\s*$' -or
                $decisionContent -match '(?ims)^## Status\s+Accepted\s*(?:\r?\n|$)'
        })
        if ($acceptedDecision.Count -eq 0) {
            throw "$($claim.kind) claim '$($claim.id)' requires evidence from an accepted ADR"
        }
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
            $collectedCommand = @($testManifest | Where-Object command -CEQ ([string]$evidence.command))
            if ($collectedCommand.Count -eq 0) {
                throw "evidence command was not collected for '$($claim.id)': $($evidence.command)"
            }
            $collectedTest = @($collectedCommand[0].tests | Where-Object {
                $_.path -CEQ ([string]$evidence.path) -and $_.selector -CEQ ([string]$evidence.selector)
            })
            if ($collectedTest.Count -eq 0) {
                throw "test selector was not executed for '$($claim.id)' by '$($evidence.command)': $($evidence.selector)"
            }
        }
    }

    if ($claim.maturity -eq "implemented" -and $claim.kind -in @("capability", "guarantee")) {
        if (-not $claim.PSObject.Properties.Name.Contains("verification") -or @($claim.verification).Count -eq 0) {
            throw "implemented $($claim.kind) '$($claim.id)' requires verification assertions"
        }
        $testSelectors = @($testEvidence | ForEach-Object { [string]$_.selector })
        foreach ($assertion in $claim.verification) {
            if (-not $assertion.PSObject.Properties.Name.Contains("assertion") -or -not $assertion.assertion) {
                throw "claim '$($claim.id)' has a verification assertion without an ID"
            }
            $assertionSelectors = @($assertion.evidence_selectors)
            if ($assertionSelectors.Count -eq 0) {
                throw "verification assertion '$($assertion.assertion)' has no executable test evidence"
            }
            foreach ($selector in $assertionSelectors) {
                if ($testSelectors -notcontains $selector) {
                    throw "verification assertion '$($assertion.assertion)' references undeclared evidence selector '$selector'"
                }
            }
        }
    }
}

function Convert-PublicSurfaceGlobToRegex {
    param([Parameter(Mandatory = $true)][string]$Glob)

    $pattern = [regex]::Escape($Glob.Replace('\', '/'))
    $pattern = $pattern.Replace('\*\*/', '(?:.*/)?')
    $pattern = $pattern.Replace('\*\*', '.*')
    $pattern = $pattern.Replace('\*', '[^/]*')
    $pattern = $pattern.Replace('\?', '[^/]')
    return "^$pattern`$"
}

function Test-PublicSurfaceGlob {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Glob
    )

    return $Path -cmatch (Convert-PublicSurfaceGlobToRegex -Glob $Glob)
}

if (-not $PSBoundParameters.ContainsKey("PublicSurfacesPath") -and $registryFullPath -eq $defaultRegistry) {
    $PublicSurfacesPath = $defaultPublicSurfaces
}
if ($PublicSurfacesPath) {
    $surfaceConfigFullPath = (Resolve-Path -LiteralPath $PublicSurfacesPath).Path
    $surfaceConfigRaw = Get-Content -LiteralPath $surfaceConfigFullPath -Raw
    $surfaceConfig = $surfaceConfigRaw | ConvertFrom-Json
    if ($surfaceConfig.schema_version -ne 1) {
        throw "unsupported public surface schema_version '$($surfaceConfig.schema_version)'"
    }

    $excludedDirectoryRoots = @($surfaceConfig.exclude | Where-Object { $_.EndsWith('/**') } | ForEach-Object {
        $_.Substring(0, $_.Length - 3)
    })
    $repositoryFiles = [System.Collections.Generic.List[string]]::new()
    $pendingDirectories = [System.Collections.Generic.Stack[System.IO.DirectoryInfo]]::new()
    $pendingDirectories.Push([System.IO.DirectoryInfo]::new($repositoryRoot))
    while ($pendingDirectories.Count -gt 0) {
        $directory = $pendingDirectories.Pop()
        foreach ($file in $directory.EnumerateFiles()) {
            $relativePath = $file.FullName.Substring($repositoryRoot.Length).TrimStart('\', '/').Replace('\', '/')
            $repositoryFiles.Add($relativePath)
        }
        foreach ($childDirectory in $directory.EnumerateDirectories()) {
            $relativeDirectory = $childDirectory.FullName.Substring($repositoryRoot.Length).TrimStart('\', '/').Replace('\', '/')
            if ($excludedDirectoryRoots -notcontains $relativeDirectory) {
                $pendingDirectories.Push($childDirectory)
            }
        }
    }
    $discovered = [System.Collections.Generic.List[object]]::new()
    foreach ($path in $repositoryFiles) {
        $globallyExcluded = @($surfaceConfig.exclude | Where-Object { Test-PublicSurfaceGlob -Path $path -Glob $_ }).Count -gt 0
        if ($globallyExcluded) { continue }

        $matchingRules = @($surfaceConfig.rules | Where-Object {
            $rule = $_
            $included = @($rule.include | Where-Object { Test-PublicSurfaceGlob -Path $path -Glob $_ }).Count -gt 0
            $excluded = @($rule.exclude | Where-Object { Test-PublicSurfaceGlob -Path $path -Glob $_ }).Count -gt 0
            $included -and -not $excluded
        })
        if ($matchingRules.Count -gt 1) {
            throw "public surface '$path' matches multiple discovery categories"
        }
        if ($matchingRules.Count -eq 1) {
            $discovered.Add([pscustomobject]@{ path = $path; category = [string]$matchingRules[0].category })
        }
    }

    foreach ($surface in $discovered) {
        $registered = @($registry.public_surfaces | Where-Object path -CEQ $surface.path)
        if ($registered.Count -eq 0) {
            throw "discovered public surface '$($surface.path)' is not registered"
        }
        if ($registered[0].category -cne $surface.category) {
            throw "public surface '$($surface.path)' is registered as '$($registered[0].category)' but discovered as '$($surface.category)'"
        }
    }
    foreach ($surface in $registry.public_surfaces) {
        if (@($discovered | Where-Object path -CEQ ([string]$surface.path)).Count -eq 0) {
            throw "registered public surface '$($surface.path)' is not included by public-surfaces.json"
        }
    }

    $publicSurfaceSchemaPath = Join-Path $repositoryRoot "docs\contracts\public-surfaces.schema.json"
    if ($testJson -and (Test-Path -LiteralPath $publicSurfaceSchemaPath)) {
        $surfaceConfigValid = $surfaceConfigRaw | Test-Json -SchemaFile $publicSurfaceSchemaPath -ErrorAction SilentlyContinue
        if (-not $surfaceConfigValid) { throw "public surface configuration does not satisfy public-surfaces.schema.json" }
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

# Run structural schema validation after the targeted semantic checks so invalid fixtures retain
# actionable diagnostics on both Windows PowerShell (where Test-Json may be absent) and PowerShell 7.
if ($testJson) {
    $schemaValid = $raw | Test-Json -SchemaFile $schemaPath -ErrorAction SilentlyContinue
    if (-not $schemaValid) { throw "product claim registry does not satisfy product-claims.schema.json" }
}

if (-not $PSBoundParameters.ContainsKey("LifecyclePath") -and $registryFullPath -eq $defaultRegistry) {
    $LifecyclePath = $defaultLifecycle
}
if ($LifecyclePath) {
    $lifecycleFullPath = (Resolve-Path -LiteralPath $LifecyclePath).Path
    $lifecycleRaw = Get-Content -LiteralPath $lifecycleFullPath -Raw
    $lifecycleSchemaPath = Join-Path $repositoryRoot "docs\contracts\lifecycle-mapping.schema.json"
    $lifecycle = $lifecycleRaw | ConvertFrom-Json
    $requiredExecution = @("queued", "running", "waiting", "completed", "failed", "cancelled")
    $requiredAssurance = @("unverified", "accepted", "conditional", "rejected")
    $requiredKernel = @("accepted", "conditional", "rejected")

    function Assert-ExactSet {
        param([string]$Label, [object[]]$Actual, [string[]]$Expected)
        $actualValues = @($Actual | ForEach-Object { [string]$_ } | Sort-Object -Unique)
        $expectedValues = @($Expected | Sort-Object -Unique)
        if (($actualValues -join "|") -cne ($expectedValues -join "|")) {
            throw "$Label differs from the required vocabulary (actual: $($actualValues -join ', '))"
        }
    }

    $overlap = @($lifecycle.target_execution_statuses | Where-Object { @($lifecycle.platform_assurance_verdicts) -contains $_ })
    if ($overlap.Count -gt 0) {
        throw "execution and assurance vocabularies overlap: $($overlap -join ', ')"
    }
    Assert-ExactSet "target execution statuses" @($lifecycle.target_execution_statuses) $requiredExecution
    Assert-ExactSet "platform assurance verdicts" @($lifecycle.platform_assurance_verdicts) $requiredAssurance
    Assert-ExactSet "kernel verdicts" @($lifecycle.kernel_verdicts) $requiredKernel
    if (@($lifecycle.kernel_verdicts) -contains "unverified") {
        throw "unverified is a platform assurance state, not a kernel verdict"
    }

    $lifecycleNames = @($lifecycle.lifecycles | ForEach-Object name)
    $duplicateLifecycle = $lifecycleNames | Group-Object | Where-Object Count -gt 1 | Select-Object -First 1
    if ($duplicateLifecycle) { throw "duplicate lifecycle '$($duplicateLifecycle.Name)'" }

    foreach ($item in $lifecycle.lifecycles) {
        $sourcePath = Join-Path $repositoryRoot $item.source_path
        if (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) {
            throw "lifecycle '$($item.name)' source path does not exist: $($item.source_path)"
        }
        $sourceContent = Get-Content -LiteralPath $sourcePath -Raw
        $enumPattern = "(?s)pub\s+enum\s+$([regex]::Escape([string]$item.source_enum))\s*\{(?<body>.*?)\}"
        $enumMatch = [regex]::Match($sourceContent, $enumPattern)
        if (-not $enumMatch.Success) {
            throw "lifecycle '$($item.name)' source enum '$($item.source_enum)' was not found"
        }
        $sourceStatuses = @(
            $enumMatch.Groups["body"].Value -split "`n" |
                ForEach-Object { $_.Trim().TrimEnd(',') } |
                Where-Object { $_ -match '^[A-Z][A-Za-z0-9_]*$' } |
                ForEach-Object { [regex]::Replace($_, '(?<!^)([A-Z])', '_$1').ToLowerInvariant() }
        )
        $documentedStatuses = @($item.statuses | ForEach-Object name)
        $sourceSet = @($sourceStatuses | Sort-Object -Unique)
        $documentedSet = @($documentedStatuses | Sort-Object -Unique)
        if (($sourceSet -join "|") -cne ($documentedSet -join "|")) {
            throw "lifecycle '$($item.name)' status inventory differs from source enum '$($item.source_enum)' (source: $($sourceSet -join ', '); documented: $($documentedSet -join ', '))"
        }

        foreach ($status in $item.statuses) {
            if ($item.category -eq "execution" -and $null -eq $status.target) {
                throw "execution status '$($item.name).$($status.name)' must map to a target execution status"
            }
            if ($null -ne $status.target -and $requiredExecution -notcontains $status.target) {
                throw "status '$($item.name).$($status.name)' maps to unknown target '$($status.target)'"
            }
        }
        foreach ($transition in $item.transitions) {
            if ($documentedStatuses -notcontains $transition.from -or $documentedStatuses -notcontains $transition.to) {
                throw "lifecycle '$($item.name)' transition references unknown status '$($transition.from) -> $($transition.to)'"
            }
        }
    }

    if ($testJson -and (Test-Path -LiteralPath $lifecycleSchemaPath)) {
        $lifecycleSchemaValid = $lifecycleRaw | Test-Json -SchemaFile $lifecycleSchemaPath -ErrorAction SilentlyContinue
        if (-not $lifecycleSchemaValid) { throw "lifecycle mapping does not satisfy lifecycle-mapping.schema.json" }
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
