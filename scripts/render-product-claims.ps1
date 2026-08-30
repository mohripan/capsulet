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

$areaGroups = @($registry.claims | Sort-Object area, maturity, id | Group-Object area)
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
