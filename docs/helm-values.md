<!-- capsulet-claims: CAP-AGENT-008, CAP-PERSISTENCE-001, CAP-OBSERVABILITY-001, CAP-SECURITY-001, CAP-HELM-001 -->
# Helm Values

This document summarizes the current Helm values. The authoritative defaults live in `charts/capsulet/values.yaml`.

## Image

```yaml
image:
  registry: ghcr.io
  namespace: mohripan
  tag: "0.1.0"
  pullPolicy: IfNotPresent
```

Each component names its image, and may pin it by digest:

```yaml
api:
  image:
    name: capsulet-api
    digest: ""
```

A component runs `<registry>/<namespace>/<name>:<tag>`, with `@<digest>` appended when one is set,
for example `ghcr.io/mohripan/capsulet-api:0.1.0`. The registry, namespace, tag and names all come
from `release.toml`, and `capsulet-xtask release check` fails when the values disagree with it;
`release check-chart` renders the chart and checks every workload's image, by tag and by digest. See
the [release process](contracts/release-process.md).

For local minikube images, built as in the [installation guide](installation.md):

```yaml
image:
  registry: ""
  namespace: ""
  tag: dev
  pullPolicy: Never
```

The third-party images the chart bundles, `postgresql.image`, `seaweedfs.image` and `tests.image`,
each name a version and a `digest`, and are rendered as `<repository>:<tag>@<digest>`.

## Components

Top-level component sections:

- `api`
- `worker`
- `scheduler`
- `evaluator`
- `graphWorker`
- `dashboard`

Each component supports:

- `enabled`
- `replicaCount`
- `image.name` and `image.digest`
- `resources`

Worker pods receive a unique `CAPSULET_WORKER_ID` from their pod name. Lease heartbeats and
finalization use this identity to reject results from a worker after another replica adopts its
expired lease.

API and dashboard also expose service settings.

Dashboard API configuration:

```yaml
dashboard:
  apiBaseUrl: ""
```

When `dashboard.apiBaseUrl` is empty, the chart renders an in-cluster default of `http://<release-name>-api`.

To sign in to the dashboard with a username and password before an OIDC provider is configured,
name a Secret that holds that login and the API token its session uses:

```yaml
dashboard:
  temporaryAdmin:
    existingSecret: capsulet-dashboard-login
```

```powershell
kubectl create secret generic capsulet-dashboard-login -n capsulet `
  --from-literal=username=admin `
  --from-literal=password=<a generated password> `
  --from-literal=token=<an API token the API accepts>
```

The dashboard reads the keys `username`, `password` and `token`. With the value empty, the default,
it offers no password sign-in. The `kind` gate installs the chart this way, with a login and token it
generates for that cluster.

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

The graph worker executes agent runs. `graphWorker.ollamaUrl` points it at a local model; left empty,
model nodes fail and say so, and everything deterministic still runs. Reaching a model needs egress the
default network policy does not grant:

```yaml
graphWorker:
  pollSeconds: 2
  leaseSeconds: 60
  ollamaUrl: http://ollama:11434
```

The scheduler has its own polling controls:

```yaml
scheduler:
  loop: true
  pollSeconds: 5
```

## PostgreSQL

Bundled PostgreSQL is enabled by default for experimental local evaluation:

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

## Object Storage

The chart bundles SeaweedFS as its S3-compatible object store, for local evaluation and single-node
installs:

```yaml
seaweedfs:
  mode: bundled
  image:
    repository: chrislusf/seaweedfs
    tag: "4.53"
    digest: sha256:ccad23e18639c8b98cee17e7100b95830a06077b56d8e5b006d6dcf8ec5e94a0
  auth:
    accessKeyId: capsulet
    secretAccessKey: ""
  bucket: capsulet-artifacts
  region: us-east-1
  persistence:
    enabled: true
    size: 2Gi
```

It runs SeaweedFS's single-process `mini` mode, which creates the bucket when it starts, so no bucket
Job is needed. WebDAV, the admin UI, the Iceberg and Lance endpoints and SeaweedFS's usage telemetry
are switched off, and only the S3 port is exposed.

Credentials are never chart defaults:

- `seaweedfs.auth.secretAccessKey` left empty generates a key on first install and keeps it on every
  upgrade. `helm template` cannot read the cluster, so it renders a new key each time; set the key
  yourself when you render manifests rather than install them.
- The chart also generates, and keeps, a volume-server and a filer signing key. Without them anything
  that reaches the pod could write to SeaweedFS's volume server or filer directly, or create S3
  identities through the filer's IAM service, around the access key.

Bundled mode renders:

- Secret: `<release>-seaweedfs`, with `access-key-id`, `secret-access-key`, `volume-signing-key` and
  `filer-signing-key`
- Service: `<release>-seaweedfs`
- StatefulSet: `<release>-seaweedfs`

and makes the API, worker and evaluator use:

```yaml
CAPSULET_OBJECT_STORAGE_MODE: s3
CAPSULET_OBJECT_STORAGE_ENDPOINT: http://<release>-seaweedfs:8333
CAPSULET_OBJECT_STORAGE_BUCKET: <seaweedfs.bucket>
CAPSULET_OBJECT_STORAGE_PATH_STYLE: "true"
```

For external S3-compatible object storage:

```yaml
seaweedfs:
  mode: external
config:
  objectStorage:
    mode: s3
    bucket: capsulet-artifacts
    endpoint: https://s3.example.com
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
seaweedfs:
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

Bundled PostgreSQL and SeaweedFS also run as non-root with dropped capabilities, but they keep `readOnlyRootFilesystem: false` because stateful database/object-storage images need writable runtime paths. Bundled dependencies are for experimental local evaluation; production-shaped installs should use external PostgreSQL and external S3-compatible storage.

The evaluator leases durable trigger events and uses durable state to deduplicate evaluations. It executes timezone-aware cron, named read-only SQL, signed webhook, and isolated custom-plugin triggers, and runs retention cleanup. Configure SQL connections and webhook secrets outside chart values as secrets in production.

Named SQL connections are supplied as a JSON object in a Secret, for example `{"inventory":"postgres://readonly:..."}`:

```yaml
evaluator:
  sqlConnections:
    existingSecret: capsulet-sql-connections
    secretKey: connections
```

Retention cleanup removes job artifacts and trigger events after `evaluator.retentionDays` and audit
events after `evaluator.auditRetentionDays`. `evaluator.agentRunPayloadRetentionDays` is unset by
default; set it to erase what finished agent runs were given and produced that many days after they end,
keeping their logs. It is the same erasure an admin can ask for, done as `capsulet-retention`:

```yaml
evaluator:
  agentRunPayloadRetentionDays: 30
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
  --set seaweedfs.mode=external \
  --set config.objectStorage.mode=s3 \
  --set config.objectStorage.endpoint=https://s3.example.com \
  --set config.objectStorage.credentialsSecret.name=capsulet-object-storage
```
