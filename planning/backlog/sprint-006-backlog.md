# Sprint 006 Backlog

This is the working backlog for Sprint 006: Dashboard API Integration and Alpha UX.

Status legend:

- `todo`: not started
- `doing`: in progress
- `blocked`: waiting on a decision or prerequisite
- `done`: complete

## Dashboard API Client

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S6-CLIENT-001 | done | Add dashboard API base URL config | Dashboard can target local API and in-cluster API URLs without code edits |
| S6-CLIENT-002 | done | Add typed Capsulet API client | Runs, logs, artifacts, submit, and cancel calls share one client boundary |
| S6-CLIENT-003 | done | Normalize dashboard API errors | Pages can render clear error states for missing API, not found, and store failures |
| S6-CLIENT-004 | done | Add API client tests | Response mapping and error mapping are covered |

## Runs List

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S6-RUNS-001 | done | Replace mock runs with live data | `GET /v1/jobs/runs` drives the dashboard runs page |
| S6-RUNS-002 | done | Add run list states | Loading, empty, error, and populated states render cleanly |
| S6-RUNS-003 | done | Add run detail links | Each run links to its live detail page |
| S6-RUNS-004 | done | Add run list rendering tests | Status, pool, attempt count, and empty state are covered through build, helper tests, and smoke |

## Run Detail

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S6-DETAIL-001 | done | Add run detail route | `/runs/{id}` renders one run from the API |
| S6-DETAIL-002 | done | Show captured logs | Detail page renders `GET /logs` output and handles missing logs |
| S6-DETAIL-003 | done | Show object log availability | Detail page shows when a full object-backed `stdout.log` artifact exists |
| S6-DETAIL-004 | done | Show artifact metadata | Detail page lists artifact ID, name, kind, size, and content type |
| S6-DETAIL-005 | done | Add not-found handling | Missing runs render a clear not-found state |

## Submission

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S6-SUBMIT-001 | done | Add seeded job submission form | Dashboard can submit `job_hello_python`, `job_fail_python`, `job_timeout_python`, and `job_artifact_python` |
| S6-SUBMIT-002 | done | Add Python script submission form | Dashboard can submit `python_script` runs |
| S6-SUBMIT-003 | done | Add execution pool selector | Submission can choose at least `mini` or `large` |
| S6-SUBMIT-004 | done | Validate submission fields | Empty pool, empty job definition, and empty script are rejected before API call |
| S6-SUBMIT-005 | done | Link created runs | Successful submission links to the created run detail page |

## Cancellation

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S6-CANCEL-001 | done | Add cancel action to run detail | Cancellable runs can call `POST /cancel` |
| S6-CANCEL-002 | done | Hide cancel for terminal states | `succeeded`, `failed`, `cancelled`, and `timed_out` do not show active cancel controls |
| S6-CANCEL-003 | done | Refresh state after cancellation | Detail page shows the updated run after a cancel response |
| S6-CANCEL-004 | done | Add cancellation UI tests | Cancellable and terminal state behavior is covered through helper tests and smoke |

## Artifact Download

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S6-ARTIFACT-001 | done | Add artifact download control | Dashboard downloads artifacts through the API endpoint |
| S6-ARTIFACT-002 | done | Preserve artifact names | Browser download uses the artifact name where practical |
| S6-ARTIFACT-003 | done | Handle storage download failures | Missing object bytes render a clear error |
| S6-ARTIFACT-004 | done | Add artifact download tests | Download URL/action behavior is covered through smoke |

## Helm And Configuration

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S6-HELM-001 | done | Add dashboard API URL value | Chart values include dashboard API base URL configuration |
| S6-HELM-002 | done | Pass dashboard API URL to deployment | Rendered dashboard pod receives the API URL |
| S6-HELM-003 | done | Add schema coverage | `values.schema.json` validates dashboard API URL settings |
| S6-HELM-004 | done | Keep Helm checks passing | `helm lint` and `helm template` pass |

## Documentation

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S6-DOC-001 | done | Update installation docs | Dashboard enablement and API URL configuration are documented |
| S6-DOC-002 | done | Update dashboard README | Local dashboard development against the API is documented |
| S6-DOC-003 | done | Update troubleshooting docs | API URL, unreachable API, and dashboard runtime config failures are covered |
| S6-DOC-004 | done | Document live versus prototype pages | Users know which dashboard pages are connected to the API |
| S6-DOC-005 | done | Document alpha limitations | No auth, no workflow UI, no streaming logs, and no live metrics are explicit |

## Quality

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S6-QA-001 | done | Keep dashboard lint passing | `npm run lint` passes in `dashboard/` |
| S6-QA-002 | done | Keep dashboard build passing | `npm run build` passes in `dashboard/` |
| S6-QA-003 | done | Keep Rust workspace tests passing | `cargo test --workspace` passes |
| S6-QA-004 | done | Keep Rust clippy passing | `cargo clippy --workspace --all-targets -- -D warnings` passes |
| S6-QA-005 | done | Keep Helm checks passing | `helm lint` and dashboard-enabled `helm template` pass |
| S6-QA-006 | done | Complete dashboard smoke | Local smoke covers live runs, submit, detail, cancel, logs, artifacts, and download |

## Sprint Risks

- The dashboard can grow into a redesign. Keep Sprint 006 focused on live API integration and operational clarity.
- Browser access to the API can run into CORS or deployment URL questions. Prefer a clear dashboard API configuration value and document local port-forwarding.
- Artifact download can tempt previews and file-type handling. Keep Sprint 006 to reliable download controls.
- Live polling can complicate state management. Add manual refresh first; polling is stretch scope.
- No authentication means dashboard actions are open to whoever can reach the API. Document this clearly instead of hiding it.

## Completed Notes

- Added a dashboard-side same-origin API proxy configured by `CAPSULET_DASHBOARD_API_URL`.
- Added a typed dashboard API client for runs, logs, artifacts, submit, cancel, and artifact download.
- Replaced `/runs` mock data with live API data, loading/error/empty states, seeded job submission, and Python script submission.
- Added `/runs/{id}` detail with run state, logs, object-log availability, artifact metadata, cancellation, and artifact download.
- Added `/healthz` for dashboard probes and `Dockerfile.dashboard` for the chart image path.
- Added Helm dashboard API URL values/schema/config rendering.
- Updated docs for dashboard local development, Helm configuration, troubleshooting, and live-versus-prototype pages.
- Completed Docker-backed dashboard smoke through the dashboard proxy: list runs, submit script, cancel run, run worker, inspect logs, list artifacts, and download artifact.

## Remaining Notes

- No new Kubernetes runner code was added; Sprint 006 used the existing Kubernetes-capable runtime and chart, with Docker-backed local E2E for the dashboard/API integration path.
- Authentication, workflow UI, streaming logs, metrics, and bundled chart dependencies remain future work.

## Sprint Exit

Move any unfinished `todo` items back into `planning/backlog/product-backlog.md` or into Sprint 007 planning.
