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

Each Capsulet component appends its repository suffix, such as `api`, `worker`, `scheduler`, `evaluator`, or `dashboard`.

For local minikube images:

```yaml
image:
  registry: ""
  repository: capsulet
  tag: dev
  pullPolicy: Never
```

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

Dashboard API configuration:

```yaml
dashboard:
  apiBaseUrl: ""
```

When `dashboard.apiBaseUrl` is empty, the chart renders an in-cluster default of `http://<release-name>-api`.

## Worker Runner

The worker exposes Kubernetes runner settings:

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

## PostgreSQL

Bundled PostgreSQL is enabled by default for local public-alpha evaluation:

```yaml
postgresql:
  mode: bundled
  auth:
    database: capsulet
    username: capsulet
    password: capsulet
  persistence:
    enabled: true
    size: 1Gi
```

Bundled mode renders:

- Secret: `<release>-postgresql`
- Service: `<release>-postgresql`
- StatefulSet: `<release>-postgresql`

The Secret includes `DATABASE_URL`, and API, worker, and migration Job read `CAPSULET_DATABASE_URL` from it.

For external PostgreSQL:

```yaml
postgresql:
  mode: external
config:
  databaseUrlSecret:
    name: capsulet-db
    key: DATABASE_URL
```

The external Secret must already exist.

## Migrations

The chart renders a migration Job by default:

```yaml
migrations:
  enabled: true
  backoffLimit: 6
```

The Job runs the API image with `CAPSULET_MIGRATE_ONLY=true`, applies embedded SQLx migrations, optionally seeds examples, and exits.

Inspect logs:

```sh
kubectl logs job/capsulet-migrate -n capsulet
```

If a local migration Job must be rerun:

```sh
kubectl delete job capsulet-migrate -n capsulet
helm upgrade --install capsulet charts/capsulet -n capsulet
```

## MinIO And Object Storage

Bundled MinIO is enabled by default for local public-alpha evaluation:

```yaml
minio:
  mode: bundled
  auth:
    rootUser: capsulet
    rootPassword: capsuletpassword
  bucket: capsulet-artifacts
  region: us-east-1
  pathStyle: true
  persistence:
    enabled: true
    size: 2Gi
  bucketJob:
    enabled: true
    backoffLimit: 6
```

Bundled mode renders:

- Secret: `<release>-minio`
- Service: `<release>-minio`
- StatefulSet: `<release>-minio`
- bucket initialization Job: `<release>-minio-bucket`

Bundled mode makes API and worker use:

```yaml
CAPSULET_OBJECT_STORAGE_MODE: s3
CAPSULET_OBJECT_STORAGE_ENDPOINT: http://<release>-minio:9000
CAPSULET_OBJECT_STORAGE_BUCKET: <minio.bucket>
```

For external S3-compatible object storage:

```yaml
minio:
  mode: external
config:
  objectStorage:
    mode: s3
    bucket: capsulet-artifacts
    endpoint: http://minio.example:9000
    region: us-east-1
    pathStyle: true
    credentialsSecret:
      name: capsulet-object-storage
      accessKeyKey: access-key-id
      secretKeyKey: secret-access-key
```

The external bucket and credential Secret must already exist.

Filesystem object storage is still available for narrow local tests:

```yaml
minio:
  mode: external
config:
  objectStorage:
    mode: filesystem
    path: /var/lib/capsulet/objects
```

Filesystem mode mounts an `emptyDir` volume for the API and worker. It is not a realistic multi-pod install mode.

## Security

Capsulet API, worker, scheduler, evaluator, and dashboard defaults include:

- non-root pod security context
- `RuntimeDefault` seccomp
- disabled privilege escalation
- read-only root filesystem
- dropped Linux capabilities

Bundled PostgreSQL and MinIO also run as non-root with dropped capabilities, but they keep `readOnlyRootFilesystem: false` because stateful database/object-storage images need writable runtime paths. Bundled dependencies are for local alpha evaluation; production-shaped installs should use external PostgreSQL and external S3-compatible storage.

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

`maxConcurrentJobs` is represented in values for future scheduler/concurrency work. It is not enforced yet.

## Network Policies And ServiceMonitor

These values are present but not implemented by templates yet:

```yaml
networkPolicies:
  enabled: false

serviceMonitor:
  enabled: false
```

Metrics and network policy presets remain future work.

## Validation

Run:

```sh
helm lint charts/capsulet
helm template capsulet charts/capsulet
helm template capsulet charts/capsulet \
  --set postgresql.mode=external \
  --set config.databaseUrlSecret.name=capsulet-db \
  --set minio.mode=external \
  --set config.objectStorage.mode=s3 \
  --set config.objectStorage.endpoint=http://minio.example:9000 \
  --set config.objectStorage.credentialsSecret.name=capsulet-object-storage
```
