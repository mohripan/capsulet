# Local Kubernetes Runner

Capsulet can execute and control built-in Python job definitions as real Kubernetes Jobs in minikube.

This flow uses:

- Docker Compose PostgreSQL and MinIO on the host
- minikube as the local Kubernetes cluster
- Helm to install the API and worker
- S3-compatible object storage for script bundles, large logs, and artifacts

## Start Dependencies

Start PostgreSQL and MinIO:

```powershell
docker compose up -d postgres minio
docker run --rm --network capsulet_default --entrypoint /bin/sh minio/mc:latest -c "mc alias set local http://minio:9000 capsulet capsuletpassword && mc mb -p local/capsulet-artifacts"
```

Start minikube:

```powershell
minikube start
kubectl create namespace capsulet
kubectl label node minikube capsulet.dev/pool=mini --overwrite
```

Build API, worker, and dashboard images into minikube's Docker daemon:

```powershell
minikube docker-env --shell powershell | Invoke-Expression
docker build -f Dockerfile.rust --build-arg PACKAGE=capsulet-api --build-arg BIN=capsulet-api -t capsulet-api:dev .
docker build -f Dockerfile.rust --build-arg PACKAGE=capsulet-worker --build-arg BIN=capsulet-worker -t capsulet-worker:dev .
docker build -f Dockerfile.dashboard -t capsulet-dashboard:dev .
```

Create the database secret. `host.minikube.internal` lets pods reach the host machine from minikube:

```powershell
kubectl create secret generic capsulet-db `
  --namespace capsulet `
  --from-literal=DATABASE_URL=postgres://capsulet:capsulet@host.minikube.internal:5432/capsulet
```

Create the object storage credentials secret:

```powershell
kubectl create secret generic capsulet-object-storage `
  --namespace capsulet `
  --from-literal=access-key-id=capsulet `
  --from-literal=secret-access-key=capsuletpassword
```

If your Docker/minikube setup cannot resolve or reach `host.minikube.internal`, run a temporary PostgreSQL deployment in the cluster for the smoke test instead:

```powershell
minikube image load postgres:16-alpine
kubectl create deployment capsulet-postgres -n capsulet --image=postgres:16-alpine
kubectl set env deployment/capsulet-postgres -n capsulet `
  POSTGRES_DB=capsulet `
  POSTGRES_USER=capsulet `
  POSTGRES_PASSWORD=capsulet
kubectl expose deployment capsulet-postgres -n capsulet --port=5432 --target-port=5432
kubectl rollout status deployment/capsulet-postgres -n capsulet
kubectl create secret generic capsulet-db `
  --namespace capsulet `
  --from-literal=DATABASE_URL=postgres://capsulet:capsulet@capsulet-postgres.capsulet.svc:5432/capsulet
```

## Install Capsulet

Install only the API and worker for the current runtime path:

```powershell
helm upgrade --install capsulet charts/capsulet `
  --namespace capsulet `
  --set image.registry= `
  --set image.repository=capsulet `
  --set image.tag=dev `
  --set image.pullPolicy=Never `
  --set config.databaseUrlSecret.name=capsulet-db `
  --set config.objectStorage.mode=s3 `
  --set config.objectStorage.bucket=capsulet-artifacts `
  --set config.objectStorage.endpoint=http://host.minikube.internal:9000 `
  --set config.objectStorage.region=us-east-1 `
  --set config.objectStorage.pathStyle=true `
  --set config.objectStorage.credentialsSecret.name=capsulet-object-storage `
  --set dashboard.enabled=false `
  --set scheduler.enabled=false `
  --set evaluator.enabled=false
```

Wait for the API and worker:

```powershell
kubectl rollout status deployment/capsulet-api -n capsulet
kubectl rollout status deployment/capsulet-worker -n capsulet
```

Forward the API locally:

```powershell
kubectl port-forward svc/capsulet-api 8080:80 -n capsulet
```

## Optional Dashboard

Install or upgrade with the dashboard enabled after building the dashboard image:

