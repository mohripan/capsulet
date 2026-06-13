# Sprint 007 Backlog

This is the working backlog for Sprint 007: Bundled Dependencies and Chart Alpha.

Status legend:

- `todo`: not started
- `doing`: in progress
- `blocked`: waiting on a decision or prerequisite
- `done`: complete

## Dependency Values

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S7-VALUES-001 | done | Add PostgreSQL dependency mode values | `postgresql.mode` supports `bundled` and `external` with local-alpha defaults |
| S7-VALUES-002 | done | Add PostgreSQL bundled settings | Image, auth, service, persistence, and resources are configurable |
| S7-VALUES-003 | done | Add MinIO dependency mode values | `minio.mode` supports `bundled` and `external` with local-alpha defaults |
| S7-VALUES-004 | done | Add MinIO bundled settings | Image, auth, service, bucket, persistence, and resources are configurable |
| S7-VALUES-005 | done | Add schema validation for dependency values | Invalid modes and malformed bundled settings fail Helm schema validation |
| S7-VALUES-006 | done | Preserve external dependency values | Existing database Secret and S3-compatible object storage settings still render for external mode |

## Bundled PostgreSQL

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S7-PG-001 | done | Add PostgreSQL Secret template | Bundled mode renders database, user, password, and `DATABASE_URL` Secret data |
| S7-PG-002 | done | Add PostgreSQL Service template | Bundled mode renders a stable in-cluster PostgreSQL service name |
| S7-PG-003 | done | Add PostgreSQL StatefulSet template | Bundled mode renders one PostgreSQL pod with configured security, resources, and storage |
| S7-PG-004 | done | Add PostgreSQL persistence controls | Persistence-enabled and throwaway local install modes render predictably |
| S7-PG-005 | done | Wire API database Secret for bundled mode | API reads `CAPSULET_DATABASE_URL` from the bundled PostgreSQL Secret |
| S7-PG-006 | done | Wire worker database Secret for bundled mode | Worker reads `CAPSULET_DATABASE_URL` from the bundled PostgreSQL Secret |
| S7-PG-007 | done | Validate external PostgreSQL rendering | External mode renders no PostgreSQL StatefulSet and uses `config.databaseUrlSecret` |

## Database Migrations

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S7-MIGRATE-001 | done | Add migration Job values | Chart values can enable or disable migration Job rendering |
| S7-MIGRATE-002 | done | Add migration Job template | Fresh installs can run database migrations against the configured database URL |
| S7-MIGRATE-003 | done | Add migration Job labels and restart policy | Job renders with common chart labels and a safe failure-visible restart policy |
| S7-MIGRATE-004 | done | Document migration troubleshooting | Docs explain how to inspect migration Job status and logs |

## Bundled MinIO

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S7-MINIO-001 | done | Add MinIO Secret template | Bundled mode renders root access key and secret key data |
| S7-MINIO-002 | done | Add MinIO Service template | Bundled mode renders a stable in-cluster MinIO endpoint |
| S7-MINIO-003 | done | Add MinIO StatefulSet template | Bundled mode renders one MinIO pod with configured security, resources, and storage |
| S7-MINIO-004 | done | Add MinIO persistence controls | Persistence-enabled and throwaway local install modes render predictably |
| S7-MINIO-005 | done | Add bucket initialization Job | Configured object bucket exists before API and worker need it |
| S7-MINIO-006 | done | Wire bundled object storage env | API and worker render S3 mode, bucket, endpoint, region, path style, and credential Secret for bundled MinIO |
| S7-MINIO-007 | done | Validate external object storage rendering | External mode renders no MinIO resources and uses configured external S3-compatible settings |

## Install Ergonomics

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S7-INSTALL-001 | done | Add API readiness probe review | API readiness is safe for first install and does not hide database failures |
| S7-INSTALL-002 | done | Add dashboard readiness probe review | Dashboard readiness reflects whether the web server is reachable |
| S7-INSTALL-003 | done | Add PostgreSQL readiness probe | PostgreSQL pod readiness reflects local database availability |
| S7-INSTALL-004 | done | Add MinIO readiness probe | MinIO pod readiness reflects local object storage availability |
| S7-INSTALL-005 | done | Add Helm install notes | `helm install` output includes dashboard and API port-forward commands |
| S7-INSTALL-006 | done | Confirm stable service names | API, dashboard, PostgreSQL, and MinIO services have predictable names in rendered manifests |

