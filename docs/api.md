# API

Capsulet's API exposes job definition authoring, manual job runs, workflow definition authoring, automation triggers, cancellation, status inspection, log inspection, and artifact retrieval.

## Run Locally

For the full local stack, including PostgreSQL, MinIO, API, worker, and dashboard, run:

```sh
docker compose up --build
```

For manual API development, start only the local data services:

```sh
docker compose up -d postgres minio minio-init
```

Set the API environment:

```powershell
$env:CAPSULET_DATABASE_URL = "postgres://capsulet:capsulet@localhost:5432/capsulet"
$env:CAPSULET_API_ADDR = "127.0.0.1:8080"
$env:CAPSULET_EXECUTION_POOLS = "mini,large"
$env:CAPSULET_SEED_EXAMPLES = "true"
$env:CAPSULET_OBJECT_STORAGE_MODE = "filesystem"
$env:CAPSULET_OBJECT_STORAGE_PATH = ".capsulet-objects"
```

Start the API:

```sh
cargo run -p capsulet-api
```

The API runs migrations on startup. With `CAPSULET_SEED_EXAMPLES=true`, it also upserts `job_hello_python`, `job_sleep_python`, `job_fail_python`, `job_timeout_python`, and `job_artifact_python`.

For S3-compatible storage such as MinIO, set:

```powershell
$env:CAPSULET_OBJECT_STORAGE_MODE = "s3"
$env:CAPSULET_OBJECT_STORAGE_BUCKET = "capsulet-artifacts"
$env:CAPSULET_OBJECT_STORAGE_ENDPOINT = "http://127.0.0.1:9000"
$env:CAPSULET_OBJECT_STORAGE_REGION = "us-east-1"
$env:CAPSULET_OBJECT_STORAGE_PATH_STYLE = "true"
$env:CAPSULET_OBJECT_STORAGE_ACCESS_KEY_ID = "capsulet"
$env:CAPSULET_OBJECT_STORAGE_SECRET_ACCESS_KEY = "capsuletpassword"
```

## Seed a Job Definition

Manual run submission validates that the referenced job definition exists. The easiest local option is `CAPSULET_SEED_EXAMPLES=true`. You can also seed one directly:

```powershell
docker exec -i capsulet-postgres psql -U capsulet -d capsulet -c "INSERT INTO job_definitions (id, name, runtime_image, command, bundle_object_key, input_schema) VALUES ('job_hello_python', 'Hello Python', 'python:3.12-slim', ARRAY['python', '-c', 'print(''hello from capsulet'')'], 'bundles/job_hello_python.tar.gz', '{}'::jsonb) ON CONFLICT (id) DO NOTHING;"
```

## Endpoints

The router currently exposes:

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/livez`, `/readyz`, `/healthz` | Process and database health |
| `GET`, `POST` | `/v1/job-definitions` | List or create definitions |
| `GET`, `PUT`, `DELETE` | `/v1/job-definitions/{id}` | Read, replace, or delete a definition |
| `GET` | `/v1/execution-pools`, `/v1/host-groups` | Read static execution configuration |
| `GET`, `POST` | `/v1/workflows` | List or create workflow DAGs |
| `GET`, `PUT` | `/v1/workflows/{id}` | Read or replace a workflow DAG |
| `GET`, `POST` | `/v1/automations` | List or create automations |
| `GET`, `PUT`, `DELETE` | `/v1/automations/{id}` | Read, replace, or delete an automation |
| `POST` | `/v1/automations/{id}/enable`, `/disable` | Change automation state |
| `GET` | `/v1/automations/{id}/triggers` | List an automation's trigger graph |
| `POST` | `/v1/automations/{id}/trigger` | Start its workflow manually |
| `GET`, `POST` | `/v1/trigger-plugins` | List or create plugin metadata |
| `GET` | `/v1/trigger-plugins/{id}` | Read plugin metadata |
| `GET` | `/v1/workflow-runs` | List workflow runs and step runs |
| `GET` | `/v1/workflow-runs/{id}` | Read one workflow run and its step runs |
| `GET` | `/v1/workflow-runs/{id}/logs` | Aggregate step-run logs |
| `POST` | `/v1/workflow-runs/{id}/remove` | Remove a queued run from normal listings |
| `POST` | `/v1/workflow-runs/{id}/cancel` | Cancel a running workflow and its jobs |
| `POST` | `/v1/workflow-runs/{id}/resume` | Resume from successful checkpoints |
| `GET`, `POST` | `/v1/jobs/runs` | List or create job runs |
| `GET` | `/v1/jobs/runs/{id}` | Read a job run |
| `POST` | `/v1/jobs/runs/{id}/cancel` | Cancel a job run |
| `GET` | `/v1/jobs/runs/{id}/logs` | Read inline/object-log status |
| `GET` | `/v1/jobs/runs/{id}/artifacts` | List artifacts |
| `GET` | `/v1/jobs/runs/{id}/artifacts/{artifact_id}` | Download an artifact |

Liveness, readiness, and the backward-compatible health alias:

```sh
curl http://127.0.0.1:8080/healthz
curl http://127.0.0.1:8080/livez
curl http://127.0.0.1:8080/readyz
```

List execution pools:

```sh
curl http://127.0.0.1:8080/v1/execution-pools
```

Create a reusable Python job definition:

```sh
curl -X POST http://127.0.0.1:8080/v1/job-definitions \
  -H "content-type: application/json" \
  -d '{"name":"Hourly email","runtime_image":"python:3.12-slim","python_script":"print(\"send email\")"}'
