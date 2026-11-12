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

- [ ] `cargo run -p capsulet-xtask --locked -- verify --profile fast` passes locally, or the gates
      this change touches (`verify --gate <name>`) do.
- [ ] `Required` is green. It passes only when every gate of the full profile ran and passed, no
      job was skipped or cancelled, and no run changed the worktree or left anything behind.
- [ ] A new check is a gate in `crates/xtask/src/verify/catalog.rs`, not a workflow step
      (`ci-contract` refuses the latter).
- [ ] The tests that show this change are listed below.

Verification notes:
