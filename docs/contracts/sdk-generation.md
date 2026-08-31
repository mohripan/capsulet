# SDK and Client Generation Policy

The checked OpenAPI 3.1 artifact is the only input authority for generated remote transports.
Stable unique `operationId` values determine method identity; stable reusable schema names determine
generated wire types. Runtime Rust declarations remain the upstream authority that generates the
artifact.

The generation flow is:

```text
Rust routes + wire models
  -> deterministic checked OpenAPI
  -> pinned generator version and configuration
  -> generated Python/TypeScript transport
  -> handwritten ergonomic authoring layer
```

Generated files are reproducible artifacts. Their generator version, configuration, input digest,
and product API version are pinned. CI regenerates or contract-tests them and fails on drift.
Generated output never becomes a second endpoint/schema authority and is not hand-edited.

Handwritten layers may provide decorators, builders, retries, pagination helpers, higher-level
errors, and domain-friendly types. They wrap generated operations and explicitly map to their
`operationId`; they may not invent a different HTTP method, path, project-context header, wire
field, or response assumption. Client-only convenience models must not masquerade as server wire
schemas.

Until generated transports land, the current Python and dashboard clients remain experimental and
publish explicit operation maps. Their tests compare every declared method, path template,
project-context requirement, and response expectation to OpenAPI. Adding a client call without a
matching operation is a contract failure.

M0 validates the generation input and handwritten boundaries only; it does not publish generated
packages. The current maps are `CLIENT_OPERATIONS` in the Python client and
`DASHBOARD_OPERATIONS` in the dashboard client. `scripts/check-sdk-contracts.ps1` runs both
conformance suites and the dashboard type checker.

SDK compatibility metadata states the supported product/API release range independently from the
package's ecosystem version. Stable generated symbols follow the deprecation window in the
stability policy; ergonomic aliases may outlive a wire deprecation but must eventually call the
replacement operation.
