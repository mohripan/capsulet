# Installation

Capsulet can be installed locally with Helm as a self-contained public-alpha stack. The default chart renders:

- API
- worker
- scheduler
- evaluator
- dashboard
- bundled PostgreSQL
- bundled MinIO
- database migration Job
- MinIO bucket initialization Job

Bundled PostgreSQL and MinIO are local evaluation defaults. For production-shaped installs, use external PostgreSQL and external S3-compatible object storage.

## Local Kubernetes Install

Build local images into your cluster image store. For minikube:

```powershell
minikube start
minikube docker-env --shell powershell | Invoke-Expression

docker build -f crates/Dockerfile --build-arg PACKAGE=capsulet-api --build-arg BIN=capsulet-api -t capsulet-api:dev .
docker build -f crates/Dockerfile --build-arg PACKAGE=capsulet-worker --build-arg BIN=capsulet-worker -t capsulet-worker:dev .
docker build -f crates/Dockerfile --build-arg PACKAGE=capsulet-scheduler --build-arg BIN=capsulet-scheduler -t capsulet-scheduler:dev .
docker build -f crates/Dockerfile --build-arg PACKAGE=capsulet-evaluator --build-arg BIN=capsulet-evaluator -t capsulet-evaluator:dev .
docker build -f dashboard/Dockerfile -t capsulet-dashboard:dev dashboard
```

Install Capsulet:

```powershell
kubectl create namespace capsulet
kubectl label node minikube capsulet.dev/pool=mini --overwrite
kubectl create secret generic capsulet-api-auth `
  --namespace capsulet `
  --from-literal='tokens=[{"name":"cluster-admin","role":"admin","token":"replace-with-at-least-32-random-characters"}]'

helm upgrade --install capsulet charts/capsulet `
  --namespace capsulet `
  --set image.registry= `
  --set image.repository=capsulet `
  --set image.tag=dev `
  --set image.pullPolicy=Never `
  --set api.auth.existingSecret=capsulet-api-auth
```

Wait for the install:

```powershell
kubectl wait --for=condition=ready pod -l app.kubernetes.io/component=postgresql -n capsulet --timeout=180s
kubectl wait --for=condition=complete job/capsulet-migrate -n capsulet --timeout=180s
kubectl wait --for=condition=ready pod -l app.kubernetes.io/component=minio -n capsulet --timeout=180s
kubectl wait --for=condition=complete job/capsulet-minio-bucket -n capsulet --timeout=180s
kubectl rollout status deployment/capsulet-api -n capsulet
kubectl rollout status deployment/capsulet-worker -n capsulet
kubectl rollout status deployment/capsulet-dashboard -n capsulet
```

Access the dashboard:

```powershell
kubectl port-forward svc/capsulet-dashboard 3000:80 -n capsulet
```

Open:

```text
http://127.0.0.1:3000/login
```

Sign in with the exact token stored in `capsulet-api-auth`. Authentication fails closed when neither a token Secret nor the explicit development-only `api.auth.disabled=true` setting is present.

Access the API:

```powershell
kubectl port-forward svc/capsulet-api 8080:80 -n capsulet
```

Open:

```text
http://127.0.0.1:8080/healthz
```

## Chart Checks

Validate the default bundled chart:

```sh
helm lint charts/capsulet
helm template capsulet charts/capsulet
```

Validate external dependency mode:

```sh
helm template capsulet charts/capsulet \
  --set postgresql.mode=external \
  --set config.databaseUrlSecret.name=capsulet-db \
  --set minio.mode=external \
  --set config.objectStorage.mode=s3 \
  --set config.objectStorage.endpoint=http://minio.example:9000 \
  --set config.objectStorage.credentialsSecret.name=capsulet-object-storage
```

## External Dependencies

Create a database Secret:

```powershell
kubectl create secret generic capsulet-db `
  --namespace capsulet `
  --from-literal=DATABASE_URL=postgres://capsulet:password@postgres.example:5432/capsulet
```

Create an object storage credentials Secret:

```powershell
kubectl create secret generic capsulet-object-storage `
  --namespace capsulet `
  --from-literal=access-key-id=capsulet `
  --from-literal=secret-access-key=capsuletpassword
```

Install with external PostgreSQL and external S3-compatible storage:

```powershell
helm upgrade --install capsulet charts/capsulet `
  --namespace capsulet `
  --create-namespace `
  --set postgresql.mode=external `
  --set config.databaseUrlSecret.name=capsulet-db `
  --set minio.mode=external `
  --set config.objectStorage.mode=s3 `
  --set config.objectStorage.bucket=capsulet-artifacts `
  --set config.objectStorage.endpoint=http://minio.example:9000 `
  --set config.objectStorage.region=us-east-1 `
  --set config.objectStorage.pathStyle=true `
  --set config.objectStorage.credentialsSecret.name=capsulet-object-storage
```

The external object bucket must exist before Capsulet writes bundles, logs, or artifacts.

## Smoke Test

After port-forwarding the API:

```powershell
$env:CAPSULET_API_URL = "http://127.0.0.1:8080"
Set-Content -Path .\main.py -Value "print('hello from capsulet')"
cargo run -p capsulet-cli -- submit-script .\main.py --pool mini
cargo run -p capsulet-cli -- runs --limit 5
cargo run -p capsulet-cli -- logs <run-id>
cargo run -p capsulet-cli -- artifacts list <run-id>
```

For the full local Kubernetes smoke, including artifact download and control paths, see:

```text
docs/local-kubernetes-runner.md
```

## Cleanup

```powershell
helm uninstall capsulet -n capsulet
kubectl delete namespace capsulet
```
