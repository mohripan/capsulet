# Sprint 007: Bundled Dependencies and Chart Alpha

## Sprint Goal

Make the Helm chart install a self-contained public-alpha Capsulet stack for local Kubernetes evaluation.

By the end of this sprint, a local evaluator should be able to:

1. Install Capsulet with `helm install` into a clean local cluster.
2. Get PostgreSQL, MinIO, API, worker, scheduler, evaluator, and dashboard resources from one chart install.
3. Run database migrations without a manual local Docker Compose database.
4. Submit a Python script through the dashboard or CLI.
5. Execute the job through the Kubernetes runner.
6. Store script bundles, large logs, and artifacts in bundled MinIO.
7. Inspect runs and download artifacts through the dashboard.
8. Follow installation docs that clearly distinguish bundled local dependencies from production external dependencies.

This sprint should turn the existing runtime and dashboard into an installable alpha chart path. It should not attempt to solve release publishing, metrics, authentication, or production-grade persistence.

## Sprint Length

Recommended: 1 week.

## Sprint Theme

One-command local install, honest dependency boundaries.

The product gap after Sprint 006 is no longer the user flow. The gap is that the chart still expects users to bring important dependencies themselves. Sprint 007 should close that gap for local public-alpha installs while preserving external PostgreSQL and S3-compatible object storage configuration for realistic deployments.

## Current Context

Sprint 006 completed:

- live dashboard run listing
- run detail, logs, cancellation, and artifact metadata
- seeded job and single-file Python script submission from the dashboard
- artifact download through the dashboard
- dashboard API URL chart configuration
- Docker-backed dashboard/API/worker/object-storage smoke coverage

The roadmap's Phase 2 public alpha chart features still require:

- bundled PostgreSQL for local installs
- bundled MinIO for local installs
- external database support
- external object storage support
- dashboard deployment and service
- worker and scheduler deployments
- `helm lint` and `helm template` smoke tests

External database and object storage support already exist at the values/configuration boundary. Dashboard deployment and service were handled in Sprint 006. Sprint 007 should therefore focus on bundled dependency templates, migration/install behavior, and chart-level smoke documentation.

## Committed Scope

### 1. Dependency Mode Values

Add explicit Helm values for bundled versus external dependencies.

Expected work:

- Add `postgresql.mode` with supported values `bundled` and `external`.
- Add bundled PostgreSQL image, auth, service, persistence, and resource defaults.
- Add `minio.mode` with supported values `bundled` and `external`.
- Add bundled MinIO image, auth, service, bucket, persistence, and resource defaults.
- Keep existing `config.databaseUrlSecret` and `config.objectStorage.credentialsSecret` usable for external mode.
- Add `values.schema.json` coverage for all new dependency values.

Acceptance criteria:

- `helm lint charts/capsulet` validates default values.
- `helm template capsulet charts/capsulet` renders bundled PostgreSQL and MinIO resources by default.
- External-mode values render no bundled PostgreSQL or MinIO StatefulSet.
- Invalid dependency modes fail schema validation.

### 2. Bundled PostgreSQL Templates

Add chart-managed PostgreSQL for local alpha installs.

Expected work:

- Render a PostgreSQL Secret with database, user, password, and `DATABASE_URL`.
- Render a PostgreSQL Service.
- Render a PostgreSQL StatefulSet with a persistent volume claim when persistence is enabled.
- Render an `emptyDir`-backed local path when persistence is disabled for throwaway installs.
- Use the chart's common labels and fullname helpers.
- Keep the API and worker reading `CAPSULET_DATABASE_URL` through the existing secret/key mechanism.

Acceptance criteria:

- Default chart output includes exactly one PostgreSQL Secret, Service, StatefulSet, and PVC template path.
- API and worker receive `CAPSULET_DATABASE_URL` from the bundled Secret when `postgresql.mode: bundled`.
- External database mode requires `config.databaseUrlSecret.name` and uses that Secret instead.
- Chart docs state that bundled PostgreSQL is for local evaluation, not production.

### 3. Database Migration Job

Add a Helm-managed migration path so a fresh install can initialize the database.

Expected work:

- Add a migration Job template that runs the existing migration command or migration-capable API image entrypoint.
- Add values to enable or disable the migration Job.
- Configure the Job to read the same database URL Secret as the API and worker.
- Add Helm hook annotations only if they produce predictable install and upgrade behavior.
- Document how to rerun migrations after failed local installs.

Acceptance criteria:

- A fresh bundled PostgreSQL install can create the schema without manual SQL commands.
- Migration Job output is visible through normal Kubernetes Job logs.
- The Job does not block `helm template` or `helm lint`.
- Docs name the exact `kubectl logs job/...` command pattern for troubleshooting migrations.

### 4. Bundled MinIO Templates

Add chart-managed MinIO for local alpha object storage.

Expected work:

- Render a MinIO Secret with root user and password values.
- Render a MinIO Service.
- Render a MinIO StatefulSet with persistence settings.
- Render a bucket initialization Job for the configured Capsulet object bucket.
- Wire API and worker S3 settings to the bundled MinIO endpoint and credential Secret when `minio.mode: bundled`.
- Preserve the existing S3-compatible external storage path.

Acceptance criteria:

- Default chart output includes MinIO Secret, Service, StatefulSet, and bucket initialization Job.
- API and worker render `CAPSULET_OBJECT_STORAGE_MODE=s3` for bundled MinIO installs.
- API and worker receive MinIO credentials from the bundled Secret.
- External object storage mode renders no bundled MinIO resources and still uses configured endpoint, bucket, region, path style, and credentials Secret.

