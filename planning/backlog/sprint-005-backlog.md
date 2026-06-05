# Sprint 005 Backlog

This is the working backlog for Sprint 005: Object Storage and Artifacts.

Status legend:

- `todo`: not started
- `doing`: in progress
- `blocked`: waiting on a decision or prerequisite
- `done`: complete

## Object Storage Boundary

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S5-STORAGE-001 | done | Add object storage trait | API, worker, and runner can use put/get/head behavior without depending directly on an S3 SDK |
| S5-STORAGE-002 | done | Add storage fake for tests | Unit and integration tests can exercise storage behavior without MinIO |
| S5-STORAGE-003 | done | Add S3-compatible adapter | MinIO and external S3-compatible endpoints can be configured |
| S5-STORAGE-004 | done | Add object storage config parsing | filesystem, S3 endpoint, bucket, region, credentials, and path-style settings are wired |
| S5-STORAGE-005 | done | Add deterministic storage key helpers | bundle, log, and artifact keys are run-scoped and Kubernetes-safe |
| S5-STORAGE-006 | done | Add storage error mapping | API and worker logs include actionable storage failure context |

## Artifact Metadata Persistence

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S5-ARTIFACT-DB-001 | done | Add artifact metadata migration | PostgreSQL stores run ID, attempt ID, artifact name, object key, content type, size, checksum when available, and created timestamp |
| S5-ARTIFACT-DB-002 | done | Add repository create method | Worker can persist an uploaded artifact record |
| S5-ARTIFACT-DB-003 | done | Add repository list-by-run method | API can list artifacts for one run without object storage listing |
| S5-ARTIFACT-DB-004 | done | Add repository fetch-by-run method | API download path cannot read artifacts owned by another run |
| S5-ARTIFACT-DB-005 | done | Add artifact repository tests | API/worker and database-backed smoke cover the path; dedicated Postgres artifact repository tests cover save/list/find isolation |

## Script Bundles

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S5-BUNDLE-001 | done | Define single-file Python bundle shape | Sprint 005 has a documented bundle contract for Python jobs |
| S5-BUNDLE-002 | done | Store submitted script bundle in object storage | Job submission writes script content to the configured bucket |
| S5-BUNDLE-003 | done | Persist bundle object key | Worker can resolve the object key for a run's script bundle |
| S5-BUNDLE-004 | done | Materialize bundle in Kubernetes Job | Runner pod executes script content loaded from object storage |
| S5-BUNDLE-005 | done | Keep seeded examples working | hello, sleep, fail, timeout, and artifact examples still run through direct seeded commands |
| S5-BUNDLE-006 | done | Add missing-bundle failure coverage | Missing object storage bundles fail with a clear reason |

## Large Log Offload

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S5-LOG-001 | done | Define inline and object log limits | Limits are explicit in constants and documented |
| S5-LOG-002 | done | Preserve small-log API behavior | Existing `GET /v1/jobs/runs/{id}/logs` and `capsulet logs` still work for small logs |
| S5-LOG-003 | done | Upload large logs to object storage | Large captured logs are stored under a run-scoped object key |
| S5-LOG-004 | done | Persist large-log reference | API indicates when additional object-backed log data exists |
| S5-LOG-005 | done | Add large-log tests | small log and large log behavior are covered |
| S5-LOG-006 | done | Add large-log smoke fixture | Local smoke can produce a deterministic object-backed log |

## Artifact Collection

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S5-COLLECT-001 | done | Define artifact output directory | Jobs have a documented directory such as `/capsulet/artifacts` for published files |
| S5-COLLECT-002 | done | Configure Kubernetes pod artifact path | Runner jobs can expose files for post-completion collection |
| S5-COLLECT-003 | done | Collect output files after completion | Kubernetes runner captures artifact file payloads before job completion handling returns to the worker |
| S5-COLLECT-004 | done | Upload artifacts to object storage | Stub and Kubernetes runner artifacts are written to run-scoped object keys |
| S5-COLLECT-005 | done | Persist artifact metadata after upload | Uploaded files appear in the artifact metadata table |
| S5-COLLECT-006 | done | Normalize artifact names | Artifact names cannot escape the intended namespace |
| S5-COLLECT-007 | done | Add artifact-producing example job | Local smoke can create and retrieve a predictable artifact |

