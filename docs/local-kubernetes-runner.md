# Local Kubernetes Runner

Sprint 004 can execute and control built-in Python job definitions as real Kubernetes Jobs in minikube.

This flow uses:

- Docker Compose PostgreSQL on the host
- minikube as the local Kubernetes cluster
- Helm to install the API and worker
- PostgreSQL-backed bounded logs through the generic log storage boundary

Large logs and artifacts are still intended to move to object storage later. PostgreSQL log storage is capped by the worker and exists so the first Kubernetes runner slice is inspectable.

## Start Dependencies

Start PostgreSQL:

```powershell
docker compose up -d postgres
```

Start minikube:

```powershell
minikube start
kubectl create namespace capsulet
kubectl label node minikube capsulet.dev/pool=mini --overwrite
```

Build API and worker images into minikube's Docker daemon:

```powershell
minikube docker-env --shell powershell | Invoke-Expression
docker build -f Dockerfile.rust --build-arg PACKAGE=capsulet-api --build-arg BIN=capsulet-api -t capsulet-api:dev .
docker build -f Dockerfile.rust --build-arg PACKAGE=capsulet-worker --build-arg BIN=capsulet-worker -t capsulet-worker:dev .
```

Create the database secret. `host.minikube.internal` lets pods reach the host machine from minikube:

```powershell
kubectl create secret generic capsulet-db `
  --namespace capsulet `
  --from-literal=DATABASE_URL=postgres://capsulet:capsulet@host.minikube.internal:5432/capsulet
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

Install only the API and worker for the Sprint 003 runner path:

```powershell
helm upgrade --install capsulet charts/capsulet `
  --namespace capsulet `
  --set image.registry= `
  --set image.repository=capsulet `
  --set image.tag=dev `
  --set image.pullPolicy=Never `
  --set config.databaseUrlSecret.name=capsulet-db `
  --set scheduler.enabled=false `
  --set evaluator.enabled=false `
  --set dashboard.enabled=false
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
```
