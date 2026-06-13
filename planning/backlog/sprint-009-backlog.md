# Sprint 009 Backlog: Workflow and Automation MVP

| ID | Status | Item | Acceptance |
| --- | --- | --- | --- |
| S9-DOM-001 | done | Add workflow and automation domain types | Core exposes workflow, workflow run, automation, and trigger status types |
| S9-DB-001 | done | Add workflow and automation tables | Migrations create workflow definitions, steps, automations, workflow runs, and workflow step runs |
| S9-API-001 | done | Add job definition authoring endpoints | Dashboard can create and list reusable Python job definitions |
| S9-API-002 | done | Add workflow endpoints | Dashboard can create and list linear workflows |
| S9-API-003 | done | Add automation endpoints | Dashboard can create/list automations and manually trigger one |
| S9-SCH-001 | done | Trigger due interval automations | Scheduler creates queued workflow runs for due enabled interval automations |
| S9-SCH-002 | done | Advance workflow runs | Scheduler starts next steps and marks workflow runs terminal after step completion |
| S9-FE-001 | done | Wire job definitions page to live API | No hard-coded job definition data is shown as real state |
| S9-FE-002 | done | Wire execution pools page to live API | No hard-coded execution pool rows remain |
| S9-FE-003 | done | Wire workflows page to live API | User can create a linear workflow from authored job definitions |
| S9-FE-004 | done | Wire automations page to live API | User can create and trigger automations |
| S9-DOC-001 | done | Add email dry-run example | Example documents dry-run mode and SMTP environment variables |
| S9-QA-001 | done | Compose smoke | Rebuilt Compose and verified authored job definitions, workflow, manual automation, interval automation, workflow advancement, logs, and artifacts |

## Deferred

- Branching and conditional workflow steps.
- Cron expressions.
- Dedicated workflow run detail page with richer step history.
- Update/delete controls for workflows and automations.
- Authenticated multi-user ownership.
