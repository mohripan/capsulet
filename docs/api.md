# API

Capsulet's API exposes the manual job-run path, script-backed submission, cancellation, status inspection, log inspection, and artifact retrieval.

## Run Locally

Start PostgreSQL:

```sh
docker compose up -d postgres
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

Health:

```sh
curl http://127.0.0.1:8080/healthz
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
- `job_run_not_found`
- `job_run_logs_not_found`
- `job_artifact_not_found`
- `job_artifact_object_not_found`
- `object_store_error`
- `store_error`