### 5. Component Readiness And Install Ergonomics

Make the default chart shape understandable during local install.

Expected work:

- Add readiness probes where the API, dashboard, PostgreSQL, and MinIO can support them safely.
- Keep liveness probes conservative to avoid restart loops during first install.
- Add chart notes with local port-forward commands for the dashboard and API.
- Ensure component services and environment variables use stable in-cluster names.
- Preserve security defaults from earlier sprints unless a dependency image requires an explicit exception.

Acceptance criteria:

- Rendered resources expose clear service names for API, dashboard, PostgreSQL, and MinIO.
- `helm install` output tells users how to reach the dashboard locally.
- Readiness checks do not require external internet access.
- Security exceptions for bundled dependencies are documented when needed.

### 6. Documentation

Update public-alpha installation and Helm values docs.

Expected work:

- Update `docs/installation.md` with a local Kubernetes bundled-dependency install path.
- Update `docs/helm-values.md` with PostgreSQL and MinIO value tables/examples.
- Update `docs/troubleshooting.md` with PostgreSQL readiness, migration, MinIO bucket, and object storage credential failures.
- Add a short production note recommending external PostgreSQL and external S3-compatible storage outside local evaluation.
- Keep dashboard usage docs aligned with the bundled chart defaults.

Acceptance criteria:

- A contributor can follow docs from a clean local cluster to dashboard access.
- Docs include the values needed to switch both dependencies to external mode.
- Troubleshooting includes commands for `kubectl get pods`, `kubectl describe pod`, and `kubectl logs` for migration and bucket jobs.
- Docs explicitly state that bundled dependencies are alpha evaluation defaults.

### 7. Chart And Smoke Verification

Add repeatable checks for the new chart path.

Expected work:

- Keep `helm lint charts/capsulet` passing.
- Add a default `helm template` smoke check.
- Add an external-dependency `helm template` smoke check.
- Run Rust workspace checks to ensure environment variable changes did not break API/worker startup.
- Run dashboard lint/build checks if chart values or dashboard env behavior changes.
- Document a local kind or minikube smoke that installs the chart and submits a script.

Acceptance criteria:

- Default and external-mode Helm renders are both valid.
- Rust tests and clippy pass.
- Dashboard build still passes if dashboard configuration changes.
- Local smoke covers install, migration, bundled MinIO bucket creation, script submission, worker execution, run detail, logs, and artifact download.

## Stretch Scope

Only do these after committed scope is complete:

- Add optional NetworkPolicy templates for local chart components.
- Add chart README generation.
- Add `helm test` coverage that checks API and dashboard services.
- Add optional PVC size guidance for local MinIO and PostgreSQL.
- Add a docs-only upgrade note for dependency mode transitions.

## Explicit Non-Goals

- no release automation or GHCR publishing
- no GitHub Pages Helm repository
- no OCI Helm chart publishing
- no authentication or RBAC for dashboard/API users
- no Prometheus metrics or ServiceMonitor implementation
- no production backup/restore story
- no high-availability PostgreSQL or MinIO
- no managed cloud database provisioning
- no object retention cleanup
- no workflow engine UI

## Definition of Done

Sprint 007 is done when:

- chart defaults render bundled PostgreSQL and bundled MinIO for local evaluation
- chart can render external PostgreSQL and external S3-compatible storage without bundled dependency resources
- API and worker receive the correct database URL and object storage credentials in both modes
- a migration Job initializes a fresh bundled PostgreSQL database
- a bucket initialization Job prepares bundled MinIO for Capsulet artifacts
- installation, Helm values, and troubleshooting docs explain bundled and external dependency paths
- Helm lint and template checks pass for default and external modes
- Rust workspace checks still pass
- dashboard checks still pass when dashboard configuration is touched
- a local Kubernetes smoke demonstrates install, submit, run, inspect logs, and download artifact

## Suggested Work Order

1. Add dependency mode values and schema coverage.
2. Add bundled PostgreSQL Secret, Service, StatefulSet, and API/worker wiring.
3. Add the database migration Job.
4. Add bundled MinIO Secret, Service, StatefulSet, bucket Job, and API/worker S3 wiring.
5. Add readiness probes and Helm install notes.
6. Add default and external-mode Helm render checks.
7. Update installation, Helm values, and troubleshooting docs.
8. Run Helm, Rust, dashboard, and local Kubernetes smoke checks.

## Sprint Review Checklist

- Can a new evaluator install Capsulet into a local cluster without starting Docker Compose services by hand?
- Does the default chart install have an initialized database?
- Does bundled MinIO receive and serve script bundles, large logs, and artifacts?
- Can the dashboard submit a script and download an artifact after chart install?
- Is switching to external PostgreSQL and external S3-compatible storage clear and tested at the template level?
- Are bundled dependency limitations documented honestly?
- Did Sprint 007 introduce any security exceptions that need a future hardening task?

## Sprint 008 Preview

Sprint 008 should choose one of these paths based on Sprint 007 results:

- release automation for GHCR images, Helm chart packaging, and generated release notes
- observability metrics for queue depth, worker outcomes, retries, storage operations, and Kubernetes Job failures
- security hardening foundations such as image allowlists, network policy presets, and pod security documentation
- chart maturity work such as chart README generation, `helm test`, and install/upgrade validation
