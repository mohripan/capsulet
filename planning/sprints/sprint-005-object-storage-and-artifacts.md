# Sprint 005: Object Storage and Artifacts

## Sprint Goal

Move Capsulet beyond bounded PostgreSQL logs by adding an object-storage-backed data plane for script bundles, large logs, and job artifacts.

By the end of this sprint, Capsulet should support this local evaluation flow:

1. Start local PostgreSQL and an S3-compatible object store such as MinIO.
2. Submit a Python job whose script bundle is stored in object storage.
3. Run the job through the Kubernetes runner.
4. Capture bounded inline logs and upload larger log content to object storage when needed.
5. Upload at least one job artifact from the runner pod.
6. List artifacts through the API and CLI.
7. Download an artifact through the API and CLI.
8. Install the chart with either bundled MinIO for local evaluation or external S3-compatible settings.

This sprint should create the storage boundary needed for the public alpha without building the full dashboard artifact browser or retention system.

## Sprint Length

Recommended: 1 week.

## Sprint Theme

Durable outputs, narrow storage contract.

The goal is not a complete data platform. The goal is a small, testable object storage layer that lets jobs carry code in and return files out while keeping PostgreSQL focused on metadata and state.

## Current Context

Sprint 004 completed:

- cancellation through API, CLI, and worker/Kubernetes runner paths
- timeout classification as `timed_out`
- fixed-delay retry policy
- expired lease recovery
- Kubernetes Job cleanup policy
- local smoke coverage for success, failure, timeout, retry, cancellation, and cleanup

Phase 1 is now practically complete for single-job execution. Phase 2 introduces artifact upload, S3-compatible storage, bundled MinIO for local installs, dashboard detail views, and public-alpha packaging. Sprint 005 should land the object storage foundation first because dashboard and release work need stable storage references to display and download.

## Committed Scope

### 1. Object Storage Boundary

Add a storage abstraction that can write, read, and check objects without leaking S3-specific details into core job logic.

Expected work:

- Add a small object storage trait in the appropriate crate boundary.
- Implement an in-memory or filesystem-backed fake for tests.
- Implement an S3-compatible adapter for MinIO and external S3 endpoints.
- Keep storage keys deterministic and namespaced by run ID, attempt ID, and object kind.
- Ensure object storage errors carry enough context for API and worker logs.

Recommended implementation:

- Keep object storage interfaces out of Kubernetes-specific runner code.
- Use one shared storage client for script bundles, large logs, and artifacts.
- Prefer explicit object metadata records in PostgreSQL over deriving everything from bucket listings.

Acceptance criteria:

- Unit tests cover put, get, head/not-found, and error mapping through the storage boundary.
- S3 endpoint, bucket, region, access key, secret key, and path-style mode can be configured.
- Kubernetes runner and API code depend on the storage boundary, not directly on an S3 SDK.

### 2. Artifact Metadata Persistence

Persist artifact metadata in PostgreSQL so API and CLI can list outputs without scanning object storage.

Expected work:

- Add an artifact metadata table.
- Record run ID, attempt ID when available, artifact name, object key, content type, size, checksum if practical, and creation timestamp.
- Add repository methods to create, list, and fetch artifact records by run ID and artifact ID or name.
- Prevent one run from reading another run's artifacts through repository lookups.

Acceptance criteria:

- Migration applies cleanly from the Sprint 004 schema.
- Repository tests cover create, list by run, fetch by run and artifact, and missing artifact behavior.
- Artifact metadata is stable enough for API, CLI, and dashboard use in later sprints.

### 3. Script Bundle Storage

Store submitted script bundles in object storage instead of relying only on seeded static definitions.

Expected work:

- Define the first script bundle shape for Python jobs.
- Store submitted script content or a local example bundle into object storage.
- Persist the bundle object key on the job definition or run input record selected by the current domain model.
- Make the Kubernetes runner materialize the bundle into the runner pod.
- Keep seeded examples working by migrating them to the same bundle path where reasonable.

Recommended implementation:

