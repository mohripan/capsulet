# Sprint 008: Authoring Foundation

## Sprint Goal

Let users create the first durable product objects themselves: reusable Python job definitions, real execution-pool choices, and the design/API foundation for workflows and automations.

This replaces the earlier alpha-polish framing. Capsulet is not alpha-ready while Automations, Workflows, and Execution Pools are hard-coded dashboard pages.

By the end of this sprint, a user should be able to:

1. Create a reusable Python job definition from the dashboard.
2. List, inspect, and submit that user-created job definition.
3. Choose execution pools returned by the API instead of hard-coded frontend arrays.
4. Use dashboard pages that do not present mock automations, workflows, or pool data as live operational state.
5. Review a concrete workflow/automation MVP design that maps directly onto Sprint 009 implementation.

## Sprint Length

Recommended: 1 week.

## Sprint Theme

Author real objects before publishing anything.

## Product Model

Capsulet should expose these objects in this order:

1. **Job definition**: reusable executable work, initially a Python script plus runtime settings.
2. **Execution pool**: configured runtime target exposed by the API from service/chart configuration.
3. **Workflow**: ordered steps, where each step references a job definition and execution pool.
4. **Automation**: trigger binding that starts a workflow, with manual trigger first.
5. **Workflow run**: durable execution record that links to underlying job runs, logs, and artifacts.

Sprint 008 implements the first two and designs the next three.

## Current Context

Existing live implementation:

- one-off job run submission
- run list/detail
- cancellation
- logs
- artifacts
- script-backed run submission that creates run-scoped job definitions
- static execution-pool validation from service configuration

Existing gaps:

- no job definition CRUD API for user-created reusable jobs
- no execution-pool API for the dashboard
- no workflow definitions
- no workflow runs
- no automation records
- no manual automation trigger
- Automations, Workflows, and Execution Pools dashboard pages use mock data
- global topbar create controls can imply functionality that does not exist

## Committed Scope

### 1. User-Created Python Job Definitions

Create the first reusable authoring object.

Expected work:

- Add API endpoints to create, list, fetch, update, and delete job definitions.
- Store user-submitted Python scripts in object storage under stable job-definition keys.
- Persist job definitions in PostgreSQL using the existing `job_definitions` table or a minimal additive migration if needed.
- Keep run-scoped script submissions working.
- Add dashboard UI for creating and listing Python job definitions.
- Let the existing run submission UI choose user-created job definitions.

Acceptance criteria:

- A user can create a Python job definition named `daily-report`.
- The dashboard lists `daily-report`.
- The dashboard can submit a run using `daily-report`.
- The worker executes the stored Python script through the existing bundle path.
- Logs and artifacts remain visible through the existing run detail page.

### 2. Execution Pool API

Stop hard-coding execution pools in the frontend.

Expected work:

- Add `GET /v1/execution-pools`.
- Return pool names from the configured API execution pools.
- Include enough fields for the current dashboard: name, description when available, and whether it is the default.
- Update dashboard run submission and execution-pool page to use the API.
- Remove hard-coded pool cards that imply live cluster/node metrics.

Acceptance criteria:

- Dashboard pool selectors use `GET /v1/execution-pools`.
- The execution-pool page no longer shows fake node names, CPU, memory, queued, or running counts.
- Unknown pool validation still rejects invalid run submissions.

### 3. Dashboard Truthfulness Pass

Remove misleading mock operational state from authoring pages.

Expected work:

- Replace the fake Automations page with an empty/coming-next state that links to the workflow/automation MVP design.
- Replace the fake Workflows page with an empty/coming-next state that links to the workflow/automation MVP design.
- Keep the topbar primary action hidden unless a page has an implemented creation flow.
- Audit panel title actions and remove inert buttons.

Acceptance criteria:

- No page claims schedules, webhooks, dependency triggers, workflow lineage, fake nodes, fake cluster metrics, or fake policies are available.
- The dashboard remains useful for live runs and job definitions.
- Frontend lint and build pass.

### 4. Workflow and Automation MVP Design

Create the implementation design for the end-to-end product flow the user wants.

Expected work:

- Add a design doc for linear workflow definitions.
- Add a design doc section for manual automations.
- Define database tables for workflow definitions, workflow steps, automations, workflow runs, and workflow step runs.
- Define API endpoints for workflow CRUD, automation CRUD, manual trigger, workflow run list/detail.
- Define scheduler/orchestrator responsibilities for starting the next step after the previous step succeeds.
- Define dashboard pages and states for create workflow, create automation, trigger automation, and inspect workflow run.

Acceptance criteria:

- Sprint 009 can implement workflow/automation end to end from the design without changing core terminology.
- The design explicitly defers schedules, webhooks, condition builders, fan-out/fan-in, and dependency triggers.
- The roadmap points alpha after this workflow/automation product path, not before it.

## Stretch Scope

Only do these after committed scope is complete:

- Implement workflow definition CRUD API without execution.
- Add CLI commands for job definition CRUD.
- Add dashboard screenshots or Playwright checks for job definition authoring.

## Explicit Non-Goals

- no public alpha release
- no GHCR image publishing
- no Helm repository publishing
- no schedule triggers
- no webhook triggers
- no dependency triggers
- no DAG/fan-out/fan-in execution
- no visual workflow builder
- no authentication or RBAC
- no fake dashboard data to make unfinished pages look complete

## Definition Of Done

Sprint 008 is done when:

- user-created Python job definitions are durable and runnable
- execution pool selectors and the execution-pool page use API data
- mock Automations/Workflows/Execution Pools operational claims are removed
- the workflow/automation MVP design exists and is linked from planning docs
- roadmap public alpha is moved after authoring/workflow capability
- Rust tests, dashboard lint/build, and an end-to-end Compose smoke pass

## Suggested Work Order

1. Add job definition API tests.
2. Add job definition API and persistence support.
3. Store job-definition script bundles in object storage.
4. Add dashboard API client methods for job definitions.
5. Build dashboard job definition list/create flow.
6. Wire run submission to list job definitions from the API.
7. Add execution-pool API tests and endpoint.
8. Wire dashboard pool selectors and execution-pool page to API data.
9. Remove misleading mock operational pages.
10. Write workflow/automation MVP design.
11. Run Compose smoke: create job definition, submit it, inspect run, logs, and artifacts.

## Sprint Review Checklist

- Can a user create their own reusable Python job definition?
- Can a user submit that job definition without manually editing the database?
- Are execution-pool choices API-backed?
- Did the dashboard stop pretending automations/workflows exist before they do?
- Is the workflow/automation implementation path clear for Sprint 009?
- Is alpha clearly deferred until users can create and run workflows/automations end to end?

## Sprint 009 Preview

Sprint 009 should implement the first end-to-end workflow/automation path:

- workflow definition CRUD for linear steps
- automation CRUD for manual triggers
- manual automation trigger endpoint
- workflow run orchestration through scheduler/evaluator or a small orchestrator loop
- dashboard create workflow, create automation, trigger automation, inspect workflow run
