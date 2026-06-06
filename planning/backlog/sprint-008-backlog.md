# Sprint 008 Backlog

This is the working backlog for Sprint 008: Authoring Foundation.

Status legend:

- `todo`: not started
- `doing`: in progress
- `blocked`: waiting on a decision or prerequisite
- `done`: complete

## Job Definitions

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S8-JD-001 | todo | Add job definition create API test | `POST /v1/job-definitions` creates a reusable Python job definition |
| S8-JD-002 | todo | Add job definition list/fetch API tests | `GET /v1/job-definitions` and `GET /v1/job-definitions/{id}` return created definitions |
| S8-JD-003 | todo | Add job definition update/delete API tests | Update changes name/script/pool defaults; delete prevents future submissions |
| S8-JD-004 | todo | Add persistence methods for job definition CRUD | PostgreSQL can create, list, find, update, and delete user job definitions |
| S8-JD-005 | todo | Store Python scripts as job-definition bundles | Scripts are stored under stable object keys, not run-scoped keys |
| S8-JD-006 | todo | Keep run-scoped script submission working | `python_script` run submission continues to create run-scoped definitions |
| S8-JD-007 | todo | Add dashboard job definition API client | Frontend can create/list/fetch job definitions |
| S8-JD-008 | todo | Add dashboard job definition create/list UI | User can create and see a reusable Python job definition |
| S8-JD-009 | todo | Wire run submission to API job definitions | Run form lists user-created and seeded definitions from the API |

## Execution Pools

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S8-POOL-001 | todo | Add execution pool list API test | `GET /v1/execution-pools` returns configured pool names |
| S8-POOL-002 | todo | Implement execution pool list endpoint | API returns pool choices from configuration |
| S8-POOL-003 | todo | Add dashboard execution pool API client | Frontend fetches pools from API |
| S8-POOL-004 | todo | Wire run forms to API pools | Seeded/script/job-definition forms use live pool choices |
| S8-POOL-005 | todo | Replace fake execution-pool page data | Page stops showing hard-coded nodes, CPU, memory, running, and queued values |

## Dashboard Truthfulness

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S8-UI-001 | done | Hide global topbar create action without real target | `+ Automation` no longer appears as a fake create action |
| S8-UI-002 | todo | Replace fake Automations page | Page states automation creation is next and links to MVP design |
| S8-UI-003 | todo | Replace fake Workflows page | Page states workflow creation is next and links to MVP design |
| S8-UI-004 | todo | Audit panel title actions | Inert panel buttons are removed or changed to links/commands |
| S8-UI-005 | done | Fix `/runs` table long-value collisions | Long run IDs and job definition IDs do not overlap adjacent columns |
| S8-UI-006 | done | Fix overview recent-runs long node overflow | Long node names stay inside the table and remain inspectable |

## Workflow and Automation MVP Design

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S8-DESIGN-001 | todo | Add workflow/automation MVP design doc | Design explains job definitions, workflows, automations, workflow runs, and step runs |
| S8-DESIGN-002 | todo | Specify database tables | Tables cover workflow definitions, workflow steps, automations, workflow runs, and workflow step runs |
| S8-DESIGN-003 | todo | Specify API endpoints | Workflow CRUD, automation CRUD, manual trigger, workflow run list/detail are scoped |
| S8-DESIGN-004 | todo | Specify orchestrator behavior | Scheduler/evaluator responsibility for sequential steps is clear |
| S8-DESIGN-005 | todo | Specify dashboard flows | Create workflow, create automation, trigger automation, inspect workflow run are scoped |

## Documentation and Planning

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S8-DOC-001 | done | Move alpha later in roadmap | Public alpha follows authoring/workflow capability |
| S8-DOC-002 | todo | Update architecture docs | Docs describe job definitions, execution pools, workflows, automations, and current gaps |
| S8-DOC-003 | todo | Update API docs | Docs include job definition and execution pool endpoints |
| S8-DOC-004 | todo | Update product backlog | Backlog is organized around authoring, workflows, automations, then alpha/release |

## Verification

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S8-VERIFY-001 | todo | Run Rust checks | `cargo fmt --check`, `cargo test --workspace`, and clippy pass |
| S8-VERIFY-002 | todo | Run dashboard checks | `npm run lint` and `npm run build` pass |
| S8-VERIFY-003 | todo | Run Compose authoring smoke | Create job definition, submit run, inspect logs/artifacts through dashboard |

## Sprint Risks

- Job definitions can expand into a full package manager. Keep Sprint 008 to Python script definitions.
- Execution pools can expand into mutable cluster scheduling policy. Keep Sprint 008 to listing configured pools.
- Workflows can expand into DAGs. Keep Sprint 009 preview to linear workflows.
- Dashboard pages can look empty after removing fake data. Prefer honest empty states over fake operational claims.

## Sprint Exit

Move any unfinished `todo` items back into `planning/backlog/product-backlog.md` or into Sprint 009 planning.
