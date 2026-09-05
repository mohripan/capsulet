Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$aggregate = Get-Content -LiteralPath (Join-Path $repoRoot "scripts\check-contracts.ps1") -Raw
foreach ($required in @("capsulet-xtask", "--gate claims", "--gate api-contracts", "--gate sdk")) {
  if (-not $aggregate.Contains($required)) {
    throw "Unified contract gate does not delegate $required"
  }
}

foreach ($wrapper in @("check-contracts.ps1", "check-openapi.ps1", "check-sdk-contracts.ps1")) {
  $content = Get-Content -LiteralPath (Join-Path $repoRoot "scripts\$wrapper") -Raw
  if (-not $content.Contains("capsulet-xtask") -or $content -match "cargo test|npm test|python -m") {
    throw "$wrapper is not a thin xtask delegate"
  }
}

# CI must decide pass/fail with the same gate a developer runs, whether it calls
# the orchestrator directly or through the wrapper script. What is forbidden is a
# workflow that reimplements a check in YAML, because that is a second
# definition of "green" that drifts from the first.
$workflows = Get-ChildItem -LiteralPath (Join-Path $repoRoot ".github/workflows") -Filter "*.yml"
$contractGates = @("--gate claims", "--gate api-contracts", "--gate sdk")
$runsContracts = @($workflows | Where-Object {
    $content = Get-Content -LiteralPath $_.FullName -Raw
    $content.Contains("scripts/check-contracts.ps1") -or
    @($contractGates | Where-Object { $content.Contains($_) }).Count -gt 0
  })
if ($runsContracts.Count -eq 0) {
  throw "no workflow invokes the unified contract gate"
}
foreach ($workflow in $runsContracts) {
  $content = Get-Content -LiteralPath $workflow.FullName -Raw
  if (-not $content.Contains("capsulet-xtask") -and -not $content.Contains("scripts/check-contracts.ps1")) {
    throw "$($workflow.Name) checks contracts without going through the gate"
  }
}

# No workflow may hand-roll a check a gate already owns.
foreach ($workflow in $workflows) {
  $content = Get-Content -LiteralPath $workflow.FullName -Raw
  foreach ($reimplemented in @("cargo fmt", "cargo clippy", "cargo test")) {
    if ($content.Contains("run: $reimplemented")) {
      throw "$($workflow.Name) runs '$reimplemented' directly; call the gate that owns it"
    }
  }
}

# One orchestration path for IR runs.
#
# A run whose log two components can write is a run whose history nobody can
# explain: the fold that reconstructs it would be reconstructing the interleaving
# of two schedulers. Only the graph worker advances an IR run, and the only other
# place these names may appear is the storage adapter that defines them.
#
# Paths are compared with forward slashes throughout. The first version of this
# check spelled the allow-list with backslashes, so on Linux nothing matched it
# and the graph worker failed its own rule — a check that passed on one platform
# and failed on the other, which is worse than not having it.
$writers = @("append_ir_run_event", "lease_next_ir_run", "consume_ir_run_signal")
$allowed = @("crates/graph-worker/", "crates/postgres/")
$sources = Get-ChildItem -LiteralPath (Join-Path $repoRoot "crates") -Filter "*.rs" -Recurse
foreach ($source in $sources) {
  $relative = $source.FullName.Substring($repoRoot.Length + 1).Replace("\", "/")
  if (@($allowed | Where-Object { $relative.StartsWith($_) }).Count -gt 0) { continue }
  $content = Get-Content -LiteralPath $source.FullName -Raw
  foreach ($writer in $writers) {
    if ($content.Contains($writer)) {
      throw "$relative calls $writer; only the graph worker advances an IR run"
    }
  }
}

# And the components that own the compatibility path must not have grown a
# second one. They may enqueue an IR run; they may not execute it.
foreach ($component in @("scheduler", "evaluator")) {
  $componentRoot = Join-Path (Join-Path $repoRoot "crates") $component
  foreach ($source in (Get-ChildItem -LiteralPath $componentRoot -Filter "*.rs" -Recurse)) {
    $content = Get-Content -LiteralPath $source.FullName -Raw
    foreach ($forbidden in @("ir_run_events", "capsulet_runtime::decide", "GraphWorker")) {
      if ($content.Contains($forbidden)) {
        throw "crates/$component references $forbidden; IR runs advance through the graph worker only"
      }
    }
  }
}

Write-Host "Unified contract wiring test passed."
