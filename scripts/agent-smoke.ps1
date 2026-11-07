param(
  [string]$BaseUrl = "http://127.0.0.1:8080",
  [string]$Token = "capsulet-local-admin-token-change-me",
  [int]$TimeoutSeconds = 240,
  [switch]$SkipStart
)

# Runs agents through the stack compose.yaml brings up: over HTTP, against the deployed
# graph worker, with that worker killed while a run is in flight.
#
# Nothing here needs a model. Every node is deterministic or a real job, so this checks
# the platform around the model rather than whatever a model happened to answer.

$ErrorActionPreference = "Stop"
$suffix = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()

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

function Wait-RunStatus {
  param([string]$Id, [string[]]$Statuses)
  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  do {
    $run = Invoke-CapsuletJson -Method GET -Path "/v1/agent-runs/$Id"
    if ($run.status -in $Statuses) { return $run }
    if ((Get-Date) -gt $deadline) {
      throw "Run $Id is still $($run.status) after $TimeoutSeconds seconds, wanted $($Statuses -join ' or ')."
    }
    Start-Sleep -Seconds 1
  } while ($true)
}

function New-Agent {
  param([string]$Name, [hashtable]$Graph)
  $Graph.id = "graph_${Name}_$suffix"
  $Graph.name = $Name
  $Graph.transition_policy = @{ mode = "static"; cycles_allowed = $false }
  Invoke-CapsuletJson -Method POST -Path "/v1/graphs" -Body $Graph | Out-Null
  $agent = Invoke-CapsuletJson -Method POST -Path "/v1/agents" -Body @{
    id = "agent_${Name}_$suffix"
    name = $Name
    graph_id = $Graph.id
    budget = @{ max_steps = 3; max_tokens = 1000; max_seconds = 300; max_cost_micros = 1000 }
    termination_conditions = @("validator_pass")
  }
  return $agent.id
}

function Start-AgentRun {
  param([string]$Agent, [string]$Name, [hashtable]$State)
  $run = Invoke-CapsuletJson -Method POST -Path "/v1/agents/$Agent/runs" -Body @{
    id = "run_${Name}_$suffix"
    initial_state = $State
  }
  if ($run.status -ne "queued") { throw "A new run should be queued, not $($run.status)." }
  return $run.id
}

function Get-EventKinds {
  param([string]$Id)
  $log = Invoke-CapsuletJson -Method GET -Path "/v1/agent-runs/$Id/events"
  return @($log.events | ForEach-Object { $_.kind })
}

if (-not $SkipStart) {
  Write-Host "Starting the stack..."
  docker compose up -d --build api worker graph-worker
  if ($LASTEXITCODE -ne 0) { throw "docker compose up failed." }
}

$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
do {
  try {
    Invoke-RestMethod -Uri "$BaseUrl/readyz" | Out-Null
    break
  } catch {
    if ((Get-Date) -gt $deadline) { throw "The API did not become ready within $TimeoutSeconds seconds." }
    Start-Sleep -Seconds 3
  }
} while ($true)

$checked = New-Agent -Name "checked" -Graph @{
  nodes = @(
    @{
      id = "normalize"; name = "Normalize"; kind = "query_normalizer"
      ports = @(
        @{ id = "in"; direction = "input"; value_type = "user_query" },
        @{ id = "out"; direction = "output"; value_type = "normalized_query" }
      )
    },
    @{
      id = "prompt"; name = "Prompt"; kind = "prompt_builder"
      ports = @(
        @{ id = "query"; direction = "input"; value_type = "normalized_query" },
        @{ id = "text"; direction = "output"; value_type = "prompt" }
      )
      config = @{ template = "Answer this: {{query}}" }
    },
    @{
      id = "check"; name = "Check"; kind = "validator"
      ports = @(
        @{ id = "text"; direction = "input"; value_type = "prompt" },
        @{ id = "result"; direction = "output"; value_type = "validation_result" }
      )
      config = @{ "rule.contains" = "Answer this" }
    }
  )
  hyperedges = @(
    @{
      id = "query-in"
      sources = @(@{ kind = "state_field"; field = "query"; value_type = "user_query" })
      targets = @(@{ kind = "port"; node_id = "normalize"; port_id = "in"; value_type = "user_query" })
    },
    @{
      id = "normal"
      sources = @(@{ kind = "port"; node_id = "normalize"; port_id = "out"; value_type = "normalized_query" })
      targets = @(@{ kind = "port"; node_id = "prompt"; port_id = "query"; value_type = "normalized_query" })
    },
    @{
      id = "checked"
      sources = @(@{ kind = "port"; node_id = "prompt"; port_id = "text"; value_type = "prompt" })
      targets = @(@{ kind = "port"; node_id = "check"; port_id = "text"; value_type = "prompt" })
    },
    @{
      id = "written"
      sources = @(@{ kind = "port"; node_id = "prompt"; port_id = "text"; value_type = "prompt" })
      targets = @(@{ kind = "state_field"; field = "prompt"; value_type = "prompt" })
    }
  )
}

