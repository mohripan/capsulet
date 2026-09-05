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

# Ordered by an explicit ordinal insertion, and not by the two obvious
# alternatives. `Sort-Object` compares strings the way the current culture would.
# `[Array]::Sort($keys, $items, $comparer)` looks right and, here, bound to an
# overload that silently reordered nothing on Windows while sorting on Linux —
# no error, just two different documents from one registry, which the staleness
# check then reported as the second one being stale.
#
# A generated artefact has to come out byte-identical wherever it is rendered.
# With a few dozen claims the cost of an insertion sort is irrelevant next to
# being able to see exactly what it does.
$ordered = [System.Collections.Generic.List[object]]::new()
foreach ($claim in $registry.claims) {
    $key = "$([string]$claim.area)`u{1}$([string]$claim.maturity)`u{1}$([string]$claim.id)"
    $at = 0
    while ($at -lt $ordered.Count -and
           [string]::CompareOrdinal([string]$ordered[$at].Key, $key) -le 0) {
        $at++
    }
    $ordered.Insert($at, [pscustomobject]@{ Key = $key; Claim = $claim })
}
# Sections are opened as the ordered list changes area, rather than by
# Group-Object. Grouping is the last step whose ordering this script would not
# control: the claims arrive here in a known order and leave in it, and a new
# heading is simply where the area changes.
$area = $null
foreach ($entry in $ordered) {
    $claim = $entry.Claim
    if ([string]$claim.area -cne $area) {
        if ($null -ne $area) { $lines.Add("") }
        $area = [string]$claim.area
        $lines.Add("## $area")
        $lines.Add("")
        $lines.Add("| ID | Maturity | Kind | Claim |")
        $lines.Add("| --- | --- | --- | --- |")
    }
    $statement = $claim.statement.Replace("|", "\|").Replace("`r", " ").Replace("`n", " ")
    $lines.Add("| ``$($claim.id)`` | $($claim.maturity) | $($claim.kind) | $statement |")
}
$lines.Add("")

$rendered = ($lines -join "`n").TrimEnd() + "`n"
if ($OutputPath) {
    [System.IO.File]::WriteAllText($OutputPath, $rendered, [System.Text.UTF8Encoding]::new($false))
}
else {
    Write-Output $rendered -NoEnumerate
}
