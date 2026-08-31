# ADR 0016: Public Contract Stability and Versioning

Status: Accepted

## Context

Capsulet publishes several coupled surfaces: HTTP API, SDKs, future workflow IR and certificate
formats, domain packs, database migrations, images, Helm chart, CLI, and dashboard. Independent
unlabeled change would make self-hosted upgrades and generated clients unsafe even before 1.0.

## Decision

Every public surface is labeled `stable`, `experimental`, or `internal`. Before public alpha all
surfaces default to experimental unless explicitly promoted. Stable alpha removals or incompatible
changes receive at least one published alpha minor release of deprecation notice, release notes,
and a migration path. An urgent security removal may bypass the window only with a published
security advisory and replacement guidance.

One release version is propagated to images, chart `appVersion`, CLI output, dashboard build
metadata, SDK compatibility metadata, and served API metadata. Component package versions may
differ only where the release policy documents why; they never obscure the compatible product
release.

Persisted definitions and future IR, certificate, and domain-pack payloads carry explicit schema
versions. Readers accept every version promised by the support window and either migrate or reject
unknown versions explicitly. Database migrations are forward-only and use expand/migrate/contract
sequencing with pre-upgrade backup and restore verification. Rollback normally restores the prior
application and database backup; down SQL is not assumed able to reverse transformed data.

Generated SDK transports derive only from the checked OpenAPI contract. Handwritten ergonomic
authoring layers wrap generated transports and may not redefine endpoints or wire schemas.

## Consequences

- Promotion to stable is explicit and adds compatibility obligations.
- Releases coordinate version metadata across deployable and client artifacts.
- Deprecation, compatibility readers, migration tests, backup, and restore are release work rather
  than optional documentation.
- Experimental surfaces may change quickly, but changes remain intentional, tested, and visible.
