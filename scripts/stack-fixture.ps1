# Dot-source this from a smoke script run at the repository root.
#
# It reads the stack fixture that `cargo run -p capsulet-xtask -- stack up`, or a
# verifier gate, wrote for a running stack; points `docker compose` at that
# stack; and defines Invoke-CapsuletJson, which acts as one of the fixture's
# identities. No credential here is a literal: each one was generated for the
# stack and is deleted with it.

$fixturePath = if ($env:CAPSULET_FIXTURE_FILE) {
  $env:CAPSULET_FIXTURE_FILE
} else {
  Join-Path $PSScriptRoot "../.capsulet/stack/capsulet-local/fixture.json"
}
if (Test-Path $fixturePath) {
  $Fixture = Get-Content -Raw $fixturePath | ConvertFrom-Json
  $stackDirectory = Split-Path -Parent (Resolve-Path $fixturePath)
  if (-not $env:COMPOSE_PROJECT_NAME) { $env:COMPOSE_PROJECT_NAME = Split-Path -Leaf $stackDirectory }
  if (-not $env:COMPOSE_ENV_FILES) { $env:COMPOSE_ENV_FILES = Join-Path $stackDirectory "stack.env" }
} elseif ($env:CAPSULET_API_URL -and $env:CAPSULET_API_TOKEN) {
  # A deployment that is not a Compose stack, such as a local Kubernetes cluster:
  # the caller names the API and an administrator token for it.
  $Fixture = [pscustomobject]@{
    api_url = $env:CAPSULET_API_URL.TrimEnd("/")
    identities = [pscustomobject]@{ admin = [pscustomobject]@{ token = $env:CAPSULET_API_TOKEN } }
  }
} else {
  throw "No stack fixture at $fixturePath. Start a stack with: cargo run -p capsulet-xtask -- stack up, or set CAPSULET_API_URL and CAPSULET_API_TOKEN."
}
$BaseUrl = $Fixture.api_url

function Invoke-CapsuletJson {
  param(
    [string]$Method,
    [string]$Path,
    [object]$Body = $null,
    # An identity alias from the fixture: admin, operator, viewer or service.
    [string]$As = "admin",
    [string]$Project = ""
  )
  $headers = @{ Authorization = "Bearer $($Fixture.identities.$As.token)" }
  if ($Project) { $headers["x-capsulet-project-id"] = $Project }
  if ($null -eq $Body) {
    return Invoke-RestMethod -Method $Method -Uri "$BaseUrl$Path" -Headers $headers
  }
  return Invoke-RestMethod -Method $Method -Uri "$BaseUrl$Path" -Headers $headers -ContentType "application/json" -Body ($Body | ConvertTo-Json -Depth 12)
}

# Whether a GET finds something, without treating a 404 as a failure.
function Test-CapsuletResource {
  param([string]$Path, [string]$As = "admin", [string]$Project = "")
  try {
    Invoke-CapsuletJson -Method GET -Path $Path -As $As -Project $Project | Out-Null
    return $true
  } catch {
    if ($_.Exception.Response -and [int]$_.Exception.Response.StatusCode -eq 404) { return $false }
    throw
  }
}

function Invoke-Compose {
  docker compose @args
  if ($LASTEXITCODE -ne 0) { throw "docker compose $($args -join ' ') failed." }
}