```

List job definitions:

```sh
curl http://127.0.0.1:8080/v1/job-definitions
```

Fetch, update, or delete one job definition:

```sh
curl http://127.0.0.1:8080/v1/job-definitions/job_123

curl -X PUT http://127.0.0.1:8080/v1/job-definitions/job_123 \
  -H "content-type: application/json" \
  -d '{"name":"Hourly email","runtime_image":"python:3.12-slim","python_script":"print(\"updated\")"}'

curl -X DELETE http://127.0.0.1:8080/v1/job-definitions/job_123
```

Create a manual run:

```sh
curl -X POST http://127.0.0.1:8080/v1/jobs/runs \
  -H "content-type: application/json" \
  -d '{"job_definition_id":"job_hello_python","execution_pool":"mini"}'
```

Create a single-file Python script run. The API stores the script as a bundle object and creates a run-specific job definition:

```sh
curl -X POST http://127.0.0.1:8080/v1/jobs/runs \
  -H "content-type: application/json" \
  -d '{"job_definition_id":"script","execution_pool":"mini","python_script":"print(\"hello from a bundle\")"}'
```

List runs:

```sh
curl http://127.0.0.1:8080/v1/jobs/runs
```

Fetch one run:

```sh
curl http://127.0.0.1:8080/v1/jobs/runs/run_123
```

Fetch captured logs for one run:

```sh
curl http://127.0.0.1:8080/v1/jobs/runs/run_123/logs
```

The log response includes `object_log_available`. When `true`, the full log was also written as an artifact named `stdout.log`.

List artifacts for one run:

```sh
curl http://127.0.0.1:8080/v1/jobs/runs/run_123/artifacts
```

Download one artifact:

```sh
curl http://127.0.0.1:8080/v1/jobs/runs/run_123/artifacts/artifact_123 --output artifact.bin
```

Cancel a queued or running run:

```sh
curl -X POST http://127.0.0.1:8080/v1/jobs/runs/run_123/cancel
```

Create a fan-out/fan-in workflow DAG:

```sh
curl -X POST http://127.0.0.1:8080/v1/workflows \
  -H "content-type: application/json" \
  -d '{
    "id":"workflow_daily_report",
    "name":"Daily report",
    "description":"Extract sources in parallel, then merge them",
    "steps":[
      {"id":"extract_customers","name":"Extract customers","job_definition_id":"job_customers","execution_pool":"mini"},
      {"id":"extract_orders","name":"Extract orders","job_definition_id":"job_orders","execution_pool":"mini"},
      {"id":"merge_reports","name":"Merge reports","job_definition_id":"job_merge","execution_pool":"mini"}
    ],
    "dependencies":[
      {"from_step_id":"extract_customers","to_step_id":"merge_reports"},
      {"from_step_id":"extract_orders","to_step_id":"merge_reports"}
    ]
  }'
```

Every dependency endpoint must reference a step in the same workflow. Duplicate edges, self-dependencies, unknown endpoints, and cycles return `400 validation_error`. Omitting `dependencies` preserves the legacy behavior by creating a position-ordered chain; sending an empty array creates independent root nodes. Workflow responses always include the persisted dependency list. Replace a definition with `PUT /v1/workflows/{id}` using the same request shape.

List and fetch workflows:

```sh
curl http://127.0.0.1:8080/v1/workflows
curl http://127.0.0.1:8080/v1/workflows/workflow_123
```

Create a manual automation:

```sh
curl -X POST http://127.0.0.1:8080/v1/automations \
  -H "content-type: application/json" \
  -d '{"name":"Manual email","workflow_id":"workflow_123","trigger_kind":"manual"}'
```

Create an interval automation. For hourly execution, use `3600` seconds:

```sh
curl -X POST http://127.0.0.1:8080/v1/automations \
  -H "content-type: application/json" \
  -d '{"name":"Hourly email","workflow_id":"workflow_123","trigger_kind":"interval","interval_seconds":3600}'
