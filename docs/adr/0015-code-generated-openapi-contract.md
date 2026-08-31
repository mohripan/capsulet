# ADR 0015: Code-Generated OpenAPI Contract

Status: Accepted

## Context

The handwritten OpenAPI file diverged from the Axum router: it omitted runtime paths and lacked
operation IDs, parameters, request bodies, and reusable response schemas. A sampled path check
could remain green while most of the public API changed. Handwritten dashboard and Python clients
therefore had no complete contract to verify against.

## Decision

Rust route declarations, endpoint metadata, and wire models are the authority for runtime HTTP
methods, paths, authorization metadata, and schemas. They generate OpenAPI 3.1 through a
contract-aware router. The generated document is served at `/openapi.json` and checked into
`crates/api/openapi.json` in deterministic canonical order.

CI compares the complete runtime and specification method/path sets, rejects any public `/v1`
route that bypasses contract-aware declaration, checks stable unique `operationId` values and
schema names, and fails when the checked-in artifact differs from generated output. Internal raw
routes require an explicit allowlist.

OpenAPI describes the running API only. Planned endpoints do not appear until they are registered
runtime behavior. Endpoint stability and authorization/project-context requirements are published
as extensions derived from the same endpoint metadata used by middleware.

## Consequences

- API changes require route metadata, wire schemas, tests, and regenerated OpenAPI in one change.
- `operationId` and schema names become compatibility-sensitive inputs to generated transports.
- Narrative API docs may explain use cases but may not define conflicting wire behavior.
- The checked-in artifact supports review, client generation, release comparison, and offline use
  without becoming a second authority.