- Start with a single-file Python script bundle before supporting multi-file archives.
- Use an init container or mounted projected content only if it fits the existing runner design; otherwise download the bundle inside the job container through a small bootstrap command.
- Keep arbitrary remote Git or container command jobs out of scope.

Acceptance criteria:

- A submitted Python script runs from object storage in a local Kubernetes smoke test.
- Missing bundle objects fail with a clear run failure reason.
- Seeded `job_hello_python`, failure, timeout, and sleep examples still run.
- Docs explain the Sprint 005 bundle format and limitations.

### 4. Large Log Offload

Keep Sprint 003 bounded PostgreSQL logs for quick inspection, but offload larger logs to object storage.

Expected work:

- Define the inline log limit and object log limit.
- Store the first bounded log segment in PostgreSQL for existing `GET /logs` behavior.
- Upload full or truncated large logs to object storage when the captured log exceeds the inline limit.
- Persist log object metadata or reuse artifact metadata with a distinct object kind.
- Include API response fields that tell clients whether more log data is available from object storage.

Acceptance criteria:

- Existing logs API and CLI behavior still works for small logs.
- A large-log job stores inline preview plus an object storage reference.
- Tests cover small logs, large logs, and missing object-log references.
- Documentation explains which log data is inline and which data is object-backed.

### 5. Artifact Collection From Runner Pods

Allow jobs to publish files from a known output directory.

Expected work:

- Define a container output directory such as `/capsulet/artifacts`.
- Configure the Kubernetes Job pod so artifacts can be collected after completion.
- Upload discovered files to object storage.
- Persist artifact metadata for each collected file.
- Classify artifact upload failure behavior clearly.

Recommended implementation:

- Start with files collected from the completed pod before cleanup.
- Set a conservative artifact count and size limit.
- Treat artifact upload failures as worker errors for Sprint 005 unless the job itself already failed.

Acceptance criteria:

- A local example job writes an artifact and the worker uploads it.
- Artifact names are normalized and cannot escape the intended artifact namespace.
- Tests cover artifact key generation, metadata persistence, and upload failure handling.
- Kubernetes cleanup does not delete persisted artifact data.

### 6. Artifact API and CLI

Expose artifact listing and download through the existing user surfaces.

Expected endpoints and commands:

```text
GET /v1/jobs/runs/{id}/artifacts
GET /v1/jobs/runs/{id}/artifacts/{artifact_id}
capsulet artifacts <run-id>
capsulet artifacts download <run-id> <artifact-id> --output <path>
```

Expected work:

- Add API routes for listing and downloading artifacts.
- Return metadata as JSON for list responses.
- Stream or return object bytes for download responses.
- Add CLI commands for listing and downloading.
- Print clear errors for missing runs, missing artifacts, and storage read failures.

Acceptance criteria:

- API tests cover list, download, missing run, missing artifact, and storage failure.
- CLI tests cover argument parsing and output path handling.
- Manual smoke can run an artifact-producing job, list artifacts, and download the file.

### 7. Helm MinIO and External S3 Configuration

Make object storage usable from local chart installs and configurable for production-shaped installs.

Expected work:

- Add optional bundled MinIO chart dependency or internal templates, following the current chart style.
- Add external S3-compatible configuration values.
- Add object storage Secret templates for credentials.
- Pass object storage settings to API and worker pods.
- Add `values.schema.json` coverage.
- Keep existing PostgreSQL and runner configuration intact.

Recommended implementation:

- Prefer bundled MinIO enabled for local evaluation values, not as a mandatory production dependency.
- Support path-style access for MinIO.
- Keep credential values in Kubernetes Secrets, not ConfigMaps.

Acceptance criteria:

- `helm lint charts/capsulet` passes.
- `helm template capsulet charts/capsulet` renders valid object storage configuration.
- Local install documentation includes bundled MinIO and external S3 examples.
- Chart defaults remain honest about what is local-evaluation versus production-oriented.

### 8. Documentation and Smoke Checklist

Update docs for the new storage-backed flows.

