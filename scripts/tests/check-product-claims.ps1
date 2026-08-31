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
        [string]$GeneratedPath,
        [string]$LifecyclePath
    )

    try {
        $arguments = @{ RegistryPath = $RegistryPath }
        if ($GeneratedPath) { $arguments.GeneratedPath = $GeneratedPath }
        if ($LifecyclePath) { $arguments.LifecyclePath = $LifecyclePath }
        $captured = & $checker @arguments 2>&1 | Out-String
        return @{ Succeeded = $true; Output = $captured }
    }
    catch {
        return @{ Succeeded = $false; Output = $_.Exception.Message }
    }
}

function Assert-Check {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Registry,
        [Parameter(Mandatory = $true)][bool]$ShouldSucceed,
        [string]$ExpectedMessage,
        [string]$GeneratedPath,
        [string]$LifecyclePath
    )

    $result = Invoke-ClaimCheck -RegistryPath $Registry -GeneratedPath $GeneratedPath -LifecyclePath $LifecyclePath
    if ($result.Succeeded -ne $ShouldSucceed) {
        $failures.Add("$Name`: expected success=$ShouldSucceed, got success=$($result.Succeeded). $($result.Output)")
        return
    }
    $normalizedOutput = [regex]::Replace($result.Output, '\x1B\[[0-?]*[ -/]*[@-~]', '')
    $normalizedOutput = [regex]::Replace($normalizedOutput, '\s+', ' ')
    $normalizedExpected = [regex]::Replace($ExpectedMessage, '\s+', ' ')
    if ($ExpectedMessage -and -not $normalizedOutput.Contains($normalizedExpected)) {
        $failures.Add("$Name`: expected diagnostic '$ExpectedMessage'. Output: $($result.Output)")
    }
}

Assert-Check -Name "valid registry" -Registry (Join-Path $fixtures "valid-claims.json") -ShouldSucceed $true

$mainRegistry = Get-Content -LiteralPath (Join-Path $repositoryRoot "docs\contracts\product-claims.json") -Raw | ConvertFrom-Json
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
    @{ File = "missing-accepted-adr.json"; Message = "requires evidence from an accepted ADR" },
    @{ File = "implemented-guarantee-without-test.json"; Message = "requires executable test evidence" },
    @{ File = "compound-claim-partial-evidence.json"; Message = "verification assertion 'adapter-kubernetes' has no executable test evidence" },
    @{ File = "selector-not-executed.json"; Message = "test selector was not executed" },
    @{ File = "command-does-not-select-test.json"; Message = "evidence command was not collected" }
)

foreach ($case in $invalidCases) {
    Assert-Check -Name $case.File `
        -Registry (Join-Path $fixtures "invalid-claims\$($case.File)") `
        -ShouldSucceed $false `
        -ExpectedMessage $case.Message
}

$invalidLifecycleCases = @(
    @{ File = "collapsed-execution-assurance.json"; Message = "execution and assurance vocabularies overlap" },
    @{ File = "undocumented-persisted-status.json"; Message = "status inventory differs from source enum" },
    @{ File = "invalid-transition-reference.json"; Message = "transition references unknown status" }
)
foreach ($case in $invalidLifecycleCases) {
    Assert-Check -Name $case.File `
        -Registry (Join-Path $fixtures "valid-claims.json") `
        -LifecyclePath (Join-Path $fixtures "invalid-lifecycle\$($case.File)") `
        -ShouldSucceed $false `
        -ExpectedMessage $case.Message
}

$temporaryPageDirectory = Join-Path $repositoryRoot "dashboard\app\__contract-test__"
$temporaryPage = Join-Path $temporaryPageDirectory "page.tsx"
try {
    New-Item -ItemType Directory -Path $temporaryPageDirectory -Force | Out-Null
    Set-Content -LiteralPath $temporaryPage -Value "export default function ContractTestPage() { return null }" -NoNewline
    Assert-Check -Name "new dashboard page requires registration" `
        -Registry (Join-Path $repositoryRoot "docs\contracts\product-claims.json") `
        -ShouldSucceed $false `
        -ExpectedMessage "discovered public surface 'dashboard/app/__contract-test__/page.tsx' is not registered"
}
finally {
    Remove-Item -LiteralPath $temporaryPage -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $temporaryPageDirectory -Force -ErrorAction SilentlyContinue
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

Write-Host "Product claim contract tests passed ($($invalidCases.Count + $invalidLifecycleCases.Count + 3) cases)."
