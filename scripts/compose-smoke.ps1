param(
  [int]$TimeoutSeconds = 180
)

# A job run through a running stack: it is leased, executed, and leaves logs and
# a downloadable artifact. Start the stack first, with
# `cargo run -p capsulet-xtask -- stack up`; the `compose` gate does that itself.

$ErrorActionPreference = "Stop"
. "$PSScriptRoot/stack-fixture.ps1"

Write-Host "Checking identity and OpenAPI..."
$me = Invoke-CapsuletJson -Method GET -Path "/v1/auth/me"
if ($me.role -ne "admin" -or -not $me.platform_admin) { throw "Expected a platform admin, got $($me.role)." }
Invoke-RestMethod -Uri "$BaseUrl/openapi.json" | Out-Null

Write-Host "Submitting a run..."
$run = Invoke-CapsuletJson -Method POST -Path "/v1/jobs/runs" -Body @{
  job_definition_id = "job_artifact_python"
  execution_pool = "mini"
}

$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
do {
  $current = Invoke-CapsuletJson -Method GET -Path "/v1/jobs/runs/$($run.id)"
  if ($current.status -in @("succeeded", "failed", "cancelled", "timed_out")) { break }
  if ((Get-Date) -gt $deadline) { throw "Run $($run.id) did not finish within $TimeoutSeconds seconds." }
  Start-Sleep -Seconds 2
} while ($true)
if ($current.status -ne "succeeded") { throw "Run $($run.id) finished with status $($current.status)." }

$logs = Invoke-CapsuletJson -Method GET -Path "/v1/jobs/runs/$($run.id)/logs"
if ([string]::IsNullOrWhiteSpace($logs.logs)) { throw "Run $($run.id) had empty logs." }

$artifacts = @((Invoke-CapsuletJson -Method GET -Path "/v1/jobs/runs/$($run.id)/artifacts").artifacts)
if ($artifacts.Count -lt 1) { throw "Run $($run.id) recorded no artifact." }
$download = Invoke-WebRequest -UseBasicParsing -Uri "$BaseUrl/v1/jobs/runs/$($run.id)/artifacts/$($artifacts[0].id)" -Headers @{
  Authorization = "Bearer $($Fixture.identities.admin.token)"
}
$content = if ($download.Content -is [byte[]]) { [Text.Encoding]::UTF8.GetString($download.Content) } else { [string]$download.Content }
if ($content -notmatch "artifact from capsulet") {
  throw "Artifact $($artifacts[0].id) of run $($run.id) is not what the runner wrote: $content"
}

Write-Host "PASS compose smoke: run=$($run.id) succeeded with logs and artifact $($artifacts[0].id), principal=$($me.name)"