```powershell
helm upgrade --install capsulet charts/capsulet `
  --namespace capsulet `
  --set image.registry= `
  --set image.repository=capsulet `
  --set image.tag=dev `
  --set image.pullPolicy=Never `
  --set config.databaseUrlSecret.name=capsulet-db `
  --set config.objectStorage.mode=s3 `
  --set config.objectStorage.bucket=capsulet-artifacts `
  --set config.objectStorage.endpoint=http://host.minikube.internal:9000 `
  --set config.objectStorage.region=us-east-1 `
  --set config.objectStorage.pathStyle=true `
  --set config.objectStorage.credentialsSecret.name=capsulet-object-storage `
  --set dashboard.enabled=true `
  --set dashboard.apiBaseUrl=http://capsulet-api `
  --set scheduler.enabled=false `
  --set evaluator.enabled=false
```

Forward the dashboard:

```powershell
kubectl port-forward svc/capsulet-dashboard 3000:80 -n capsulet
```

Open:

```text
http://127.0.0.1:3000/runs
```

The `/runs` page is live. It can submit seeded jobs and scripts, open run detail, cancel active runs, show logs, list artifacts, and download artifacts.

## Run Hello Python

Submit a run:

```powershell
$env:CAPSULET_API_URL = "http://127.0.0.1:8080"
cargo run -p capsulet-cli -- submit job_hello_python --pool mini
```

List and inspect runs:

```powershell
cargo run -p capsulet-cli -- runs
cargo run -p capsulet-cli -- status <run-id>
cargo run -p capsulet-cli -- run get <run-id>
```

Fetch captured logs:

```powershell
cargo run -p capsulet-cli -- logs <run-id>
```

Expected log output:

```text
hello from capsulet
```

## Run A Script Bundle

Create a local Python file and submit it as a run-specific bundle:

```powershell
Set-Content -Path .\main.py -Value "print('hello from a script bundle')"
cargo run -p capsulet-cli -- submit-script .\main.py --pool mini
cargo run -p capsulet-cli -- runs --limit 5
cargo run -p capsulet-cli -- logs <run-id>
cargo run -p capsulet-cli -- artifacts list <run-id>
```

The artifact list includes the stored `main.py` bundle metadata.

## Run An Artifact Job

Submit the seeded artifact example:

```powershell
cargo run -p capsulet-cli -- submit job_artifact_python --pool mini
cargo run -p capsulet-cli -- runs --limit 5
cargo run -p capsulet-cli -- artifacts list <run-id>
cargo run -p capsulet-cli -- artifacts download <run-id> artifact_<run-id>_report.txt --output report.txt
Get-Content .\report.txt
```

Expected artifact content:

```text
artifact from capsulet
```

## Large Logs

When captured stdout exceeds 64 KiB, the worker keeps the existing logs endpoint usable and also writes the full stdout to object storage as `stdout.log`. `GET /v1/jobs/runs/{id}/logs` returns `object_log_available: true`, and `capsulet artifacts list <run-id>` shows the `stdout.log` artifact.

## Control Smokes

Cancel a long-running run:

```powershell
cargo run -p capsulet-cli -- submit job_sleep_python --pool mini
cargo run -p capsulet-cli -- runs --limit 5
cargo run -p capsulet-cli -- cancel <run-id>
cargo run -p capsulet-cli -- status <run-id>
kubectl get jobs,pods -n capsulet
```

Expected status: `cancelled`. The run-derived Kubernetes Job should be deleted or stopping.

Exercise retry on failure:

```powershell
cargo run -p capsulet-cli -- submit job_fail_python --pool mini
cargo run -p capsulet-cli -- runs --limit 5
```

Expected behavior: the first failed attempt moves through `retry_scheduled`; after the fixed delay, the worker queues and runs the retry. After configured attempts are exhausted, the run ends as `failed`.

Exercise timeout by temporarily lowering the mini pool timeout in Helm values or with `--set executionPools.pools.mini.timeoutSeconds=5`, then run:

```powershell
cargo run -p capsulet-cli -- submit job_timeout_python --pool mini
cargo run -p capsulet-cli -- status <run-id>
```

Expected status after the Kubernetes deadline: `timed_out`.

Completed runner Jobs include the configured `ttlSecondsAfterFinished` value. Inspect Jobs shortly after completion if you need to debug the pod before Kubernetes TTL cleanup removes it.

## Diagnostics

Inspect worker logs:

```powershell
kubectl logs deployment/capsulet-worker -n capsulet
```

Inspect script Jobs and pods:

```powershell
kubectl get jobs,pods -n capsulet
kubectl describe job -n capsulet -l capsulet.dev/managed-by=capsulet
```

Clean up:

```powershell
helm uninstall capsulet -n capsulet
kubectl delete namespace capsulet
docker compose stop postgres minio
```
