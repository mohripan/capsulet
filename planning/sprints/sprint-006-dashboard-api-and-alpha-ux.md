# Sprint 006: Dashboard API Integration and Alpha UX

## Sprint Goal

Turn the dashboard from a static prototype into a functional public-alpha surface for the runtime delivered through Sprints 002 through 005.

By the end of this sprint, a local evaluator should be able to:

1. Install or run the API, worker, PostgreSQL, object storage, and dashboard.
2. Open the dashboard.
3. See live run data from the API.
4. Submit a seeded job or a single-file Python script from the dashboard.
5. Inspect one run's status, attempts, logs, and artifacts.
6. Cancel a queued or running run from the dashboard.
7. Download an artifact from the dashboard.
8. Follow docs that explain the dashboard/API configuration and known alpha limits.

This sprint should create a useful product demo without adding authentication, workflow authoring, metrics, or a full release pipeline.

## Sprint Length

Recommended: 1 week.

## Sprint Theme

Real data, clear controls, honest limits.

The goal is not a polished multi-user console. The goal is to connect the existing dashboard shell to the real API in a way that is reliable, testable, and understandable during local public-alpha evaluation.

## Current Context

Sprint 005 completed:

- object storage boundary with filesystem and S3-compatible adapters
- script-backed run submission
- large-log offload
- runner artifact upload
- artifact metadata persistence
- artifact list and download through API and CLI
- MinIO-backed Docker smoke for bundles, logs, and artifacts
- Helm object storage values and docs

The dashboard currently exists as a Next.js visual prototype with static/mock data. Phase 2 of the roadmap calls for a dashboard with job list and job detail. Sprint 006 should connect that UI to the live API before chart packaging and release automation work.

## Committed Scope

### 1. Dashboard API Client Boundary

Add a small dashboard-side API client instead of scattering `fetch` calls through pages.

Expected work:

- Add a typed API client module in the dashboard app.
- Configure the API base URL through an environment variable.
- Normalize API errors into user-facing error states.
- Keep the client compatible with local `http://127.0.0.1:8080` and in-cluster service URLs.

Acceptance criteria:

- Dashboard pages call one shared client module for Capsulet API requests.
- Missing or invalid API URLs produce a clear dashboard state.
- TypeScript catches response-shape drift for run, log, and artifact objects used by the UI.

### 2. Live Runs List

Replace the static runs page with live run data.

Expected work:

- Fetch `GET /v1/jobs/runs`.
- Render run ID, status, job definition ID, execution pool, and attempt count.
- Show loading, empty, error, and refreshed states.
- Link each run to a run detail route.

Acceptance criteria:

- The runs page reflects newly submitted API/CLI runs without changing mock data.
- Empty databases render a useful empty state.
- API failures do not crash the page.

### 3. Run Detail Page

Add a detail page that makes one run inspectable.

Expected work:

- Fetch `GET /v1/jobs/runs/{id}`.
- Fetch `GET /v1/jobs/runs/{id}/logs`.
- Fetch `GET /v1/jobs/runs/{id}/artifacts`.
- Display state, attempt count, execution pool, logs, large-log availability, and artifact metadata.
- Keep the layout dense and operational rather than marketing-oriented.

Acceptance criteria:

- A completed run can be inspected from the dashboard.
- Large-log availability is visible when `object_log_available` is true.
- Missing runs render a not-found state.

### 4. Dashboard Run Submission

Let users create useful runs from the dashboard.

Expected work:

- Add a submit form for seeded job definitions such as `job_hello_python`, `job_fail_python`, `job_timeout_python`, and `job_artifact_python`.
- Add a single-file Python script submission flow using `python_script`.
- Allow selecting an execution pool.
- Show the created run and link to its detail page.

Acceptance criteria:

- A user can submit a seeded job from the dashboard.
- A user can submit a Python script from the dashboard.
- Validation prevents empty scripts and empty pool values.

### 5. Dashboard Cancellation

Expose cancellation for queued, leased, and running runs.

Expected work:

- Add a cancel action on run detail.
- Call `POST /v1/jobs/runs/{id}/cancel`.
- Disable the action for terminal states.
- Refresh visible state after cancellation.

Acceptance criteria:

- Cancelling from the dashboard produces the same final state as the CLI path.
- Terminal runs do not show a misleading cancel control.
- API cancellation errors are visible without losing the current page state.

### 6. Dashboard Artifact Download

Make artifact metadata useful from the run detail page.

Expected work:

