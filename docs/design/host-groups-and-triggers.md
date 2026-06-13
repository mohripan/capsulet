# Host Groups and Trigger Model

## Product Direction

Capsulet should present host groups as the user-facing routing concept. A host group is a named set of eligible execution hosts. A host may be a Kubernetes node, a Kubernetes-backed runner target, or a future external agent such as an EC2 instance.

The current implementation still stores this field as `execution_pool` because the first runner backend is Kubernetes Jobs. For compatibility, the API accepts `host_group` as an alias and exposes `GET /v1/host-groups`. In the Kubernetes runner, a host group maps to existing execution-pool configuration: node selectors, tolerations, resource defaults, timeouts, and concurrency limits.

## Target Runtime Roles

- `capsulet-api`: authoring and control-plane API.
- `capsulet-scheduler`: emits schedule-trigger events and advances queued workflow runs until a dedicated orchestrator exists.
- `capsulet-evaluator`: evaluates trigger condition trees and creates durable workflow or job runs.
- `capsulet-orchestrator`: target role for routing jobs to host groups and coordinating workflow hosts.
- `workflow-host`: future per-host or per-agent process that claims jobs assigned to its host group.
- `workflow-engine`: isolated container runtime that executes a Python job bundle on behalf of a workflow host.

In the current Kubernetes implementation, `capsulet-worker` combines orchestrator, workflow-host, and workflow-engine coordination by creating Kubernetes Jobs. That is acceptable for the first backend, but the docs and API should not imply Kubernetes nodes are the only possible host type.

## Automation Model

An automation should contain:

- a target job definition or workflow definition
- one or more named triggers
- a structured boolean condition tree
- a host group used for created jobs unless a workflow step overrides it

Initial trigger kinds:

- `manual`
- `schedule`
- `sql`

Later trigger kinds:

- `webhook`
- `dependency`
- `custom`

Condition trees are structured data, not raw text:

```json
{
  "all": [
    { "any": [{ "trigger": "trigger_a" }, { "trigger": "trigger_b" }] },
    { "trigger": "trigger_c" }
  ]
}
```

This maps to `(trigger_a OR trigger_b) AND trigger_c`.

## SQL Trigger Direction

SQL triggers should support two modes:

- query polling for local and simple deployments
- CDC events through Debezium or a compatible event source for production-shaped deployments

The trigger contract should store a durable trigger event with a dedupe key before evaluating automation conditions. Evaluation must be idempotent so repeated CDC messages or scheduler retries do not create duplicate runs.

## Python Sandbox Direction

Job definitions should support:

- editor-authored `main.py`
- uploaded Python files
- optional `requirements.txt`
- object-storage-backed bundles

The workflow engine should install dependencies and run user code inside an isolated container. The current implementation stores `main.py` in object storage and executes it through a configured Python image; adding `requirements.txt` requires a bundle format and runner install step rather than a control-plane-only change.
