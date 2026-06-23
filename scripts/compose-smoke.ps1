param(
  [string]$BaseUrl = "http://127.0.0.1:8080",
  [string]$Token = "capsulet-local-admin-token-change-me",
  [int]$TimeoutSeconds = 180,
  [switch]$KeepExistingQueue
)

$ErrorActionPreference = "Stop"

function Invoke-CapsuletJson {
  param(
    [string]$Method,
    [string]$Path,
    [object]$Body = $null
  )
  $headers = @{ Authorization = "Bearer $Token" }
  if ($null -eq $Body) {
    return Invoke-RestMethod -Method $Method -Uri "$BaseUrl$Path" -Headers $headers
  }
  return Invoke-RestMethod -Method $Method -Uri "$BaseUrl$Path" -Headers $headers -ContentType "application/json" -Body ($Body | ConvertTo-Json -Depth 12)
}

Write-Host "Starting Capsulet compose stack..."
docker compose up -d --build

$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
do {
  try {
    Invoke-RestMethod -Uri "$BaseUrl/readyz" | Out-Null
    break
  } catch {
    if ((Get-Date) -gt $deadline) { throw "Capsulet API did not become ready within $TimeoutSeconds seconds." }
    Start-Sleep -Seconds 3
  }
} while ($true)

Write-Host "Checking identity and OpenAPI..."
$me = Invoke-CapsuletJson -Method GET -Path "/v1/auth/me"
if ($me.role -ne "admin") { throw "Expected admin role, got $($me.role)." }
Invoke-RestMethod -Uri "$BaseUrl/openapi.json" | Out-Null

if (-not $KeepExistingQueue) {
  Write-Host "Resetting local smoke queue..."
  docker compose stop worker scheduler evaluator | Out-Null
  docker exec capsulet-postgres psql -U capsulet -d capsulet -v ON_ERROR_STOP=1 -c @"
DELETE FROM workflow_step_runs;
DELETE FROM workflow_runs;
DELETE FROM job_runs WHERE status IN ('queued', 'leased', 'running', 'retry_scheduled');
DELETE FROM trigger_events;
DELETE FROM trigger_runtime_status;
DELETE FROM automation_triggers;
DELETE FROM automations;
"@ | Out-Null
  docker compose up -d worker scheduler evaluator | Out-Null
}

Write-Host "Submitting smoke run..."
$run = Invoke-CapsuletJson -Method POST -Path "/v1/jobs/runs" -Body @{
  job_definition_id = "job_hello_python"
  execution_pool = "mini"
}

do {
  $current = Invoke-CapsuletJson -Method GET -Path "/v1/jobs/runs/$($run.id)"
  if ($current.status -in @("succeeded", "failed", "cancelled", "timed_out")) { break }
  if ((Get-Date) -gt $deadline) { throw "Run $($run.id) did not finish within $TimeoutSeconds seconds." }
  Start-Sleep -Seconds 2
} while ($true)

if ($current.status -ne "succeeded") {
  throw "Run $($run.id) finished with status $($current.status)."
}

$logs = Invoke-CapsuletJson -Method GET -Path "/v1/jobs/runs/$($run.id)/logs"
if ([string]::IsNullOrWhiteSpace($logs.logs)) { throw "Run $($run.id) had empty logs." }

Write-Host "PASS compose smoke: run=$($run.id), principal=$($me.name), status=$($current.status)"
