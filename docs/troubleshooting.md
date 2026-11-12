<!-- capsulet-claims: CAP-AGENT-009, CAP-JOB-001, CAP-JOB-002, CAP-JOB-003, CAP-JOB-004, CAP-JOB-005, CAP-JOB-006, CAP-JOB-007, CAP-JOB-008, CAP-JOB-009, CAP-WORKFLOW-001, CAP-AUTOMATION-001, CAP-AUTOMATION-002, CAP-AUTOMATION-003, CAP-AUTOMATION-004, CAP-AUTOMATION-005, CAP-AUTOMATION-006, CAP-AUTOMATION-007, CAP-AUTOMATION-008, CAP-IAM-001, CAP-IAM-002, CAP-IAM-003, CAP-IAM-004, CAP-IAM-005, CAP-IAM-006, CAP-IAM-007, CAP-IAM-009, CAP-PERSISTENCE-001, CAP-OBSERVABILITY-001, CAP-SECURITY-001, CAP-HELM-001 -->
# Troubleshooting

This guide covers common failures in the current experimental local Capsulet setup.

## API Cannot Start

Check the database URL:

```powershell
$env:CAPSULET_DATABASE_URL
```

For local Docker Compose PostgreSQL, use the URL the stack wrote for itself, which carries its
generated password:

```powershell
Get-Content .capsulet/stack/capsulet-local/database-url
```

If migrations fail, confirm PostgreSQL is running and reachable. Compose needs the stack's project
name and environment file, because `compose.yaml` holds no default credential:

```powershell
$env:COMPOSE_PROJECT_NAME = "capsulet-local"
$env:COMPOSE_ENV_FILES = ".capsulet/stack/capsulet-local/stack.env"
docker compose ps postgres
docker compose logs postgres
docker compose logs api
```

For Helm installs with bundled PostgreSQL, inspect the database pod and migration Job:

```powershell
kubectl get pods,jobs -n capsulet
kubectl describe pod -l app.kubernetes.io/component=postgresql -n capsulet
kubectl logs job/capsulet-migrate -n capsulet
```

The bundled database Secret is `capsulet-postgresql` for a release named `capsulet`. The API and worker read `CAPSULET_DATABASE_URL` from its `DATABASE_URL` key.

If `postgresql.mode=external`, confirm the configured Secret exists:

```powershell
kubectl get secret capsulet-db -n capsulet
```

If a failed migration Job must be rerun:

```powershell
kubectl delete job capsulet-migrate -n capsulet
helm upgrade --install capsulet charts/capsulet -n capsulet
```

## Worker Finds No Runs

Confirm a run is queued:

```powershell
cargo run -p capsulet-cli -- runs --limit 10
```

Confirm the worker points at the same database as the API:

```powershell
$env:CAPSULET_DATABASE_URL
```

Confirm the requested execution pool exists. The local defaults are `mini` and `large`; Helm installs can override them with `executionPools`.

## Runs Are Stuck In `leased` Or `running`

The worker recovers expired non-terminal leases before leasing new work. Start another worker tick with the same database:

```powershell
cargo run -p capsulet-worker
```

While execution is healthy, the worker updates `heartbeat_at` and extends the lease. Check worker logs and PostgreSQL readiness if heartbeats stop. For Kubernetes runners, another worker adopts an expired `running` lease and reattaches to the deterministic Job for the same attempt. Other runner modes requeue expired work for at-least-once execution. Finalization succeeds only for the current `lease_owner`; verify that manually launched replicas have distinct `CAPSULET_WORKER_ID` values. Helm assigns the pod name automatically.

## Kubernetes Jobs Do Not Start

Check worker mode and namespace:

```powershell
$env:CAPSULET_RUNNER_MODE
$env:CAPSULET_EXECUTION_NAMESPACE
```

Inspect Kubernetes resources:

```powershell
kubectl get jobs,pods -n capsulet
kubectl describe job -n capsulet -l capsulet.dev/managed-by=capsulet
kubectl logs deployment/capsulet-worker -n capsulet
```

If pods remain pending, check node labels and execution pool selectors:

```powershell
kubectl get nodes --show-labels
```

The default `mini` pool expects:

```text
capsulet.dev/pool=mini
```

## Object Storage Writes Fail

For filesystem mode, the API and worker must be able to read and write `CAPSULET_OBJECT_STORAGE_PATH`.

For S3-compatible mode, check:

- `CAPSULET_OBJECT_STORAGE_MODE=s3`
- `CAPSULET_OBJECT_STORAGE_BUCKET`
- `CAPSULET_OBJECT_STORAGE_ENDPOINT`
- `CAPSULET_OBJECT_STORAGE_REGION`
- `CAPSULET_OBJECT_STORAGE_PATH_STYLE=true` for SeaweedFS and most self-hosted stores
- `CAPSULET_OBJECT_STORAGE_ACCESS_KEY_ID`
- `CAPSULET_OBJECT_STORAGE_SECRET_ACCESS_KEY`

The bucket must exist before Capsulet writes objects. The bundled SeaweedFS creates it on start, in
Compose and in the chart, and its health check passes only once the bucket is there. In Compose,
check the store with `docker compose logs seaweedfs`, with the two variables above set.

For Helm installs with bundled SeaweedFS:

```powershell
kubectl get pods -n capsulet
kubectl describe pod -l app.kubernetes.io/component=seaweedfs -n capsulet
kubectl logs statefulset/capsulet-seaweedfs -n capsulet
```

