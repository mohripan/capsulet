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
    leaseSeconds: 60
```

An empty `executionNamespace` means the Helm release namespace.

The worker heartbeats active runs and extends their lease. `leaseSeconds` must leave enough time for at least one heartbeat between lease acquisition and expiry.

The scheduler has its own polling controls:

```yaml
scheduler:
  loop: true
  pollSeconds: 5
```

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

The evaluator leases durable trigger events, evaluates nested conditions exactly once, executes timezone-aware cron, named read-only SQL, signed webhook, and isolated custom-plugin triggers, and runs retention cleanup. Configure SQL connections and webhook secrets outside chart values as secrets in production.

Named SQL connections are supplied as a JSON object in a Secret, for example `{"inventory":"postgres://readonly:..."}`:

```yaml
evaluator:
  sqlConnections:
    existingSecret: capsulet-sql-connections
    secretKey: connections
```

Custom-trigger Jobs use `worker.runner.executionNamespace` and the same `executionIsolation` service account and RuntimeClass as ordinary execution Jobs.

Native Kubernetes admission policies are opt-in because local clusters often use
tagged development images:

```yaml
admissionPolicies:
  enabled: true
  failurePolicy: Fail
  security:
    enabled: true
    validationActions:
      - Deny
  images:
    requireDigest: true
    enforceAllowed: true
    allowed: []
    validationActions:
      - Audit
```

When `images.enforceAllowed` is true, the chart builds the admission image
allowlist from `executionPools.*.policy.images.allowed` and
`admissionPolicies.images.allowed`. Patterns ending in `*` are treated as
prefixes, matching runner pool policy behavior. Switch image validation actions
from `Audit` to `Deny` after all approved runtime images are pinned and tested.

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

`maxConcurrentJobs` is enforced atomically by PostgreSQL leasing, including with multiple worker replicas.

## Network Policies And ServiceMonitor

Control-plane network policies and Prometheus Operator discovery are optional:

```yaml
networkPolicies:
  enabled: false

serviceMonitor:
  enabled: false

prometheusRules:
  enabled: false

grafanaDashboards:
  enabled: false
```

`executionNetworkPolicy.enabled` defaults to true and isolates execution Jobs with default-deny ingress/egress plus optional DNS and explicit egress rules. `executionIsolation` selects the permissionless execution service account and an optional sandbox RuntimeClass. Enabling `serviceMonitor` renders discovery for API, worker, scheduler, and evaluator metrics Services.

`prometheusRules.enabled` renders Prometheus Operator alerts for API error rate,
admission rejection, queue age, worker lease age, pool saturation, scheduler
lag, workflow critical-path latency, stuck workflows, retry exhaustion, and
trigger runtime failures. `grafanaDashboards.enabled` renders a Grafana sidecar
ConfigMap containing the packaged Capsulet overview dashboard.

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
