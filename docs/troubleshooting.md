# Troubleshooting

This guide covers common failures in the current local and early-alpha Capsulet setup.

## API Cannot Start

Check the database URL:

```powershell
$env:CAPSULET_DATABASE_URL
```

For local Docker Compose PostgreSQL, use:

```text
postgres://capsulet:capsulet@localhost:5432/capsulet
```

If migrations fail, confirm PostgreSQL is running and reachable:

```powershell
docker compose ps postgres
docker compose logs postgres
docker compose logs api
```

For Helm installs with bundled PostgreSQL, inspect the database pod and migration Job:

```powershell
kubectl get pods,jobs -n capsulet
kubectl describe pod -l app.kubernetes.io/component=postgresql -n capsulet
kubectl logs job/capsulet-migrate -n capsulet
```

The bundled database Secret is `capsulet-postgresql` for a release named `capsulet`. The API and worker read `CAPSULET_DATABASE_URL` from its `DATABASE_URL` key.

If `postgresql.mode=external`, confirm the configured Secret exists:

```powershell
kubectl get secret capsulet-db -n capsulet
```

If a failed migration Job must be rerun:

```powershell
kubectl delete job capsulet-migrate -n capsulet
helm upgrade --install capsulet charts/capsulet -n capsulet
```

## Worker Finds No Runs

Confirm a run is queued:

```powershell
cargo run -p capsulet-cli -- runs --limit 10
```

Confirm the worker points at the same database as the API:

```powershell
$env:CAPSULET_DATABASE_URL
```

Confirm the requested execution pool exists. The local defaults are `mini` and `large`; Helm installs can override them with `executionPools`.

## Runs Are Stuck In `leased` Or `running`

The worker recovers expired non-terminal leases before leasing new work. Start another worker tick with the same database:

```powershell
cargo run -p capsulet-worker
```

If the run was already executing in Kubernetes, Sprint 005 does not reattach to the existing Kubernetes Job. Recovery may create replacement work after the lease expires.

## Kubernetes Jobs Do Not Start

Check worker mode and namespace:

```powershell
$env:CAPSULET_RUNNER_MODE
$env:CAPSULET_EXECUTION_NAMESPACE
```

Inspect Kubernetes resources:

```powershell
kubectl get jobs,pods -n capsulet
kubectl describe job -n capsulet -l capsulet.dev/managed-by=capsulet
kubectl logs deployment/capsulet-worker -n capsulet
```

If pods remain pending, check node labels and execution pool selectors:

```powershell
kubectl get nodes --show-labels
```

The default `mini` pool expects:

```text
capsulet.dev/pool=mini
```

## Object Storage Writes Fail

For filesystem mode, the API and worker must be able to read and write `CAPSULET_OBJECT_STORAGE_PATH`.

For S3-compatible mode, check:

- `CAPSULET_OBJECT_STORAGE_MODE=s3`
- `CAPSULET_OBJECT_STORAGE_BUCKET`
- `CAPSULET_OBJECT_STORAGE_ENDPOINT`
- `CAPSULET_OBJECT_STORAGE_REGION`
- `CAPSULET_OBJECT_STORAGE_PATH_STYLE=true` for MinIO
- `CAPSULET_OBJECT_STORAGE_ACCESS_KEY_ID`
- `CAPSULET_OBJECT_STORAGE_SECRET_ACCESS_KEY`

The bucket must exist before Capsulet writes objects. For the Docker Compose stack, the `minio-init` service creates it automatically. Check the init logs:

```powershell
docker compose logs minio-init
```

To recreate it manually:

```powershell
docker run --rm --network capsulet_default --entrypoint /bin/sh minio/mc:latest -c "mc alias set local http://minio:9000 capsulet capsuletpassword && mc mb -p local/capsulet-artifacts"
```

For Helm installs with bundled MinIO, inspect MinIO and the bucket initialization Job:

```powershell
kubectl get pods,jobs -n capsulet
kubectl describe pod -l app.kubernetes.io/component=minio -n capsulet
kubectl logs job/capsulet-minio-bucket -n capsulet
```

The bundled MinIO Secret is `capsulet-minio` for a release named `capsulet`. API and worker read S3 credentials from its `root-user` and `root-password` keys.

Forward the bundled MinIO console:

```powershell
kubectl port-forward svc/capsulet-minio 9001:9001 -n capsulet
```

Open:

```text
http://127.0.0.1:9001
```

If `minio.mode=external`, confirm the configured credential Secret and bucket exist:

```powershell
kubectl get secret capsulet-object-storage -n capsulet
```

## Artifact List Works But Download Fails

Artifact listing reads PostgreSQL metadata. Artifact download also reads object storage. If list works but download fails, the metadata row exists but the object key is missing or inaccessible.

Check the bucket:

```powershell
docker run --rm --network capsulet_default --entrypoint /bin/sh minio/mc:latest -c "mc alias set local http://minio:9000 capsulet capsuletpassword >/dev/null && mc ls -r local/capsulet-artifacts"
```

## Logs Are Truncated

Small logs are returned inline. Logs larger than 64 KiB keep an inline preview and create a `stdout.log` artifact. Check:

```powershell
cargo run -p capsulet-cli -- artifacts list <run-id>
```

Download the full large log:

```powershell
cargo run -p capsulet-cli -- artifacts download <run-id> log_<run-id>_stdout --output stdout.log
```

## Helm Template Does Not Include S3 Credentials

With bundled MinIO, S3 credentials come from the rendered `<release>-minio` Secret automatically.

With external S3-compatible object storage, credentials are only mounted into API and worker pods when this value is set:

```yaml
config:
  objectStorage:
    credentialsSecret:
      name: capsulet-object-storage
```

The Secret must contain the configured key names. Defaults:

```text
access-key-id
secret-access-key
```

## Dashboard Cannot Reach The API

The dashboard browser code calls the same-origin Next.js proxy under `/api/capsulet/...`. The proxy forwards to `CAPSULET_DASHBOARD_API_URL`.

For local development:

```powershell
$env:CAPSULET_DASHBOARD_API_URL = "http://127.0.0.1:8080"
npm run dev
```

For Docker Compose, the dashboard container uses:

```text
CAPSULET_DASHBOARD_API_URL=http://api:8080
```

For Helm installs, set:

```yaml
dashboard:
  apiBaseUrl: http://capsulet-api
```

If the runs page shows an API error, check:

- the API is running and `/healthz` returns `ok`
- `CAPSULET_DASHBOARD_API_URL` is reachable from the dashboard process or pod
- the dashboard pod has the ConfigMap value `CAPSULET_DASHBOARD_API_URL`
- the API and dashboard are in the expected namespace when using in-cluster service names

The dashboard does not require browser CORS configuration because browser requests go to the same-origin proxy.