```

The compatibility fields above drive the currently implemented manual and fixed-interval runtime. The richer authoring model accepts a `triggers` array with named `manual`, `schedule`, `sql`, or `custom` definitions and a structured `condition`. The API validates and persists those definitions, but schedule/SQL/custom execution through the evaluator is not wired yet.

List and fetch automations:

```sh
curl http://127.0.0.1:8080/v1/automations
curl http://127.0.0.1:8080/v1/automations/automation_123
```

Replace, enable, disable, delete, or inspect triggers:

```sh
curl -X PUT http://127.0.0.1:8080/v1/automations/automation_123 \
  -H "content-type: application/json" \
  -d '{"name":"Updated automation","workflow_id":"workflow_123","trigger_kind":"manual"}'
curl -X POST http://127.0.0.1:8080/v1/automations/automation_123/enable
curl -X POST http://127.0.0.1:8080/v1/automations/automation_123/disable
curl http://127.0.0.1:8080/v1/automations/automation_123/triggers
curl -X DELETE http://127.0.0.1:8080/v1/automations/automation_123
```

Custom-trigger plugins are metadata and validation contracts in the current release; Capsulet does not execute their images yet:

```sh
curl -X POST http://127.0.0.1:8080/v1/trigger-plugins \
  -H "content-type: application/json" \
  -d '{"id":"plugin_example","name":"Example","runtime_image":"example/plugin:1","command":["/plugin"]}'
curl http://127.0.0.1:8080/v1/trigger-plugins
curl http://127.0.0.1:8080/v1/trigger-plugins/plugin_example
```

Trigger one automation manually:

```sh
curl -X POST http://127.0.0.1:8080/v1/automations/automation_123/trigger
```

List workflow runs:

```sh
curl http://127.0.0.1:8080/v1/workflow-runs
curl http://127.0.0.1:8080/v1/workflow-runs/workflow_run_123
```

Each workflow run includes `step_runs`. A step run exposes its `position`, `status`, `workflow_step_id`, and underlying `job_run_id`; use that job run ID with the existing logs and artifacts endpoints.

Inspect aggregate logs, cancel, or remove an eligible queued workflow run:

```sh
curl http://127.0.0.1:8080/v1/workflow-runs/workflow_run_123/logs
curl -X POST http://127.0.0.1:8080/v1/workflow-runs/workflow_run_123/cancel
curl -X POST http://127.0.0.1:8080/v1/workflow-runs/workflow_run_123/remove
```

Resume a failed or timed-out workflow from successful step checkpoints:

```sh
curl -X POST http://127.0.0.1:8080/v1/workflow-runs/workflow_run_123/resume
```

Successful step runs and their artifacts are preserved. Unsuccessful attempts are removed, and the scheduler creates only graph nodes whose prerequisites are satisfied and which do not already have a successful checkpoint.

Visible run states are:

- `queued`
- `leased`
- `running`
- `succeeded`
- `failed`
- `cancelled`
- `timed_out`
- `retry_scheduled`

## CLI

The `capsulet` CLI uses the same API. The base URL defaults to `http://127.0.0.1:8080` and can be changed with `CAPSULET_API_URL` or `--api-url`.

Submit a manual run:

```sh
cargo run -p capsulet-cli -- submit job_hello_python --pool mini
```

Submit a single-file Python script:

```sh
cargo run -p capsulet-cli -- submit-script ./main.py --pool mini
```

List runs:

```sh
cargo run -p capsulet-cli -- runs --limit 50
```

Fetch one run:

```sh
cargo run -p capsulet-cli -- run get run_123
```

Show run status:

```sh
cargo run -p capsulet-cli -- status run_123
```

Print captured logs:

```sh
cargo run -p capsulet-cli -- logs run_123
```

List and download artifacts:

```sh
cargo run -p capsulet-cli -- artifacts list run_123
cargo run -p capsulet-cli -- artifacts download run_123 artifact_123 --output artifact.bin
```

Cancel a run:

```sh
cargo run -p capsulet-cli -- cancel run_123
```

## Docker Compose Stub Runner

The local Docker Compose worker uses the stub runner. It does not execute the authored Python inside a container, but it does exercise the complete control plane path: API authoring, scheduler workflow advancement, worker leasing, logs, and artifacts.

Compose sets deterministic stub logs and `stub-artifact.txt`, so workflow-created job runs can be inspected through:

```sh
curl http://127.0.0.1:8080/v1/jobs/runs/run_123/logs
curl http://127.0.0.1:8080/v1/jobs/runs/run_123/artifacts
```

## Error Shape

Errors return JSON:

```json
{
  "code": "unknown_job_definition",
  "message": "unknown job definition: job_missing"
}
```

Known API error codes:

- `validation_error`
- `unknown_job_definition`
- `unknown_execution_pool`
- `workflow_not_found`
- `workflow_run_not_found`
- `invalid_workflow_run_transition`
- `automation_not_found`
- `trigger_plugin_not_found`
- `job_run_not_found`
- `job_run_logs_not_found`
- `job_artifact_not_found`
- `job_artifact_object_not_found`
- `object_store_error`
- `store_error`
