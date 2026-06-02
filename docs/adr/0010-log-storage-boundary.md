# ADR 0010: Log Storage Boundary

Status: Accepted

## Context

Sprint 003 needs users to retrieve logs from the first Kubernetes Job runner path. The target architecture prefers object storage for script bundles, large logs, and artifacts because those payloads can grow beyond what belongs in PostgreSQL.

However, adding object storage in the same sprint as Kubernetes execution would expand the slice too much. The project still needs a log API and CLI surface now, while preserving a clean path to object storage later.

## Decision

Add a generic job run log repository boundary in `capsulet-core`.

The Sprint 003 implementation stores bounded logs in PostgreSQL through `capsulet-postgres`. Worker, API, and CLI code should depend on the log repository behavior, not on PostgreSQL-specific log assumptions.

Object storage remains the preferred backend for large logs and long-term artifact storage. A later sprint should add an object-storage-backed implementation and move large log bodies there while keeping PostgreSQL as metadata and index storage.

## Consequences

- Sprint 003 can expose logs without adding MinIO or S3 integration.
- PostgreSQL log storage must stay bounded and documented as temporary.
- The API and CLI log surface can remain stable when the backend changes.
- Future object storage work has a clear integration point instead of requiring API or worker rewrites.
