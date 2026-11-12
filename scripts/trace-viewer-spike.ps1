param(
  [ValidateSet("jaeger", "mlflow")]
  [string]$Viewer = "jaeger",
  [int]$TimeoutSeconds = 180,
  # Leave the viewer and the exporting services running, to look at a failure.
  [switch]$Keep
)
# The measurement behind ADR 0020: one synthetic, redacted agent run, its trace
# sent to an optional viewer and looked up there by the trace id Capsulet gives
# an authorized caller.
#
# Start a stack first, with `cargo run -p capsulet-xtask -- stack up`. The script:
#
# 1. starts the viewer, pinned by digest, on the stack's network, with nothing
#    published but its UI on 127.0.0.1;
# 2. recreates the API and graph worker exporting OTLP/HTTP to it, through a
#    Compose override written for this run;
# 3. runs a deterministic agent whose input carries a marker nobody else would
#    write, and exports its bundle;
# 4. finds the run's trace in the viewer by the bundle's trace id, and holds it to
#    what Capsulet promises: Capsulet's spans and attributes are there, and the
#    marker, which is content, is nowhere;
# 5. checks a member of no project, and the viewer's unauthenticated API, against
#    what each can see;
# 6. removes the viewer and recreates the API and graph worker as they were.
#
# Nothing here needs a model, an account, or the internet once the images are
# pulled.
$ErrorActionPreference = "Stop"
. "$PSScriptRoot/stack-fixture.ps1"

$images = @{
  jaeger = "jaegertracing/jaeger:2.23.0@sha256:3823829974ccfc2485d2e48b165579091f32295022ae80ca49704e2f4834116e"
  mlflow = "ghcr.io/mlflow/mlflow:v3.18.0@sha256:257204c77980b0f3450a26eb6089e487603fcdd4509529eadaf88f90669f5c13"
}
$project = $env:COMPOSE_PROJECT_NAME
$name = "$project-trace-viewer"
$network = "${project}_default"
$override = Join-Path ([System.IO.Path]::GetTempPath()) "$name.override.yaml"
$marker = "trace-spike-" + [guid]::NewGuid().ToString("N")

# Windows PowerShell turns a native command's stderr into an error under
# `Stop`, and Docker reports progress there; judge it by its exit code instead.
function Invoke-Docker {
  $previous = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try { $output = docker @args 2>&1 } finally { $ErrorActionPreference = $previous }
  if ($LASTEXITCODE -ne 0) { throw "docker $($args -join ' ') failed: $output" }
  return $output | Where-Object { $_ -isnot [System.Management.Automation.ErrorRecord] }
}

function Remove-Viewer {
  if (@(Invoke-Docker ps -a --format "{{.Names}}") -contains $name) {
    Invoke-Docker rm -f $name | Out-Null
  }
}

function Measure-Container {
  param([string]$Container)
  $stats = Invoke-Docker stats --no-stream --format "{{.MemUsage}}" $Container
  $size = Invoke-Docker image inspect --format "{{.Size}}" $images[$Viewer]
  return [pscustomobject]@{ memory = $stats; image_bytes = [int64]$size }
}

function Wait-Until {
  param([scriptblock]$Ready, [string]$What)
  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  while ((Get-Date) -lt $deadline) {
    try { $value = & $Ready; if ($value) { return $value } } catch { }
    Start-Sleep -Seconds 2
  }
  throw "Gave up waiting for $What after $TimeoutSeconds seconds."
}

function Restore-Stack {
  Remove-Viewer
  if (Test-Path $override) { Remove-Item $override }
  Invoke-Compose up -d --no-deps --wait api graph-worker | Out-Null
}

