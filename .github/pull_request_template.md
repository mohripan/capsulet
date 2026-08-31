## Change summary

Describe the behavior and contract impact.

## Contract checklist

- [ ] Product claims: added/updated claim IDs, maturity, public-surface markers, and executable evidence where required; or not applicable.
- [ ] Public API: updated the endpoint registry, stable `operationId`, request/response schemas, parameters, status/content types, and regenerated OpenAPI; or not applicable.
- [ ] Authorization: reviewed authentication mode, required scope, project context, and resource ownership; or not applicable.
- [ ] Lifecycle/assurance: updated current enum mapping without collapsing execution status into an assurance verdict; or not applicable.
- [ ] Persistence: added a forward-only migration and documented backup/restore/rollback impact; or not applicable.
- [ ] Stability: assigned the correct experimental/stable label and followed the compatibility/deprecation policy; or not applicable.
- [ ] SDK/clients: updated explicit operation maps and conformance tests; or not applicable.
- [ ] Documentation: updated current public docs and marked superseded/historical material honestly; or not applicable.
- [ ] Decisions: added or updated an accepted ADR for a new normative public contract; or not applicable.

## Verification

- [ ] `pwsh ./scripts/check-contracts.ps1`
- [ ] Relevant Rust, Python, dashboard, Helm, Compose, or Kubernetes tests are listed below.

Verification notes:
