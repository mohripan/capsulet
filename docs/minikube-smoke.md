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
minikube image load capsulet-api:local
minikube image load capsulet-worker:local
minikube image load capsulet-scheduler:local
minikube image load capsulet-evaluator:local

kubectl create secret generic capsulet-api-auth \
  --namespace capsulet --dry-run=client -o yaml \
  --from-literal='tokens=[{"name":"local-admin","role":"admin","token":"capsulet-local-admin-token-change-me"}]' \
  | kubectl apply -f -

helm upgrade --install capsulet ./charts/capsulet `
  --namespace capsulet --create-namespace `
  --set image.tag=local `
  --set image.pullPolicy=IfNotPresent `
  --set api.auth.existingSecret=capsulet-api-auth `
  --set api.auth.secretKey=tokens

kubectl -n capsulet rollout status deploy/capsulet-api
kubectl -n capsulet rollout status deploy/capsulet-worker
kubectl -n capsulet port-forward svc/capsulet-api 8080:8080
```

In another shell:

```powershell
$env:CAPSULET_API_TOKEN = "capsulet-local-admin-token-change-me"
.\scripts\compose-smoke.ps1 -BaseUrl http://127.0.0.1:8080 -Token $env:CAPSULET_API_TOKEN
```

Expected result: the smoke script prints `PASS compose smoke` and the corresponding Kubernetes Job reaches `Complete`.

## Debug checks

```powershell
kubectl -n capsulet get pods,jobs
kubectl -n capsulet logs deploy/capsulet-worker
kubectl -n capsulet logs deploy/capsulet-scheduler
kubectl -n capsulet logs deploy/capsulet-evaluator
curl http://127.0.0.1:8080/metrics
```
