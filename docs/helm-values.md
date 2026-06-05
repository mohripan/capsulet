# Helm Values

This document summarizes the current Helm values. The authoritative defaults live in `charts/capsulet/values.yaml`.

## Image

```yaml
image:
  registry: ghcr.io
  repository: mohripan16/capsulet
  tag: "0.1.0"
  pullPolicy: IfNotPresent
```

Each component appends its repository suffix, such as `api`, `worker`, `scheduler`, `evaluator`, or `dashboard`.

## Components

Top-level component sections:

- `api`
- `worker`
- `scheduler`
- `evaluator`
- `dashboard`

Each component supports:

- `enabled`
- `replicaCount`
- `image.repositorySuffix`
- `resources`

API and dashboard also expose service settings.

The worker also exposes Kubernetes runner settings:

```yaml
worker:
  runner:
    mode: kubernetes
    executionNamespace: ""
    logLimitBytes: 65536
    loop: true
    pollSeconds: 5
```

An empty `executionNamespace` means the Helm release namespace.

Scheduler, evaluator, and dashboard are scaffolded components. Disable them for the current API/worker runtime smoke:

```yaml
scheduler:
  enabled: false
evaluator:
  enabled: false
dashboard:
  enabled: false
```

Dashboard API configuration:

```yaml
dashboard:
  apiBaseUrl: http://capsulet-api
```

When `dashboard.apiBaseUrl` is empty, the chart renders an in-cluster default of `http://<release-name>-api`. Set this explicitly when the dashboard must call a port-forwarded API or an API service with a custom name.

## Database

The API and worker read `CAPSULET_DATABASE_URL` from a Secret:

```yaml
config:
  databaseUrlSecret:
    name: capsulet-db
    key: DATABASE_URL
```

The chart does not create PostgreSQL yet. Use an external database or the local Docker Compose database from the development guide.

## Object Storage

Filesystem mode is the default:

```yaml
config:
  objectStorage:
    mode: filesystem
    path: /var/lib/capsulet/objects
```

Filesystem mode mounts an `emptyDir` volume for the API and worker. It is useful only for local single-pod evaluation. Use S3-compatible storage for realistic installs:

```yaml
config:
  objectStorage:
    mode: s3
    bucket: capsulet-artifacts
    endpoint: http://minio:9000
    region: us-east-1
    pathStyle: true
    credentialsSecret:
      name: capsulet-object-storage
      accessKeyKey: access-key-id
      secretKeyKey: secret-access-key
```

The S3 bucket must already exist. Credentials are mounted only into API and worker pods.

## Security

Defaults include:

- non-root pod security context
- `RuntimeDefault` seccomp
- disabled privilege escalation
- read-only root filesystem
- dropped Linux capabilities

These are early defaults and will become more specific as runtime images are implemented.

## Execution Pools

Execution pools define job routing defaults:

```yaml
executionPools:
  defaultPool: mini
  pools:
    mini:
      nodeSelector:
        capsulet.dev/pool: mini
      timeoutSeconds: 120
      maxConcurrentJobs: 50
      ttlSecondsAfterFinished: 300
    large:
      nodeSelector:
        capsulet.dev/pool: large
      timeoutSeconds: 3600
      maxConcurrentJobs: 10
      ttlSecondsAfterFinished: 300
```

Capsulet chooses the execution pool. Kubernetes chooses the specific node.
`ttlSecondsAfterFinished` controls Kubernetes cleanup of completed runner Jobs.

`maxConcurrentJobs` is represented in values for the future scheduler/concurrency work. Sprint 005 does not enforce pool-level concurrency.

## Network Policies And ServiceMonitor

These values are present but not implemented by templates yet:

```yaml
networkPolicies:
  enabled: false

serviceMonitor:
  enabled: false
```

Sprint 006 planning keeps dashboard integration separate from metrics and network policy work.

## Validation

The chart includes `values.schema.json` for basic validation.

Run:

```sh
helm lint charts/capsulet
helm template capsulet charts/capsulet
```