The bundled Secret is `capsulet-seaweedfs` for a release named `capsulet`. The API, worker and
evaluator read S3 credentials from its `access-key-id` and `secret-access-key` keys. A `403` from the
store after a reinstall usually means the pods kept an old key: the chart keeps the key across
upgrades, but deleting the Secret makes it generate a new one, which SeaweedFS and the components only
read when they restart.

The bundled store has no console; any S3 client works against a port-forward:

```powershell
kubectl port-forward svc/capsulet-seaweedfs 8333:8333 -n capsulet
```

If `seaweedfs.mode=external`, confirm the configured credential Secret and bucket exist:

```powershell
kubectl get secret capsulet-object-storage -n capsulet
```

## Artifact List Works But Download Fails

Artifact listing reads PostgreSQL metadata. Artifact download also reads object storage. If list works but download fails, the metadata row exists but the object key is missing or inaccessible.

List the bucket of the local stack, signing with its generated key:

```powershell
$key = (Select-String "^CAPSULET_OBJECT_STORE_SECRET_KEY='(.*)'$" .capsulet/stack/capsulet-local/stack.env).Matches[0].Groups[1].Value
curl.exe --aws-sigv4 "aws:amz:us-east-1:s3" --user "capsulet:$key" "http://127.0.0.1:8333/capsulet-artifacts?list-type=2"
```

## Logs Are Truncated

Small logs are returned inline. Logs larger than 64 KiB keep an inline preview and create a `stdout.log` artifact. Check:

```powershell
cargo run -p capsulet-cli -- artifacts list <run-id>
```

Download the full large log:

```powershell
cargo run -p capsulet-cli -- artifacts download <run-id> log_<run-id>_stdout --output stdout.log
```

## Helm Template Does Not Include S3 Credentials

With bundled SeaweedFS, S3 credentials come from the rendered `<release>-seaweedfs` Secret automatically.

With external S3-compatible object storage, credentials are only mounted into API and worker pods when this value is set:

```yaml
config:
  objectStorage:
    credentialsSecret:
      name: capsulet-object-storage
```

The Secret must contain the configured key names. Defaults:

```text
access-key-id
secret-access-key
```

## Dashboard Cannot Reach The API

The dashboard browser code calls the same-origin Next.js proxy under `/api/capsulet/...`. The proxy forwards to `CAPSULET_DASHBOARD_API_URL`.

For local development:

```powershell
$env:CAPSULET_DASHBOARD_API_URL = "http://127.0.0.1:8080"
npm run dev
```

For Docker Compose, the dashboard container uses:

```text
CAPSULET_DASHBOARD_API_URL=http://api:8080
```

For Helm installs, set:

```yaml
dashboard:
  apiBaseUrl: http://capsulet-api
```

If the runs page shows an API error, check:

- the API is running and `/healthz` returns `ok`
- `CAPSULET_DASHBOARD_API_URL` is reachable from the dashboard process or pod
- the dashboard pod has the ConfigMap value `CAPSULET_DASHBOARD_API_URL`
- the API and dashboard are in the expected namespace when using in-cluster service names

The dashboard does not require browser CORS configuration because browser requests go to the same-origin proxy.

## An Agent Run Did Something Unexpected

Export the run as a bundle and read it on any machine, with no database, network or cluster:

```sh
capsulet agent-runs bundle "$RUN_ID" --project "$CAPSULET_PROJECT" --output run.json
cargo run -p capsulet-replay --locked -- run.json
```

The CLI reads `CAPSULET_API_URL` and `CAPSULET_API_TOKEN` (or `--api-url` and `--api-token`) and needs
`agents:read` in the project. It checks the bundle against its digest before writing it, so a file it
writes is the one the API sealed. Every export is recorded in the project's audit log.

`capsulet-replay` checks the bundle against its digest, folds the log again, and prints what the run
did: each node's attempts and failures, each effect and how it came out, what spent its budget, why it
ended, and what it was in the middle of if it was interrupted.

It also makes every decision in the log again. `decide` is a pure function of the definition, the
log so far and the time, so for each event a worker chose to write (start a node, claim an effect,
open or close an iteration, stop a loop, compensate, end the run) the replay folds the log before it
and decides again. Any event the runtime would not have written is named, with what it would have
decided instead. A log can fold and still fail this: one with a node cut out of it folds fine, and
the replay says the runtime would have started that node. The replay counts, but cannot judge, what
the outside world wrote (admission, inputs and answered signals) and what a node's or effect's own
work wrote (what it consumed and how it ended).

It exits `0` when the log supports what the installation reported and every decision replays, `1`
when either fails, and `2` when the bundle cannot be read or does not match its digest.

To look through a run rather than read a report, write it as one page and open it in a browser:

```sh
cargo run -p capsulet-replay --locked -- run.json --html run.html
```

The page needs no server, runs no script and loads nothing from anywhere. It walks the run, then
each node, then what the node was observed to do, each event marked as a decision made again, an
outside event, or a decision's own work. Under that come the evidence those events name by digest
(inputs, outputs, receipts), and what the run's verification amounted to: an agent run admitted in
observe mode captured evidence, evaluated no obligation and issued no certificate, and the page
says so. Any decision the runtime would not have made is marked where it happened and listed at
the top.

It also says how far an offline reader can go. The fold, the status and the spend are reproduced. Node
work is not: a model call can answer differently each time it is asked, a retrieval read the project's
memory as it stood then, an effect acted on the world, and even a deterministic node needs its input
values, which a bundle never carries. When the run was traced, the bundle's `trace_id` finds its spans in
your tracing backend.

The fixtures in `crates/replay/tests/fixtures` are bundles from real runs that succeeded, failed, were
interrupted mid-call, and ran out of budget.
