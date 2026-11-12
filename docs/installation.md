<!-- capsulet-claims: CAP-PRODUCT-001, CAP-JOB-001, CAP-JOB-002, CAP-JOB-003, CAP-JOB-004, CAP-JOB-005, CAP-JOB-006, CAP-JOB-007, CAP-JOB-008, CAP-JOB-009, CAP-IAM-001, CAP-IAM-002, CAP-IAM-003, CAP-IAM-004, CAP-IAM-005, CAP-IAM-006, CAP-IAM-007, CAP-IAM-009, CAP-PERSISTENCE-001, CAP-DASHBOARD-001, CAP-OBSERVABILITY-001, CAP-SECURITY-001, CAP-HELM-001 -->
# Installation

Capsulet can be evaluated locally with the experimental Helm chart. This is an alpha-readiness
installation path, not a public-alpha release. The default chart renders:

- API
- worker
- graph worker
- scheduler
- evaluator
- dashboard
- bundled PostgreSQL
- bundled SeaweedFS, the S3 object store, which creates its bucket on start
- database migration Job

Bundled PostgreSQL and SeaweedFS are local evaluation defaults. For production-shaped installs, use external PostgreSQL and external S3-compatible object storage.

## Local Kubernetes Install

Build local images into your cluster image store. For minikube:

```powershell
minikube start
minikube docker-env --shell powershell | Invoke-Expression

docker build -f crates/Dockerfile --target api -t capsulet-api:dev .
docker build -f crates/Dockerfile --target worker -t capsulet-worker:dev .
docker build -f crates/Dockerfile --target scheduler -t capsulet-scheduler:dev .
docker build -f crates/Dockerfile --target graph-worker -t capsulet-graph-worker:dev .
docker build -f crates/Dockerfile --target evaluator -t capsulet-evaluator:dev .
docker build -f dashboard/Dockerfile -t capsulet-dashboard:dev dashboard
```

Install Capsulet:

```powershell
kubectl create namespace capsulet
kubectl label node minikube capsulet.dev/pool=mini --overwrite
# A fresh administrator token, kept in this session for the steps below.
$bytes = New-Object byte[] 32
[Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
$env:CAPSULET_API_TOKEN = -join ($bytes | ForEach-Object { $_.ToString("x2") })
$tokens = New-TemporaryFile
ConvertTo-Json -Compress -InputObject @(@{ name = "local-admin"; role = "admin"; token = $env:CAPSULET_API_TOKEN }) |
  Set-Content -NoNewline $tokens
kubectl create secret generic capsulet-api-auth `
  --namespace capsulet --dry-run=client -o yaml `
  --from-file=tokens=$tokens `
  | kubectl apply -f -
Remove-Item $tokens

helm upgrade --install capsulet charts/capsulet `
  --namespace capsulet `
  --set image.registry= `
  --set image.namespace= `
  --set image.tag=dev `
  --set image.pullPolicy=Never `
  --set api.auth.existingSecret=capsulet-api-auth
```

Wait for the install:

```powershell
kubectl wait --for=condition=ready pod -l app.kubernetes.io/component=postgresql -n capsulet --timeout=180s
kubectl wait --for=condition=complete job/capsulet-migrate -n capsulet --timeout=180s
kubectl wait --for=condition=ready pod -l app.kubernetes.io/component=seaweedfs -n capsulet --timeout=180s
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
  --set seaweedfs.mode=external \
  --set config.objectStorage.mode=s3 \
  --set config.objectStorage.endpoint=https://s3.example.com \
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
  --from-literal=access-key-id=<access key id> `
  --from-literal=secret-access-key=<secret access key>
```

Install with external PostgreSQL and external S3-compatible storage:

```powershell
helm upgrade --install capsulet charts/capsulet `
  --namespace capsulet `
  --create-namespace `
  --set postgresql.mode=external `
  --set config.databaseUrlSecret.name=capsulet-db `
  --set seaweedfs.mode=external `
  --set config.objectStorage.mode=s3 `
  --set config.objectStorage.bucket=capsulet-artifacts `
  --set config.objectStorage.endpoint=https://s3.example.com `
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