## Artifact API and CLI

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S5-API-001 | done | Add artifact list endpoint | `GET /v1/jobs/runs/{id}/artifacts` returns metadata JSON |
| S5-API-002 | done | Add artifact download endpoint | `GET /v1/jobs/runs/{id}/artifacts/{artifact_id}` returns artifact bytes |
| S5-API-003 | done | Add artifact API tests | list, download, and missing artifact behavior are covered |
| S5-CLI-001 | done | Add artifact list command | `capsulet artifacts list <run-id>` prints artifact metadata clearly |
| S5-CLI-002 | done | Add artifact download command | `capsulet artifacts download <run-id> <artifact-id> --output <path>` writes the artifact file |
| S5-CLI-003 | done | Add CLI artifact tests | parsing, output formatting, and download output path behavior are covered |

## Helm and Configuration

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S5-HELM-001 | done | Add object storage values | Chart values include filesystem and S3-shaped configuration |
| S5-HELM-002 | done | Add object storage Secret template | Credentials can be loaded from an existing Kubernetes Secret |
| S5-HELM-003 | done | Pass storage config to API and worker | Workloads receive endpoint, bucket, region, path-style, path, and credential references |
| S5-HELM-004 | done | Add values schema coverage | `values.schema.json` validates object storage settings |
| S5-HELM-005 | done | Add local MinIO configuration | Compose includes MinIO and application code can use the S3 adapter |
| S5-HELM-006 | done | Keep Helm checks passing | `helm lint` and `helm template` pass with object storage values |

## Documentation

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S5-DOC-001 | done | Update architecture docs | Object storage responsibilities for bundles, logs, and artifacts are documented |
| S5-DOC-002 | done | Update API docs | Artifact endpoints and log offload fields are documented |
| S5-DOC-003 | done | Update CLI docs | Artifact list/download commands are documented |
| S5-DOC-004 | done | Update local Kubernetes runner guide | MinIO setup, script bundle execution, large logs, and artifact download smoke are documented |
| S5-DOC-005 | done | Add object storage troubleshooting | Credential, bucket, endpoint, and artifact collection failures are covered |
| S5-DOC-006 | done | Document Sprint 005 limitations | Retention cleanup, dashboard artifact browser, and multi-file bundles are explicitly deferred |

## Quality

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S5-QA-001 | done | Keep formatting passing | `cargo fmt --check` passes |
| S5-QA-002 | done | Keep clippy passing | `cargo clippy --workspace --all-targets -- -D warnings` passes |
| S5-QA-003 | done | Keep workspace tests passing | `cargo test --workspace` passes |
| S5-QA-004 | done | Keep Helm checks passing | `helm lint` and `helm template` pass |
| S5-QA-005 | done | Complete local storage smoke | Docker-backed MinIO smoke covers script bundle storage, large-log offload, artifact upload, list, and download |

## Sprint Risks

- Object storage can spread across API, worker, runner, and chart code. Keep one small storage boundary and reuse it everywhere.
- Artifact collection depends on pod lifecycle timing. Collect files before cleanup and document any cleanup ordering assumptions.
- Large-log behavior can become streaming. Keep Sprint 005 to captured logs plus object-backed references.
- Bundled MinIO can complicate production values. Keep local bundled dependency and external S3 settings clearly separated.
- Script bundles can expand into archive packaging. Start with one Python file and defer multi-file bundles.

## Completed Notes

- Added `capsulet-storage` with an object storage trait, filesystem and S3-compatible adapters, and run-scoped key helpers.
- Added `JobArtifact` metadata and `ArtifactObjectKind` to core.
- Added `job_artifacts` migration and PostgreSQL create/list/fetch methods.
- Added API endpoints for artifact list and download.
- Added CLI artifact list and download commands.
- Added worker persistence for artifacts returned by the runner.
- Added a stub-runner artifact mode for local end-to-end verification.
- Added object storage settings to Helm values/schema/templates and MinIO to `compose.yaml`.
- Completed Docker-backed MinIO smoke: submit a script-backed run, materialize the stored bundle, offload a large log, upload a runner artifact, list artifacts, download artifacts, and verify bucket contents.

## Remaining Notes

- Retention cleanup for object storage remains deferred.
- Dashboard artifact views remain deferred.
- Multi-file script bundles remain deferred.
- Streaming log retrieval remains deferred.

## Sprint Exit

Move any unfinished `todo` items back into `planning/backlog/product-backlog.md` or into Sprint 006 planning.
