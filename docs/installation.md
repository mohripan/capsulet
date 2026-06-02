# Installation

Capsulet can be installed for the Sprint 003 local API and Kubernetes worker path when local images and PostgreSQL are supplied. The full product install with bundled dependencies is still future work.

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

## Current Local Chart Rendering

During Sprint 001, validate the chart without installing it:

```sh
helm lint charts/capsulet
helm template capsulet charts/capsulet
```

The chart currently renders placeholder deployments for:

- `capsulet-api`
- `capsulet-worker`
- `capsulet-scheduler`
- `capsulet-evaluator`
- `capsulet-dashboard`

The rendered workloads are not expected to be functional until service images, health endpoints, persistence, and runtime implementations exist.

## Sprint 003 Local Install

For minikube instructions that build local API/worker images, create the database secret, install the chart, submit `job_hello_python`, and fetch logs, see:

```text
docs/local-kubernetes-runner.md
```

## External Dependencies

Future production-shaped installs should support:

- external PostgreSQL
- external S3-compatible object storage
- external Kafka

Local evaluation may later support bundled dependencies through chart values or a development profile.