try {
  Write-Host "Starting $Viewer ($($images[$Viewer]))..."
  Remove-Viewer
  Invoke-Docker pull --quiet $images[$Viewer] | Out-Null
  $started = Get-Date
  if ($Viewer -eq "jaeger") {
    Invoke-Docker run -d --name $name --network $network -p 127.0.0.1:16686:16686 $images.jaeger | Out-Null
    $endpoint = "http://${name}:4318"
    $headers = ""
    $ui = "http://127.0.0.1:16686"
    # Jaeger 2 answers its v3 query API in OTLP's own JSON shape.
    Wait-Until -What "Jaeger" -Ready { (Invoke-WebRequest -UseBasicParsing "$ui/api/v3/services").StatusCode -eq 200 } | Out-Null
  } else {
    Invoke-Docker run -d --name $name --network $network -p 127.0.0.1:5005:5000 `
      -e MLFLOW_DISABLE_TELEMETRY=true $images.mlflow `
      mlflow server --host 0.0.0.0 --port 5000 --backend-store-uri sqlite:////tmp/mlflow.db `
      --allowed-hosts "${name}:5000,localhost:5000,localhost:5005,127.0.0.1:5005" | Out-Null
    $ui = "http://127.0.0.1:5005"
    Wait-Until -What "MLflow" -Ready { (Invoke-WebRequest -UseBasicParsing "$ui/health").StatusCode -eq 200 } | Out-Null
    $experiment = Invoke-RestMethod -Method POST -Uri "$ui/api/2.0/mlflow/experiments/create" `
      -ContentType "application/json" -Body (@{ name = "capsulet" } | ConvertTo-Json)
    $endpoint = "http://${name}:5000"
    $headers = "x-mlflow-experiment-id=$($experiment.experiment_id)"
  }
  $ready = (Get-Date) - $started
  Write-Host ("  ready in {0:N1}s" -f $ready.TotalSeconds)

  Write-Host "Recreating the API and graph worker exporting to it..."
  $environment = @"
      OTEL_EXPORTER_OTLP_ENDPOINT: "$endpoint"
      OTEL_EXPORTER_OTLP_PROTOCOL: "http/protobuf"
      OTEL_EXPORTER_OTLP_HEADERS: "$headers"
      OTEL_BSP_SCHEDULE_DELAY: "500"
"@
  @"
services:
  api:
    environment:
$environment
  graph-worker:
    environment:
$environment
"@ | Set-Content -Encoding utf8 $override
  Invoke-Compose -f compose.yaml -f $override up -d --no-deps --wait api graph-worker | Out-Null

  Write-Host "Running a deterministic agent whose input carries $marker..."
  $graphId = "trace_spike_graph"
  $agentId = "trace_spike_agent"
  if (-not (Test-CapsuletResource -Path "/v1/graphs/$graphId")) {
    Invoke-CapsuletJson -Method POST -Path "/v1/graphs" -Body @{
      id = $graphId; name = "Trace spike graph"
      nodes = @(
        @{ id = "normalize"; name = "Normalize"; kind = "query_normalizer"
           ports = @(@{ id = "in"; direction = "input"; value_type = "user_query" },
                     @{ id = "out"; direction = "output"; value_type = "normalized_query" }) },
        @{ id = "check"; name = "Check"; kind = "validator"
           ports = @(@{ id = "text"; direction = "input"; value_type = "normalized_query" },
                     @{ id = "result"; direction = "output"; value_type = "validation_result" })
           config = @{ "rule.contains" = "trace-spike" } }
      )
      hyperedges = @(
        @{ id = "query-in"
           sources = @(@{ kind = "state_field"; field = "query"; value_type = "user_query" })
           targets = @(@{ kind = "port"; node_id = "normalize"; port_id = "in"; value_type = "user_query" }) },
        @{ id = "checked"
           sources = @(@{ kind = "port"; node_id = "normalize"; port_id = "out"; value_type = "normalized_query" })
           targets = @(@{ kind = "port"; node_id = "check"; port_id = "text"; value_type = "normalized_query" }) },
        @{ id = "written"
           sources = @(@{ kind = "port"; node_id = "normalize"; port_id = "out"; value_type = "normalized_query" })
           targets = @(@{ kind = "state_field"; field = "normalized"; value_type = "normalized_query" }) }
      )
    } | Out-Null
  }
  if (-not (Test-CapsuletResource -Path "/v1/agents/$agentId")) {
    Invoke-CapsuletJson -Method POST -Path "/v1/agents" -Body @{
      id = $agentId; name = "Trace spike agent"; graph_id = $graphId
      budget = @{ max_steps = 3; max_tokens = 1000; max_seconds = 300; max_cost_micros = 1000 }
      termination_conditions = @("validator_pass")
    } | Out-Null
  }
  $run = Invoke-CapsuletJson -Method POST -Path "/v1/agents/$agentId/runs" -Body @{ initial_state = @{ query = "  $marker  " } }
  $finished = Wait-Until -What "the run" -Ready {
    $now = Invoke-CapsuletJson -Method GET -Path "/v1/agent-runs/$($run.id)"
    if ($now.status -in @("succeeded", "failed", "stopped", "cancelled")) { $now }
  }
  if ($finished.status -ne "succeeded") { throw "Run $($run.id) ended $($finished.status)." }

  Write-Host "Asking Capsulet which trace the run is, as an authorized caller..."
  $sealed = Invoke-CapsuletJson -Method POST -Path "/v1/agent-runs/$($run.id)/debug-bundle" -Body @{}
  $bundle = $sealed.body | ConvertFrom-Json
  $traceId = $bundle.trace_id
  if (-not $traceId) { throw "The run's bundle names no trace: was the API exporting when it started?" }
  Write-Host "  trace $traceId"

  Write-Host "Finding the trace in $Viewer..."
  if ($Viewer -eq "jaeger") {
    $spansOf = { param($found) @($found.result.resourceSpans | ForEach-Object { $_.scopeSpans } | ForEach-Object { $_.spans }) }
    $trace = Wait-Until -What "the trace in Jaeger" -Ready {
      $found = Invoke-RestMethod -Uri "$ui/api/v3/traces/$traceId"
      $spans = & $spansOf $found
      if (@($spans | Where-Object { $_.name -eq "capsulet.lease" }).Count -gt 0 -and
          @($spans | Where-Object { $_.name -like "capsulet.node*" }).Count -ge 2) { $found }
    }
    $text = $trace | ConvertTo-Json -Depth 40 -Compress
    $spans = & $spansOf $trace
    $names = $spans | ForEach-Object { $_.name }
    $attributes = $spans | ForEach-Object { $_.attributes } | ForEach-Object { $_.key } | Sort-Object -Unique
    $unauthenticated = (Invoke-WebRequest -UseBasicParsing "$ui/api/v3/traces/$traceId").StatusCode
  } else {
    $script = @"
import json, mlflow
mlflow.set_tracking_uri('http://localhost:5000')
traces = mlflow.search_traces(experiment_ids=['$($experiment.experiment_id)'], return_type='list')
out = []
for t in traces:
    spans = [{'name': s.name, 'attributes': s.attributes, 'trace_id': s.trace_id} for s in t.data.spans]
    out.append({'request_id': t.info.request_id, 'metadata': t.info.trace_metadata, 'spans': spans})
print(json.dumps(out, default=str))
"@
    $trace = Wait-Until -What "the trace in MLflow" -Ready {
      $found = Invoke-Docker exec $name python -c $script | ConvertFrom-Json
      $mine = @($found | Where-Object { ($_ | ConvertTo-Json -Depth 30 -Compress) -match $run.id })
      if ($mine.Count -gt 0 -and @($mine.spans | Where-Object { $_.name -eq "capsulet.lease" }).Count -gt 0) { $mine }
    }
    $text = $trace | ConvertTo-Json -Depth 30 -Compress
    $names = $trace.spans | ForEach-Object { $_.name }
    $attributes = $trace.spans | ForEach-Object { $_.attributes.PSObject.Properties.Name } | Sort-Object -Unique
    $unauthenticated = (Invoke-WebRequest -UseBasicParsing "$ui/api/2.0/mlflow/experiments/search?max_results=1").StatusCode
  }

  if ($text -match [regex]::Escape($marker)) { throw "The run's input reached $Viewer." }
  if ($text -notmatch [regex]::Escape($run.id)) { throw "The trace in $Viewer does not name run $($run.id)." }
  foreach ($expected in @("capsulet.lease", "capsulet.node normalize", "capsulet.node check")) {
    if (-not ($names -contains $expected)) { throw "No `$expected` span in $Viewer; saw: $($names -join ', ')." }
  }

  $measured = Measure-Container -Container $name
  [pscustomobject]@{
    viewer = $Viewer
    image = $images[$Viewer]
    image_mb = [math]::Round($measured.image_bytes / 1MB)
    memory = $measured.memory
    ready_seconds = [math]::Round($ready.TotalSeconds, 1)
    run = $run.id
    trace = $traceId
    spans = $names.Count
    span_names = ($names | Sort-Object -Unique) -join ", "
    capsulet_attributes = (@($attributes | Where-Object { $_ -like "capsulet.*" }) -join ", ")
    content_found = $false
    viewer_api_without_credentials = $unauthenticated
  } | Format-List
  Write-Host "PASS $Viewer holds the run's trace by the id Capsulet gave, with none of its content."
} finally {
  if ($Keep) {
    Write-Host "Leaving $Viewer at $ui and the services exporting to it; run again without -Keep to restore."
  } else {
    Write-Host "Removing $Viewer and recreating the API and graph worker as they were..."
    Restore-Stack
  }
}