## Documentation

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S7-DOC-001 | done | Update installation docs | Local Kubernetes install uses bundled PostgreSQL and MinIO by default |
| S7-DOC-002 | done | Update Helm values docs | PostgreSQL and MinIO bundled/external values are documented with examples |
| S7-DOC-003 | done | Update troubleshooting docs | PostgreSQL, migration, MinIO, bucket, and object storage credential failures are covered |
| S7-DOC-004 | done | Document production dependency guidance | Docs recommend external PostgreSQL and S3-compatible storage outside local alpha evaluation |
| S7-DOC-005 | done | Confirm dashboard docs match chart defaults | Dashboard access and API URL docs align with bundled chart service names |

## Quality

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S7-QA-001 | done | Keep Helm lint passing | `helm lint charts/capsulet` passes |
| S7-QA-002 | done | Add default Helm template smoke | Default bundled dependency render is valid and includes expected resources |
| S7-QA-003 | done | Add external dependency Helm template smoke | External PostgreSQL and object storage render is valid and excludes bundled resources |
| S7-QA-004 | done | Keep Rust workspace tests passing | `cargo test --workspace` passes |
| S7-QA-005 | done | Keep Rust clippy passing | `cargo clippy --workspace --all-targets -- -D warnings` passes |
| S7-QA-006 | done | Keep dashboard checks passing | Dashboard lint/build pass if dashboard configuration or chart env behavior changes |
| S7-QA-007 | done | Complete local Kubernetes chart smoke | Install, migration, bucket init, submit, run, logs, artifact list, and artifact download are verified |

## Sprint Risks

- Bundling dependencies can accidentally look like a production recommendation. Keep docs explicit that bundled PostgreSQL and MinIO are local alpha defaults.
- Migration hooks can make Helm upgrades brittle. Prefer predictable Job behavior and document failure recovery.
- MinIO bucket creation can race API and worker startup. Make readiness and troubleshooting clear, and keep failure modes visible.
- Dependency images may not support the same strict security context as Capsulet components. Document every exception and keep it narrow.
- Chart complexity can grow quickly. Keep Sprint 007 focused on PostgreSQL, MinIO, migration, docs, and smoke validation.

## Completion Notes

- Added explicit `postgresql.mode` and `minio.mode` values. Defaults are `bundled`; external mode preserves existing database Secret and S3-compatible object storage settings.
- Added bundled PostgreSQL Secret, Service, StatefulSet, persistence controls, readiness probe, and automatic `CAPSULET_DATABASE_URL` wiring.
- Added `CAPSULET_MIGRATE_ONLY=true` support to `capsulet-api`; the chart migration Job runs the API image, applies migrations, seeds examples when enabled, and exits.
- Added bundled MinIO Secret, Service, StatefulSet, persistence controls, readiness probe, automatic S3 config wiring, and bucket initialization Job.
- Added install notes with dashboard, API, migration, and MinIO console port-forward commands.
- Added external-mode Helm template smoke to CI.
- Documented bundled dependencies as local alpha defaults and external PostgreSQL/S3-compatible storage as the production-shaped path.
- Security context exception: bundled PostgreSQL and MinIO keep `readOnlyRootFilesystem: false` because stateful dependency images need writable runtime/data paths. They still run non-root with dropped capabilities and privilege escalation disabled.
- Verification completed: `helm lint charts/capsulet`, default `helm template`, external-mode `helm template`, invalid dependency mode schema failure, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, dashboard lint, dashboard build, Docker image builds, and minikube install smoke.
- Local Kubernetes smoke result: installed full bundled chart in minikube, migration Job completed, MinIO bucket Job completed, API/worker/dashboard/scheduler/evaluator rolled out, dashboard `/runs` returned 200, script run `run_1780727061083` succeeded with logs, bundle artifact downloaded from bundled MinIO, seeded artifact run `run_1780727152145` succeeded, and `report.txt` downloaded with `artifact from capsulet`.
- No Sprint 007 committed-scope items were moved to Sprint 008.

## Sprint Exit

Move any unfinished `todo` items back into `planning/backlog/product-backlog.md` or into Sprint 008 planning.
