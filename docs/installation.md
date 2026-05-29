# Installation

Capsulet is not installable as a working product yet. This document records the intended install flow and the current Sprint 001 chart verification commands.

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

## External Dependencies

Future production-shaped installs should support:

- external PostgreSQL
- external S3-compatible object storage
- external Kafka

Local evaluation may later support bundled dependencies through chart values or a development profile.
