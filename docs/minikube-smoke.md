<!-- capsulet-claims: CAP-JOB-001, CAP-JOB-002, CAP-JOB-003, CAP-JOB-004, CAP-JOB-005, CAP-JOB-006, CAP-JOB-007, CAP-JOB-008, CAP-JOB-009, CAP-HELM-001 -->
# Minikube Smoke Guide

This guide verifies the Kubernetes runner path with local images.

## Prerequisites

- Docker
- minikube
- kubectl
- Helm

## Steps

```powershell
minikube start
minikube docker-env --shell powershell | Invoke-Expression
docker build -f crates/Dockerfile --target api -t capsulet-api:dev .
docker build -f crates/Dockerfile --target worker -t capsulet-worker:dev .
docker build -f crates/Dockerfile --target scheduler -t capsulet-scheduler:dev .
docker build -f crates/Dockerfile --target graph-worker -t capsulet-graph-worker:dev .
docker build -f crates/Dockerfile --target evaluator -t capsulet-evaluator:dev .
docker build -f dashboard/Dockerfile -t capsulet-dashboard:dev dashboard

kubectl create namespace capsulet
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

helm upgrade --install capsulet ./charts/capsulet `
  --namespace capsulet `
  --set image.registry= `
  --set image.namespace= `
  --set image.tag=dev `
  --set image.pullPolicy=Never `
  --set api.auth.existingSecret=capsulet-api-auth `
  --set api.auth.secretKey=tokens

kubectl -n capsulet rollout status deploy/capsulet-api
kubectl -n capsulet rollout status deploy/capsulet-worker
kubectl -n capsulet port-forward svc/capsulet-api 8080:80
```

In the same session, which holds the token, while the port-forward runs in another:

```powershell
$env:CAPSULET_API_URL = "http://127.0.0.1:8080"
.\scripts\compose-smoke.ps1
```

The smoke script acts as that administrator. It reads a local stack's `fixture.json` first, so
stop a running Compose stack with `cargo run -p capsulet-xtask -- stack down` before pointing it
at the cluster.

Expected result: the smoke script prints `PASS compose smoke` and the corresponding Kubernetes Job reaches `Complete`.

## Debug checks

```powershell
kubectl -n capsulet get pods,jobs
kubectl -n capsulet logs deploy/capsulet-worker
kubectl -n capsulet logs deploy/capsulet-scheduler
kubectl -n capsulet logs deploy/capsulet-evaluator
curl http://127.0.0.1:8080/metrics
```
