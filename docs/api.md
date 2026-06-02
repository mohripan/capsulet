# API

Capsulet's Sprint 002 API exposes the first manual job-run path.

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
```

Start the API:

```sh
cargo run -p capsulet-api
```

The API runs migrations on startup. With `CAPSULET_SEED_EXAMPLES=true`, it also upserts `job_hello_python`.

## Seed a Job Definition

Manual run submission validates that the referenced job definition exists. The easiest local option is `CAPSULET_SEED_EXAMPLES=true`. You can also seed one directly:

```powershell
docker exec -i capsulet-postgres psql -U capsulet -d capsulet -c "INSERT INTO job_definitions (id, name, runtime_image, command, bundle_object_key, input_schema) VALUES ('job_hello_python', 'Hello Python', 'python:3.12-slim', ARRAY['python', '/workspace/main.py'], 'bundles/job_hello_python.tar.gz', '{}'::jsonb) ON CONFLICT (id) DO NOTHING;"
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

List runs:

```sh
curl http://127.0.0.1:8080/v1/jobs/runs
```

Fetch one run:

```sh
curl http://127.0.0.1:8080/v1/jobs/runs/run_123
```

## CLI

The `capsulet` CLI uses the same API. The base URL defaults to `http://127.0.0.1:8080` and can be changed with `CAPSULET_API_URL` or `--api-url`.

Submit a manual run:

```sh
cargo run -p capsulet-cli -- submit job_hello_python --pool mini
```

List runs:

```sh
cargo run -p capsulet-cli -- runs --limit 50
```

Fetch one run:

```sh
cargo run -p capsulet-cli -- run get run_123
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
- `store_error`
