# Authoring and Workflow MVP

> Historical design baseline. The linear workflow MVP described here has shipped and workflows now support persisted DAG dependencies. See [Architecture Overview](../architecture.md) for current behavior.

## Goal

Capsulet should let a user create reusable work, compose it into a workflow, bind that workflow to an automation, trigger it manually, and inspect the resulting execution end to end.

## Product Objects

### Job Definition

A job definition is reusable executable work.

Initial fields:

- `id`
- `name`
- `description`
- `runtime_image`
- `python_script`
- `default_execution_pool`
- `retry_max_attempts`
- `retry_delay_seconds`
- `created_at`
- `updated_at`

The existing `job_definitions` table already stores the executable runtime fields. Sprint 008 should add API support and, if needed, an additive migration for description/default pool/deletion metadata.

### Execution Pool

Execution pools are runtime targets. In the current architecture they are configured through service/chart configuration, not mutable dashboard objects.

Initial API fields:

- `name`
- `description`
- `is_default`

The dashboard must fetch execution pools from `GET /v1/execution-pools` instead of `mock-data.ts`.

### Workflow Definition

A workflow definition is an ordered list of steps.

Initial fields:

- `id`
- `name`
- `description`
- `status`: `draft`, `enabled`, or `disabled`
- `created_at`
- `updated_at`

### Workflow Step

A workflow step references one job definition and one execution pool.

Initial fields:

- `id`
- `workflow_id`
- `position`
- `name`
- `job_definition_id`
- `execution_pool`

Sprint 009 should support linear workflows only. `position` defines execution order.

### Automation

An automation binds a trigger to a workflow.

Initial fields:

- `id`
- `name`
- `description`
- `workflow_id`
- `status`: `enabled` or `disabled`
- `trigger_type`: initially only `manual`
- `created_at`
- `updated_at`

Schedules, webhooks, dependency triggers, and condition expressions are deferred.

### Workflow Run

A workflow run is one execution of a workflow definition.

Initial fields:

- `id`
- `workflow_id`
- `automation_id`
- `status`: `queued`, `running`, `succeeded`, `failed`, `cancelled`, or `timed_out`
- `current_step_position`
- `created_at`
- `updated_at`
- `finished_at`

### Workflow Step Run

A workflow step run links workflow execution to an underlying job run.

Initial fields:

- `id`
- `workflow_run_id`
- `workflow_step_id`
- `job_run_id`
- `position`
- `status`
- `created_at`
- `updated_at`

## API Shape

### Job Definitions

- `POST /v1/job-definitions`
- `GET /v1/job-definitions`
- `GET /v1/job-definitions/{id}`
- `PUT /v1/job-definitions/{id}`
- `DELETE /v1/job-definitions/{id}`

### Execution Pools

- `GET /v1/execution-pools`

### Workflows

- `POST /v1/workflows`
- `GET /v1/workflows`
- `GET /v1/workflows/{id}`
- `PUT /v1/workflows/{id}`
- `DELETE /v1/workflows/{id}`

### Automations

- `POST /v1/automations`
- `GET /v1/automations`
- `GET /v1/automations/{id}`
- `PUT /v1/automations/{id}`
- `DELETE /v1/automations/{id}`
- `POST /v1/automations/{id}/trigger`

### Workflow Runs

- `GET /v1/workflow-runs`
- `GET /v1/workflow-runs/{id}`
- `POST /v1/workflow-runs/{id}/cancel`

## Execution Flow

1. User creates one or more job definitions.
2. User creates a workflow with ordered steps.
3. User creates a manual automation targeting that workflow.
4. User triggers the automation.
5. API creates a `workflow_run` in `queued`.
6. Orchestrator starts the first step by creating a normal `job_run`.
7. Worker executes the job run through the existing runner path.
8. Orchestrator observes the job run terminal state.
9. If the step succeeds, orchestrator starts the next step.
10. If all steps succeed, orchestrator marks the workflow run `succeeded`.
11. If any step fails, times out, or is cancelled, orchestrator marks the workflow run terminal with the same failure class.

## Dashboard Flow

### Job Definitions

- List job definitions.
- Create a Python job definition.
- Edit name, script, runtime image, default pool, and retry policy.
- Submit a run from a job definition.

### Workflows

- List workflows.
- Create a linear workflow.
- Add, reorder, and remove steps.
- Choose job definition and execution pool per step.
- Enable or disable workflow.

### Automations

- List automations.
- Create a manual automation from a workflow.
- Enable or disable automation.
- Trigger automation.

### Workflow Runs

- List workflow runs.
- Show workflow run status and ordered steps.
- Link each step to the underlying job run detail.
- Surface logs and artifacts through existing run detail pages.

## Deferred Work

- schedule triggers
- webhook triggers
- dependency triggers
- condition builder
- DAG branching. See [Workflow DAG Graph Plan](workflow-dag-graph-plan.md).
- fan-out/fan-in. See [Workflow DAG Graph Plan](workflow-dag-graph-plan.md).
- artifact passing between steps
- parameter schemas
- authentication and RBAC
- visual workflow graph editor

## Sprint Mapping

Sprint 008:

- job definition CRUD
- execution pool list API
- dashboard job definition authoring
- remove misleading mock pages
- keep workflow/automation to this design

Sprint 009:

- workflow CRUD
- automation CRUD
- manual trigger
- sequential orchestrator
- dashboard workflow/automation create and run flow

Sprint 010:

- harden workflow run detail
- cancellation and retry behavior for workflows
- dashboard polish
- end-to-end Kubernetes/minikube smoke

Public alpha:

- after Sprint 010 or later, once authoring and workflow execution are real.