Expected work:

- Update architecture docs with object storage responsibilities.
- Update API docs with artifact endpoints and log offload fields.
- Update CLI docs with artifact commands.
- Update local Kubernetes runner guide with MinIO setup, script bundle submission, artifact retrieval, and large-log checks.
- Add troubleshooting notes for object storage connection, credential, bucket, and artifact collection failures.

Acceptance criteria:

- A contributor can run a local artifact smoke test from the docs.
- Docs call out that retention cleanup and dashboard artifact browsing are deferred.
- Known storage limits are documented with explicit values.

### 9. Quality and Regression Coverage

Preserve the Sprint 004 behavior while adding storage-backed execution.

Acceptance criteria:

- `cargo fmt --check` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes.
- `helm lint charts/capsulet` passes.
- `helm template capsulet charts/capsulet` passes with local object storage values.
- Manual minikube smoke records success, failure, timeout, retry, cancellation, cleanup, script bundle storage, large-log offload, artifact upload, and artifact download.

## Stretch Scope

Only do these after committed scope is complete:

- Dashboard job detail integration for artifacts and object-backed logs.
- Pre-signed artifact download URLs.
- Multi-file script bundle archives.
- Artifact checksum verification on download.
- Retention cleanup for object storage.
- Separate buckets or prefixes for bundles, logs, and artifacts.
- Object storage metrics.
- External S3 smoke test against a real cloud bucket.

## Explicit Non-Goals

- no workflow engine
- no automation triggers
- no dashboard artifact browser unless all committed scope is done
- no authentication or authorization
- no retention cleanup worker
- no content-addressed storage
- no exactly-once artifact upload guarantee
- no arbitrary Git repository import
- no WASM or non-Python runtime expansion
- no streaming logs
- no custom Kubernetes operator

## Definition of Done

Sprint 005 is done when:

- object storage configuration works for local MinIO and external S3-compatible endpoints
- script bundles can be stored in object storage and executed by the Kubernetes runner
- large logs can be offloaded while preserving the existing small-log API and CLI flow
- jobs can publish artifacts from a known output directory
- artifact metadata is persisted in PostgreSQL
- users can list and download artifacts through API and CLI
- Helm values, schema, templates, and docs cover object storage configuration
- existing Sprint 004 cancellation, timeout, retry, recovery, cleanup, and smoke flows still pass

## Suggested Work Order

1. Add the object storage trait and test fake.
2. Add the S3-compatible adapter and configuration model.
3. Add artifact metadata migration and repository tests.
4. Add storage key generation helpers for bundles, logs, and artifacts.
5. Add script bundle storage and runner materialization.
6. Run a local MinIO/Kubernetes script bundle smoke test.
7. Add large-log offload while preserving inline log behavior.
8. Add artifact collection from the runner pod output directory.
9. Add artifact list and download API endpoints.
10. Add CLI artifact list and download commands.
11. Add Helm values, Secret templates, schema, and MinIO/local values.
12. Update architecture, API, CLI, local Kubernetes, and troubleshooting docs.
13. Run fmt, clippy, workspace tests, helm lint/template, and the manual smoke checklist.

## Sprint Review Checklist

- Can a contributor submit and run a Python script whose bundle lives in object storage?
- Can a job publish an artifact and can the user download it through API and CLI?
- Does small-log behavior remain simple while large logs get object-backed references?
- Are object keys deterministic, namespaced, and safe?
- Are artifact metadata records sufficient for a future dashboard?
- Does the chart distinguish bundled MinIO from external S3 configuration clearly?
- Are storage failures understandable from API, CLI, and worker output?
- Did the implementation create any security or retention decisions that need ADRs?

## Sprint 006 Preview

Sprint 006 should choose one of these paths based on Sprint 005 results:

- dashboard integration with real run, cancel, log, and artifact APIs
- bundled PostgreSQL and MinIO chart maturity for public alpha installs
- observability metrics for queue depth, attempts, retries, storage, and worker outcomes
- release automation for GHCR images and Helm chart packaging
