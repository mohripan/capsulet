# Sprint 010 Backlog: MVP Hardening and Alpha Readiness

| ID | Status | Item | Acceptance |
| --- | --- | --- | --- |
| S10-API-001 | planned | Dedicated workflow run detail endpoint | Response adds richer step history beyond the Sprint 009 list response, including timestamps and linked job state |
| S10-FE-001 | planned | Workflow run detail page | Dashboard shows each step and links to job run details |
| S10-FE-002 | planned | Automation management controls | User can disable, update, and delete automations where allowed |
| S10-FE-003 | planned | Workflow management controls | User can edit or disable workflow definitions without breaking history |
| S10-FE-004 | planned | Job definition management controls | User can update/delete job definitions with clear conflict handling |
| S10-QA-001 | planned | Compose MVP smoke script | One command creates job definitions, workflow, automation, trigger, and verifies completion |
| S10-QA-002 | planned | Minikube MVP smoke guide | Kubernetes-backed execution path is documented and verified |
| S10-OPS-001 | planned | Scheduler observability | Operators can tell whether the scheduler is polling and advancing workflow runs |
| S10-DOC-001 | planned | Alpha gate checklist | Roadmap lists concrete blockers before public alpha |

## Notes

Sprint 010 is the hardening sprint that should happen before release packaging work resumes.
