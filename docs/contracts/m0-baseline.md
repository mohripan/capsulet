# M0 Contract Baseline

This report records the repository state measured before the M0 contract work and the first honest
inventory of public claims. Baseline counts describe the pre-M0 tree; they are not targets or
permanent assertions.

## Pre-M0 observations

- The runtime registered 90 distinct literal HTTP paths.
- The handwritten OpenAPI document listed 66 paths and omitted 24 runtime paths.
- Its 89 documented operations had no `operationId`, request-body, or parameter definitions and
  shared only one reusable schema (`Error`).
- `scripts/check-openapi.ps1` checked eight selected paths rather than runtime/spec set equality.
- Public material variously called Capsulet a memory platform, automation platform, workflow SDK,
  or job runner.
- The kernel exposed three verdicts while the approved platform model additionally required
  `unverified` outside the kernel.
- The dashboard and Python clients were handwritten and had no complete OpenAPI conformance gate.

## First claim inventory

The first inventory contains 19 claims across 47 public surfaces.

| Dimension | Value | Count |
| --- | --- | ---: |
| Kind | capability | 11 |
| Kind | compatibility | 2 |
| Kind | limitation | 5 |
| Kind | positioning | 1 |
| Maturity | implemented | 10 |
| Maturity | experimental | 8 |
| Maturity | planned | 1 |

Area totals are: Automations 1; Correctness kernel 2; Dashboard and SDK 3; Governed memory 1;
Graphs and agents 3; Helm and self-hosting 1; HTTP API 1; Identity and IAM 1; Jobs and runners 1;
Observability and operations 1; Persistence and storage 1; Product identity 1; Security and
isolation 1; and Workflow compatibility 1.

Eight claims intentionally lack executable evidence:

- `CAP-PRODUCT-001` is target positioning supported by the approved constitution, not a runtime
  capability.
- `CAP-CORRECTNESS-002`, `CAP-AGENT-002`, `CAP-DASHBOARD-002`, `CAP-SECURITY-001`, and
  `CAP-OPENAPI-001` are limitations supported by source inspection.
- `CAP-PERSISTENCE-001` and `CAP-HELM-001` remain experimental because M1 must make clean-checkout
  database and installation verification mandatory and non-skippable.

No implemented capability or guarantee is accepted without executable test evidence. Later M0
tasks update this report with generated OpenAPI, client-conformance, and final gate counts.
