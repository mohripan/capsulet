# Local Kubernetes Runner

Capsulet can execute and control built-in Python job definitions as real Kubernetes Jobs in minikube.

This flow uses:

- minikube as the local Kubernetes cluster
- Helm to install the API, worker, dashboard, bundled PostgreSQL, and bundled MinIO
- the Kubernetes runner for script execution
- bundled MinIO for script bundles, large logs, and artifacts

## Start minikube

Start minikube:

```powershell
minikube start
kubectl create namespace capsulet
kubectl label node minikube capsulet.dev/pool=mini --overwrite
```

Build API, worker, and dashboard images into minikube's Docker daemon:

```powershell
minikube docker-env --shell powershell | Invoke-Expression
docker build -f crates/Dockerfile --build-arg PACKAGE=capsulet-api --build-arg BIN=capsulet-api -t capsulet-api:dev .
docker build -f crates/Dockerfile --build-arg PACKAGE=capsulet-worker --build-arg BIN=capsulet-worker -t capsulet-worker:dev .
docker build -f crates/Dockerfile --build-arg PACKAGE=capsulet-scheduler --build-arg BIN=capsulet-scheduler -t capsulet-scheduler:dev .
docker build -f crates/Dockerfile --build-arg PACKAGE=capsulet-evaluator --build-arg BIN=capsulet-evaluator -t capsulet-evaluator:dev .
docker build -f dashboard/Dockerfile -t capsulet-dashboard:dev dashboard
```

## Install Capsulet

Install the full local alpha stack:

```powershell
helm upgrade --install capsulet charts/capsulet `
  --namespace capsulet `
  --set image.registry= `
  --set image.repository=capsulet `
  --set image.tag=dev `
  --set image.pullPolicy=Never
```

Wait for bundled dependencies, migration, bucket creation, and app deployments:

```powershell
kubectl wait --for=condition=ready pod -l app.kubernetes.io/component=postgresql -n capsulet --timeout=180s
kubectl wait --for=condition=complete job/capsulet-migrate -n capsulet --timeout=180s
kubectl wait --for=condition=ready pod -l app.kubernetes.io/component=minio -n capsulet --timeout=180s
kubectl wait --for=condition=complete job/capsulet-minio-bucket -n capsulet --timeout=180s
kubectl rollout status deployment/capsulet-api -n capsulet
kubectl rollout status deployment/capsulet-worker -n capsulet
kubectl rollout status deployment/capsulet-dashboard -n capsulet
kubectl rollout status deployment/capsulet-scheduler -n capsulet
kubectl rollout status deployment/capsulet-evaluator -n capsulet
```

Forward the API locally:

```powershell
kubectl port-forward svc/capsulet-api 8080:80 -n capsulet
```

## Dashboard

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

Inspect bundled dependency jobs:

```powershell
kubectl logs job/capsulet-migrate -n capsulet
kubectl logs job/capsulet-minio-bucket -n capsulet
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
```
