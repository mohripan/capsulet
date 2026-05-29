# Capsulet

Capsulet is a planned Kubernetes-native automation platform, job queue, and sandboxed script execution system. The project goal is to ship an installable cloud-native product, distributed primarily as a Helm chart, that lets teams define automations, trigger scripts or workflows, run them as durable background work, capture logs and artifacts, and scale execution with Kubernetes.

The early product shape is intentionally narrow: submit a Python script job, persist it, execute it in an isolated Kubernetes Job, stream logs, store artifacts, retry failures, and inspect status through an API, CLI, and small dashboard. The long-term direction is a production-grade workflow engine for script-centric automation.

## Product Vision

Capsulet should feel like a real Kubernetes application, not a demo that happens to run in containers.

Users should eventually be able to install it with Helm:

```sh
helm repo add capsulet https://mohripan16.github.io/capsulet-charts
helm repo update

helm install capsulet capsulet/capsulet \
  --namespace capsulet \
  --create-namespace
```

Then access the dashboard locally:

```sh
kubectl port-forward svc/capsulet-dashboard 3000:80 -n capsulet
```

The Helm chart is a first-class deliverable. It should install the API, worker, scheduler, dashboard, RBAC, services, configuration, optional PostgreSQL, optional MinIO, and the security controls needed to run user-provided code in a dedicated namespace.

## Why Capsulet Exists

Many teams have small scripts that grow into fragile scheduled jobs, manual runbooks, or scattered CI tasks. Capsulet is intended to provide a durable execution layer for those scripts:

- define automations with one or more trigger conditions
- submit work through an API or CLI
- run scripts in isolated Kubernetes pods
- persist status, attempts, logs, and outputs
- retry failed jobs with predictable policies
- scale workers horizontally
- use object storage for artifacts
- install and operate the platform with standard Kubernetes tooling

The portfolio and engineering focus is cloud-native product quality: Helm distribution, production-shaped configuration, secure defaults, observability, documentation, and release automation.

## Target Architecture

The first complete architecture is expected to include:

- `capsulet-api`: HTTP API for job submission, status, logs, artifacts, and admin operations
- `capsulet-worker`: leases queued jobs and creates Kubernetes Jobs for execution
- `capsulet-scheduler`: handles scheduled jobs, retries, and delayed work
- `capsulet-dashboard`: web UI for job inspection and basic submission
- `capsulet-cli`: local command-line client for submitting jobs and inspecting results
- PostgreSQL: durable metadata store
- MinIO or S3-compatible object storage: artifacts and large log payloads
- Kubernetes Jobs: sandboxed script execution runtime
- Helm chart: installable product package

The central execution flow:

1. A user submits a script job.
2. The API validates the request and stores the job in PostgreSQL.
3. A worker leases the job.
4. The worker creates a Kubernetes Job in the configured namespace.
5. Kubernetes runs the script in an isolated pod.
6. The worker watches status, streams logs, stores artifacts, and records the result.
7. The API, CLI, and dashboard expose the final state.

## Security Direction

Capsulet runs user-provided code, so the security model must be explicit from the beginning. The project should default toward constrained execution and honest documentation.

Planned controls include:

- dedicated execution namespace
- non-root containers
- dropped Linux capabilities
- disabled privilege escalation
- read-only root filesystems where practical
- seccomp `RuntimeDefault`
- configurable resource requests and limits
- job timeouts
- network policy support
- minimal RBAC for workers
- separate service accounts for platform pods and script jobs
- documented production warnings for untrusted workloads

Capsulet should not claim to be a perfect sandbox. The documented guidance should recommend dedicated clusters or namespaces, strict network policy, resource limits, and careful image allowlists for sensitive environments.

## Planned Installation Modes

Default local install:

```sh
helm install capsulet capsulet/capsulet \
  --namespace capsulet \
  --create-namespace
```

Production-shaped install with external dependencies:

```sh
helm install capsulet capsulet/capsulet \
  --namespace capsulet \
  --create-namespace \
  --set postgresql.enabled=false \
  --set externalDatabase.enabled=true \
  --set externalDatabase.existingSecret=capsulet-db \
  --set minio.enabled=false \
  --set externalObjectStorage.enabled=true
```

Scale workers:

```sh
helm upgrade capsulet capsulet/capsulet \
  --namespace capsulet \
  --set worker.replicaCount=5
```

## Repository Plan

The expected source layout is:

```text
crates/
  api/
  worker/
  scheduler/
  runner/
  cli/
  core/
  postgres/
dashboard/
charts/
  capsulet/
examples/
docs/
migrations/
.github/
  workflows/
```

This repository is currently in the planning stage. See [ROADMAP.md](ROADMAP.md) for the staged delivery plan and [ARCHITECTURE.md](ARCHITECTURE.md) for the target system architecture.

Useful project docs:

- [Development](docs/development.md)
- [Installation](docs/installation.md)
- [Helm values](docs/helm-values.md)
- [Persistence](docs/persistence.md)
- [Planning](planning/README.md)
- [Backend workspace](crates/README.md)
- [Dashboard prototype](dashboard/README.md)

## Distribution Goals

Capsulet should eventually publish:

- container images to GitHub Container Registry
- Helm chart packages to a GitHub Pages chart repository
- OCI Helm charts to GitHub Container Registry
- release notes from GitHub releases
- Artifact Hub metadata for discovery

The intended release flow is tag-driven:

```sh
git tag v0.1.0
git push origin v0.1.0
```

From there, CI should test, build, package, and publish the release artifacts.

## License

Capsulet is licensed under the Apache License 2.0. See [LICENSE](LICENSE).
