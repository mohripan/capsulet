# Installation

Capsulet can be installed locally with Helm when API and worker images are available and PostgreSQL plus object storage are supplied. The current chart is suitable for local evaluation and public-alpha hardening work; bundled PostgreSQL and bundled MinIO chart dependencies are still future work.

## Intended Install Flow

The long-term install experience should be:

```sh
helm repo add capsulet https://mohripan16.github.io/capsulet-charts
helm repo update

helm install capsulet capsulet/capsulet \
  --namespace capsulet \
  --create-namespace
```

Then access the dashboard:

```sh
kubectl port-forward svc/capsulet-dashboard 3000:80 -n capsulet
```

Open:

```text
http://localhost:3000
```

## Current Chart Checks

Validate the chart before installing it:

```sh
helm lint charts/capsulet
helm template capsulet charts/capsulet
```

The chart renders deployments for:

- `capsulet-api`
- `capsulet-worker`
- `capsulet-scheduler`
- `capsulet-evaluator`
- `capsulet-dashboard`

The API and worker are the current functional runtime path. Scheduler, evaluator, and dashboard are scaffolded and can be disabled for local runtime tests.

## Local Runtime Install

For minikube instructions that build local API/worker images, create the database and object storage secrets, install the chart, submit jobs, fetch logs, and download artifacts, see:

```text
docs/local-kubernetes-runner.md
```

## External Dependencies

The chart expects these dependencies to be supplied:

- external PostgreSQL
- external S3-compatible object storage
- external Kafka, only when future scheduler/evaluator paths need it

Local evaluation can use Docker Compose PostgreSQL and MinIO. Bundled chart dependencies remain deferred.

## Minimal Helm Values

Create a database Secret:

```powershell
kubectl create secret generic capsulet-db `
  --namespace capsulet `
  --from-literal=DATABASE_URL=postgres://capsulet:capsulet@host.minikube.internal:5432/capsulet
```

For MinIO or S3-compatible storage, create a credentials Secret:

```powershell
kubectl create secret generic capsulet-object-storage `
  --namespace capsulet `
  --from-literal=access-key-id=capsulet `
  --from-literal=secret-access-key=capsuletpassword
```

Install only the functional runtime components:

```powershell
helm upgrade --install capsulet charts/capsulet `
  --namespace capsulet `
  --create-namespace `
  --set config.databaseUrlSecret.name=capsulet-db `
  --set config.objectStorage.mode=s3 `
  --set config.objectStorage.bucket=capsulet-artifacts `
  --set config.objectStorage.endpoint=http://host.minikube.internal:9000 `
  --set config.objectStorage.region=us-east-1 `
  --set config.objectStorage.pathStyle=true `
  --set config.objectStorage.credentialsSecret.name=capsulet-object-storage `
  --set scheduler.enabled=false `
  --set evaluator.enabled=false `
  --set dashboard.enabled=false
```

To include the live dashboard, build or provide a dashboard image and keep `dashboard.enabled=true`. The chart sets the dashboard API URL to the in-cluster API service by default. Override it when needed:

```powershell
helm upgrade --install capsulet charts/capsulet `
  --namespace capsulet `
  --set dashboard.enabled=true `
  --set dashboard.apiBaseUrl=http://capsulet-api
```

Forward the dashboard:

```powershell
kubectl port-forward svc/capsulet-dashboard 3000:80 -n capsulet
```

Open:

```text
http://127.0.0.1:3000/runs
```
