# Sprint 010: MVP Hardening and Alpha Readiness

## Goal

Harden the MVP into a credible pre-alpha product: make workflow execution easier to inspect, reduce operational blind spots, and prepare the remaining work needed before a public alpha.

## Scope

- Dedicated workflow run detail API and dashboard page with richer step history.
- Dashboard edit/delete flows for job definitions, workflows, and automations where safe.
- Better validation and conflict handling for deleting resources referenced by runs.
- Scheduler observability and troubleshooting docs.
- Compose and minikube smoke scripts for the full authoring-to-execution path.
- Clear alpha gate checklist.

## Non-Goals

- Public image publishing.
- Helm chart release packaging.
- Full multi-tenant authentication.
- Branching workflow engine.

## Tasks

- Add dedicated workflow run detail endpoint with timestamps, step runs, and underlying job state.
- Add dashboard workflow run detail view.
- Add links from automation workflow runs to job run detail pages.
- Add update/disable/delete controls for automations.
- Add safe edit/delete behavior for job definitions and workflows.
- Add scheduler health/metrics endpoint or log-based smoke guidance.
- Add automated Docker Compose MVP smoke script.
- Add minikube MVP smoke guide.
- Define public alpha entrance criteria.

## Acceptance Criteria

- A workflow run can be inspected step by step from a dedicated dashboard detail view.
- A user can navigate from a workflow run to the logs/artifacts for each underlying job run.
- Deleting or editing resources with historical runs does not corrupt execution history.
- The full MVP smoke path has a repeatable command or documented checklist.
- The roadmap identifies the remaining public alpha blockers explicitly.

## Review Notes

- This sprint should be completed before returning to release image publishing and Helm chart packaging.
- Public alpha remains later than this sprint unless auth, packaging, and install hardening are also complete.
