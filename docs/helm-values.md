# Helm Values

This document summarizes the initial Sprint 001 Helm values. The authoritative defaults live in `charts/capsulet/values.yaml`.

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

## Validation

The chart includes `values.schema.json` for basic validation.

Run:

```sh
helm lint charts/capsulet
helm template capsulet charts/capsulet
```
