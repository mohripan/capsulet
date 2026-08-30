Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$checker = Join-Path $repositoryRoot "scripts\check-product-claims.ps1"
$renderer = Join-Path $repositoryRoot "scripts\render-product-claims.ps1"
$fixtures = Join-Path $repositoryRoot "scripts\fixtures\contracts"
$failures = [System.Collections.Generic.List[string]]::new()

function Invoke-ClaimCheck {
    param(
        [Parameter(Mandatory = $true)][string]$RegistryPath,
        [string]$GeneratedPath
    )

    try {
        $arguments = @{ RegistryPath = $RegistryPath }
        if ($GeneratedPath) { $arguments.GeneratedPath = $GeneratedPath }
        $captured = & $checker @arguments 2>&1 | Out-String
        return @{ Succeeded = $true; Output = $captured }
    }
    catch {
        return @{ Succeeded = $false; Output = ($_ | Out-String) }
    }
}

function Assert-Check {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Registry,
        [Parameter(Mandatory = $true)][bool]$ShouldSucceed,
        [string]$ExpectedMessage,
        [string]$GeneratedPath
    )

    $result = Invoke-ClaimCheck -RegistryPath $Registry -GeneratedPath $GeneratedPath
    if ($result.Succeeded -ne $ShouldSucceed) {
        $failures.Add("$Name`: expected success=$ShouldSucceed, got success=$($result.Succeeded). $($result.Output)")
        return
    }
    if ($ExpectedMessage -and $result.Output -notmatch [regex]::Escape($ExpectedMessage)) {
        $failures.Add("$Name`: expected diagnostic '$ExpectedMessage'. Output: $($result.Output)")
    }
}

Assert-Check -Name "valid registry" -Registry (Join-Path $fixtures "valid-claims.json") -ShouldSucceed $true

$expectedPublicSurfaces = @(
    "README.md", "ARCHITECTURE.md",
    "docs/README.md", "docs/api.md", "docs/architecture.md", "docs/dashboard-streaming.md",
    "docs/development.md", "docs/helm-values.md", "docs/installation.md",
    "docs/local-kubernetes-runner.md", "docs/minikube-smoke.md", "docs/operations.md",
    "docs/operations/backup-restore-dr.md", "docs/operations/observability.md",
    "docs/persistence.md", "docs/security.md", "docs/security/secrets-rotation.md",
    "docs/troubleshooting.md", "docs/worker-runner.md",
    "sdk/python/README.md", "dashboard/README.md",
    "dashboard/app/artifacts/page.tsx", "dashboard/app/automations/page.tsx",
    "dashboard/app/components.tsx", "dashboard/app/execution-pools/page.tsx",
    "dashboard/app/job-definitions/page.tsx", "dashboard/app/layout.tsx",
    "dashboard/app/login/page.tsx", "dashboard/app/logs/page.tsx",
    "dashboard/app/memory/layout.tsx", "dashboard/app/memory/memory-ui.tsx",
    "dashboard/app/memory/page.tsx", "dashboard/app/page.tsx",
    "dashboard/app/runs/[id]/run-detail-client.tsx", "dashboard/app/runs/runs-client.tsx",
    "dashboard/app/security/page.tsx", "dashboard/app/settings/page.tsx",
    "dashboard/app/trigger-plugins/page.tsx", "dashboard/app/workflows/new/page.tsx",
    "dashboard/app/workflows/page.tsx", "dashboard/app/mock-data.ts",
    "examples/send-email/README.md", "examples/workflows/README.md",
    "charts/capsulet/Chart.yaml", "charts/capsulet/values.yaml",
    "charts/capsulet/values.schema.json", "crates/api/openapi.json"
)
$mainRegistry = Get-Content -LiteralPath (Join-Path $repositoryRoot "docs\contracts\product-claims.json") -Raw | ConvertFrom-Json
$declaredPublicSurfaces = @($mainRegistry.public_surfaces | ForEach-Object path)
foreach ($expectedSurface in $expectedPublicSurfaces) {
    if ($declaredPublicSurfaces -notcontains $expectedSurface) {
        $failures.Add("main registry is missing public surface '$expectedSurface'")
    }
}

$forbiddenPublicPhrases = @(
    "Capsulet is a local-first AI memory platform",
    "governed AI memory platform first",
    "Kubernetes-native automation platform and sandboxed job runner",
    "public-alpha stack"
)
foreach ($surface in $mainRegistry.public_surfaces) {
    $surfaceContent = Get-Content -LiteralPath (Join-Path $repositoryRoot $surface.path) -Raw
    foreach ($phrase in $forbiddenPublicPhrases) {
        if ($surfaceContent.Contains($phrase)) {
            $failures.Add("public surface '$($surface.path)' contains superseded product language '$phrase'")
        }
    }
}

$invalidCases = @(
    @{ File = "duplicate-ids.json"; Message = "duplicate claim ID" },
    @{ File = "unknown-maturity.json"; Message = "invalid maturity" },
    @{ File = "unknown-kind.json"; Message = "invalid kind" },
    @{ File = "missing-file.json"; Message = "referenced path does not exist" },
    @{ File = "missing-evidence.json"; Message = "requires executable test evidence" },
    @{ File = "invalid-selector.json"; Message = "test selector was not found" },
    @{ File = "unmarked-surface.json"; Message = "missing claim marker" },
    @{ File = "unregistered-surface.json"; Message = "unregistered public surface" },
    @{ File = "implemented-guarantee-without-test.json"; Message = "requires executable test evidence" }
)

foreach ($case in $invalidCases) {
    Assert-Check -Name $case.File `
        -Registry (Join-Path $fixtures "invalid-claims\$($case.File)") `
        -ShouldSucceed $false `
        -ExpectedMessage $case.Message
}

$temporaryMarkdown = Join-Path ([System.IO.Path]::GetTempPath()) "capsulet-stale-product-claims-$PID.md"
try {
    Set-Content -LiteralPath $temporaryMarkdown -Value "stale" -NoNewline
    Assert-Check -Name "stale generated Markdown" `
        -Registry (Join-Path $fixtures "valid-claims.json") `
        -GeneratedPath $temporaryMarkdown `
        -ShouldSucceed $false `
        -ExpectedMessage "generated Markdown is stale"
}
finally {
    Remove-Item -LiteralPath $temporaryMarkdown -Force -ErrorAction SilentlyContinue
}

if ($failures.Count -gt 0) {
    throw ($failures -join [Environment]::NewLine)
}

Write-Host "Product claim contract tests passed ($($invalidCases.Count + 2) cases)."