- Render artifact ID, name, kind, size, and content type.
- Add a download button or link for each artifact.
- Use `GET /v1/jobs/runs/{id}/artifacts/{artifact_id}`.
- Preserve the artifact file name where the browser allows it.

Acceptance criteria:

- The dashboard can download `main.py`, `stdout.log`, and runner-produced artifacts.
- Missing object storage data renders a clear error.
- Artifact list remains readable for runs with no artifacts.

### 7. Helm And Runtime Configuration

Wire dashboard runtime configuration through the chart.

Expected work:

- Add dashboard environment values for the API base URL.
- Pass dashboard configuration through Helm templates.
- Document local port-forward and in-cluster API URL options.
- Keep API and worker configuration unchanged.

Acceptance criteria:

- `helm lint charts/capsulet` passes.
- `helm template capsulet charts/capsulet` renders dashboard API configuration.
- Local dashboard docs explain how to point the dashboard at a port-forwarded API.

### 8. Dashboard Tests And Smoke

Add focused verification for the new UI behavior.

Expected work:

- Add dashboard unit or component tests for API response mapping and status rendering.
- Add at least one browser smoke path that loads the runs page with a live API.
- Keep `npm run lint` and `npm run build` passing.
- Preserve Rust workspace, Helm, and Docker smoke checks from Sprint 005.

Acceptance criteria:

- Dashboard test coverage catches basic client and rendering regressions.
- Local smoke demonstrates live runs, submit, detail, cancel, logs, artifacts, and download.
- Existing Rust and Helm checks still pass.

### 9. Documentation

Document the live dashboard path and alpha limitations.

Expected work:

- Update installation docs with dashboard enablement.
- Add dashboard usage docs or update dashboard README.
- Update API docs only if the dashboard requires API shape changes.
- Update troubleshooting docs for dashboard/API URL and CORS/proxy issues.

Acceptance criteria:

- A contributor can run the dashboard against the local API from docs.
- Docs clearly state there is no authentication yet.
- Docs distinguish live dashboard pages from still-prototype pages.

## Stretch Scope

Only do these after committed scope is complete:

- Basic dashboard summary metrics derived from run list data.
- Client-side polling on run detail while a run is active.
- Artifact preview for text files.
- Dashboard route for execution pool definitions.
- Playwright screenshot checks across desktop and mobile viewports.

## Explicit Non-Goals

- no authentication or RBAC
- no dashboard workflow builder
- no automation trigger UI
- no metrics or Prometheus integration
- no chart dependency bundling for PostgreSQL or MinIO
- no release automation
- no OpenAPI generation
- no server-side artifact proxy beyond the existing API endpoint
- no real-time streaming logs

## Definition of Done

Sprint 006 is done when:

- dashboard reads live run data from the API
- dashboard can submit seeded jobs and single-file Python scripts
- dashboard can show run detail, logs, large-log availability, and artifacts
- dashboard can cancel cancellable runs
- dashboard can download artifacts
- Helm values/templates configure the dashboard API base URL
- docs explain local dashboard setup, usage, troubleshooting, and alpha limits
- dashboard lint/build checks pass
- Rust workspace tests and Helm checks still pass
- a local end-to-end smoke demonstrates dashboard submit, inspect, cancel, artifact list, and artifact download

## Suggested Work Order

1. Add the dashboard API client and response types.
2. Replace the runs page mock data with live API data.
3. Add the run detail route and fetch run/log/artifact data.
4. Add seeded job submission.
5. Add Python script submission.
6. Add dashboard cancellation.
7. Add artifact download.
8. Wire dashboard API base URL through Helm.
9. Add dashboard tests and build verification.
10. Update docs and troubleshooting.
11. Run Rust, dashboard, Helm, and Docker-backed smoke checks.

## Sprint Review Checklist

- Can someone inspect real runs without the CLI?
- Can someone submit a new script from the dashboard and watch it finish?
- Can someone retrieve a produced artifact from the dashboard?
- Are prototype pages clearly separated from live runtime pages?
- Does the dashboard fail clearly when the API URL is wrong?
- Are terminal versus cancellable run states obvious?
- Does the chart expose dashboard configuration without hard-coding local URLs?

## Sprint 007 Preview

Sprint 007 should choose one of these paths based on Sprint 006 results:

- bundled PostgreSQL and MinIO chart maturity for local public-alpha installs
- release automation for GHCR images and Helm chart packaging
- observability metrics for queue depth, worker outcomes, retries, and storage operations
- security hardening foundations such as image allowlists and network policy presets
