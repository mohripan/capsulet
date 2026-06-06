# Capsulet

Capsulet is a planned Kubernetes-native automation platform, job queue, and sandboxed script execution system. The project goal is to ship an installable cloud-native product, distributed primarily as a Helm chart, that lets teams define automations, trigger scripts or workflows, run them as durable background work, capture logs and artifacts, and scale execution with Kubernetes.

The early product shape is intentionally narrow: author reusable Python jobs, compose them into linear workflows, trigger those workflows manually or on a fixed interval, execute each step as isolated background work, store logs and artifacts, retry failures, and inspect status through an API, CLI, and dashboard. The long-term direction is a production-grade workflow engine for script-centric automation.

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

The portfolio and engineering focus is cloud-native product quality: Helm distribution, production-shaped configuration, secure defaults, observable behavior, documentation, and eventually repeatable release automation.

## Target Architecture

The first complete architecture is expected to include:

- `capsulet-api`: HTTP API for job definitions, workflows, automations, job submission, status, logs, artifacts, and admin operations
- `capsulet-worker`: leases queued jobs and creates Kubernetes Jobs for execution
- `capsulet-scheduler`: creates due interval automation runs and advances workflow steps
- `capsulet-dashboard`: web UI for authoring jobs, workflows, automations, and inspecting runs
- `capsulet-cli`: local command-line client for submitting jobs and inspecting results
- PostgreSQL: durable metadata store
- MinIO or S3-compatible object storage: artifacts and large log payloads
- Kubernetes Jobs: sandboxed script execution runtime
- Helm chart: installable product package

The central execution flow:

1. A user creates a reusable Python job definition.
2. A user creates a workflow from one or more job definitions.
3. A user creates a manual or interval automation for that workflow.
4. The API validates the request and stores control-plane state in PostgreSQL.
5. The scheduler creates due workflow runs and starts each workflow step as a queued job run.
6. A worker leases the job run.
7. The worker creates a Kubernetes Job in the configured namespace, or uses the local stub runner in Docker Compose.
8. Kubernetes or the stub runner runs the script.
9. The worker captures logs, stores artifacts, and records the result.
10. The scheduler advances the workflow, and the API, CLI, and dashboard expose the final state.

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

## Installation Modes

Local Docker Compose demo:

```sh
docker compose up --build
```

This starts PostgreSQL, MinIO, the API, the scheduler, the stub-backed worker, and the dashboard. Open:

```text
http://127.0.0.1:3000/job-definitions
```

This path is for checking the current product flow locally: create a job definition, create a workflow, create an automation, trigger it, and inspect the underlying runs. Kubernetes-backed execution is covered by the Helm/minikube path.

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
  --set postgresql.mode=external \
  --set config.databaseUrlSecret.name=capsulet-db \
  --set minio.mode=external \
  --set config.objectStorage.mode=s3 \
  --set config.objectStorage.endpoint=http://minio.example:9000 \
  --set config.objectStorage.credentialsSecret.name=capsulet-object-storage
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
- [API](docs/api.md)
- [Helm values](docs/helm-values.md)
- [Persistence](docs/persistence.md)
- [Worker and runner](docs/worker-runner.md)
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
