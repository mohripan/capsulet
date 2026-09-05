param(
    [string]$RegistryPath = (Join-Path $PSScriptRoot "..\docs\contracts\product-claims.json"),
    [string]$OutputPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$registry = Get-Content -LiteralPath $RegistryPath -Raw | ConvertFrom-Json
$lines = [System.Collections.Generic.List[string]]::new()
$lines.Add("# Product Claims")
$lines.Add("")
$lines.Add("This file is generated from ``docs/contracts/product-claims.json``. Do not edit it directly.")
$lines.Add("")

# Sorted ordinally rather than with Sort-Object, which compares strings the way
# the current culture would. Windows and Linux disagree about where a space or a
# hyphen sorts, so the same registry rendered on two machines produced two
# different documents and the staleness check called one of them stale. Byte
# order is the same everywhere, which is what a generated artefact needs.
$claims = [object[]]@($registry.claims)
$keys = [string[]]@($claims | ForEach-Object {
        "$([string]$_.area)`u{1}$([string]$_.maturity)`u{1}$([string]$_.id)"
    })
[Array]::Sort($keys, $claims, [System.StringComparer]::Ordinal)
$areaGroups = @($claims | Group-Object area)
foreach ($areaGroup in $areaGroups) {
    $lines.Add("## $($areaGroup.Name)")
    $lines.Add("")
    $lines.Add("| ID | Maturity | Kind | Claim |")
    $lines.Add("| --- | --- | --- | --- |")
    foreach ($claim in $areaGroup.Group) {
        $statement = $claim.statement.Replace("|", "\|").Replace("`r", " ").Replace("`n", " ")
        $lines.Add("| ``$($claim.id)`` | $($claim.maturity) | $($claim.kind) | $statement |")
    }
    $lines.Add("")
}

$rendered = ($lines -join "`n").TrimEnd() + "`n"
if ($OutputPath) {
    [System.IO.File]::WriteAllText($OutputPath, $rendered, [System.Text.UTF8Encoding]::new($false))
}
else {
    Write-Output $rendered -NoEnumerate
}
