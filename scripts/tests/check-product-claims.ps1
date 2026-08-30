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

$invalidCases = @(
    @{ File = "duplicate-ids.json"; Message = "duplicate claim ID" },
    @{ File = "unknown-maturity.json"; Message = "invalid maturity" },
    @{ File = "unknown-kind.json"; Message = "invalid kind" },
    @{ File = "missing-file.json"; Message = "referenced path does not exist" },
    @{ File = "missing-evidence.json"; Message = "requires executable test evidence" },
    @{ File = "invalid-selector.json"; Message = "test selector was not found" },
    @{ File = "unmarked-surface.json"; Message = "missing claim marker" },
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