Write-Host "A run is executed by the deployed worker..."
$id = Start-AgentRun -Agent $checked -Name "first" -State @{ query = "  what   is capsulet? " }
$run = Wait-RunStatus -Id $id -Statuses @("succeeded", "failed", "stopped", "cancelled")
if ($run.status -ne "succeeded") { throw "Run $id ended $($run.status): $($run.execution.detail)" }
if ($run.state.prompt -ne "Answer this: what is capsulet?") { throw "Unexpected state: $($run.state.prompt)" }
if ($run.assurance -ne "unverified") { throw "Finishing must not read as verified, got $($run.assurance)." }

Write-Host "A queued run waits for a worker, and is cancelled before it spends anything..."
docker compose stop graph-worker | Out-Null
$cancelled = Start-AgentRun -Agent $checked -Name "cancelled" -State @{ query = "stop me" }
Invoke-CapsuletJson -Method POST -Path "/v1/agent-runs/$cancelled/cancel" | Out-Null
$still = Invoke-CapsuletJson -Method GET -Path "/v1/agent-runs/$cancelled"
if ($still.status -ne "queued") { throw "With no worker running, $cancelled should still be queued, not $($still.status)." }
docker compose start graph-worker | Out-Null
$run = Wait-RunStatus -Id $cancelled -Statuses @("cancelled", "succeeded", "failed", "stopped")
if ($run.status -ne "cancelled") { throw "Run $cancelled ended $($run.status) instead of cancelled." }
if ((Get-EventKinds -Id $cancelled) -contains "node_started") { throw "A cancelled run started a node." }

Write-Host "A worker killed while a run is in flight loses nothing and repeats nothing..."
docker compose stop worker | Out-Null
$jobbing = New-Agent -Name "jobbing" -Graph @{
  nodes = @(
    @{
      id = "job"; name = "Job"; kind = "job"
      ports = @(@{ id = "result"; direction = "output"; value_type = "json" })
      config = @{ job_definition_id = "job_hello_python"; execution_pool = "mini" }
    }
  )
  hyperedges = @(
    @{
      id = "job-out"
      sources = @(@{ kind = "port"; node_id = "job"; port_id = "result"; value_type = "json" })
      targets = @(@{ kind = "state_field"; field = "result"; value_type = "json" })
    }
  )
}
$killed = Start-AgentRun -Agent $jobbing -Name "killed" -State @{}

# The job cannot finish with its worker stopped, so the graph worker is holding this run
# open when it dies.
$deadline = (Get-Date).AddSeconds(60)
do {
  $kinds = Get-EventKinds -Id $killed
  if ($kinds -contains "effect_claimed") { break }
  if ((Get-Date) -gt $deadline) { throw "The job effect was never claimed: $($kinds -join ', ')" }
  Start-Sleep -Seconds 1
} while ($true)

docker kill --signal=KILL capsulet-graph-worker | Out-Null
docker compose start graph-worker worker | Out-Null

$run = Wait-RunStatus -Id $killed -Statuses @("succeeded", "failed", "stopped", "cancelled")
if ($run.status -ne "succeeded") { throw "Run $killed ended $($run.status): $($run.execution.detail)" }
$kinds = Get-EventKinds -Id $killed
$finalized = @($kinds | Where-Object { $_ -eq "effect_finalized" }).Count
if ($finalized -ne 1) { throw "Expected one finalized effect, found $finalized in: $($kinds -join ', ')" }

$jobRun = $run.state.result.job_run_id
$count = docker exec capsulet-postgres psql -U capsulet -d capsulet -tA -c "SELECT count(*) FROM job_runs WHERE id = '$jobRun'"
if ($count.Trim() -ne "1") { throw "Expected exactly one job run $jobRun, found $count." }

Write-Host "PASS agent smoke: $id succeeded, $cancelled cancelled, $killed survived a killed worker with one job run ($jobRun)."
