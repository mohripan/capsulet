# ADR 0005: Object Storage for Bundles, Logs, and Artifacts

Status: Accepted

## Context

Capsulet will handle script bundles, input payloads, logs, output payloads, and artifacts. These can grow large and should not turn PostgreSQL into a blob store.

## Decision

Store script bundles, log chunks, input payloads, output payloads, and artifacts in object storage.

PostgreSQL stores metadata and references:

- object keys
- checksums
- content type
- size
- retention metadata
- job and attempt status

## Consequences

- The database remains focused on durable metadata and state transitions.
- Object storage becomes a required production dependency.
- The Helm chart must support object storage configuration.
- Retention cleanup must delete object storage data and update PostgreSQL metadata idempotently.
