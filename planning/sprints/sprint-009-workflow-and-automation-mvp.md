# Sprint 009: Workflow and Automation MVP

## Goal

Turn the authoring foundation into a usable end-to-end Capsulet MVP path: create job definitions, compose a linear workflow, create manual or interval automations, execute workflow steps, and inspect job runs, logs, and artifacts.

## Scope

- Persist workflow definitions and workflow steps.
- Persist automations with manual and fixed-interval triggers.
- Add scheduler behavior for due interval automations.
- Add scheduler behavior for sequential workflow step advancement.
- Add dashboard workflow and automation creation screens backed by live API data.
- Remove misleading hard-coded workflow, automation, and execution-pool dashboard state.
- Provide an email dry-run example that can be scheduled hourly.

## Non-Goals

- Branching workflows.
- Condition trees.
- Per-user ownership and RBAC.
- Email provider integration UI.
- Release image publishing.

## Tasks

- Add workflow and automation domain types.
- Add workflow and automation PostgreSQL migrations.
- Add workflow and automation API endpoints.
- Add execution pool list endpoint.
- Add scheduler loop that triggers due interval automations.
- Add scheduler loop that advances workflow runs after step job completion.
- Wire dashboard job definition, workflow, automation, and execution pool screens to live APIs.
- Add a safe email dry-run example.
- Update API, architecture, README, and design docs.
- Verify Rust tests, clippy, dashboard build, and Docker Compose smoke.

## Acceptance Criteria

- A user can create a Python job definition in the dashboard.
- A user can create a workflow from created job definitions.
- A user can create a manual automation and trigger it from the dashboard.
- A user can create an interval automation that the scheduler turns into workflow runs.
- Workflow runs create normal job runs, and completed step runs advance to the next step.
- Logs and artifacts for workflow-created job runs are visible through the existing run detail flow.
- Docker Compose starts PostgreSQL, MinIO, API, scheduler, worker, and dashboard together.

## Review Notes

- The MVP workflow model is intentionally linear.
- Interval scheduling uses seconds-based polling and is suitable for local/product validation, not a full cron replacement.
- Docker Compose smoke verified authored job definitions, manual and interval automations, workflow advancement, workflow step job links, logs, and artifacts.
- The dashboard still needs a dedicated workflow run detail page before public alpha.
