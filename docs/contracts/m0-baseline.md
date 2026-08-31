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

No implemented capability or guarantee is accepted without executable test evidence.

## Corrected M0 inventory after M1 Task 1

The corrected inventory contains 20 claims across 57 discovered public surfaces. The entry review
showed that the existing evidence for `CAP-JOB-001` and `CAP-OPENAPI-001` proved only part of each
statement, so both remain experimental until their complete behavior is executable. The two
implemented capability statements were narrowed to the behavior their selected tests execute.
M0 remains open until the typed route and wire-model remediation in M1 Task 2 passes.

| Dimension | Value | Count |
| --- | --- | ---: |
| Kind | capability | 12 |
| Kind | compatibility | 3 |
| Kind | limitation | 4 |
| Kind | positioning | 1 |
| Maturity | implemented | 9 |
| Maturity | experimental | 10 |
| Maturity | planned | 1 |

The experimental claims are `CAP-AGENT-001`, `CAP-MEMORY-001`, `CAP-AUTOMATION-001`,
`CAP-IAM-001`, `CAP-PERSISTENCE-001`, `CAP-DASHBOARD-001`, `CAP-OBSERVABILITY-001`, and
`CAP-HELM-001`, `CAP-JOB-001`, and `CAP-OPENAPI-001`. The planned claim is `CAP-PRODUCT-001`.
The explicit limitations are
`CAP-CORRECTNESS-002`, `CAP-AGENT-002`, `CAP-DASHBOARD-002`, and `CAP-SECURITY-001`.

## Completed M0 contract measurements

- One endpoint registry describes all 116 runtime operations across 90 paths, including method,
  path, operation ID, stability, scope, project context, authentication, parameters, request and
  response bodies, status codes, and content types.
- The generated OpenAPI 3.1 document contains the same 90 paths and 116 operations, 145 reusable
  schemas, and no runtime paths missing from the document.
- The Python client maps its seven public transport operations to the generated contract.
- The dashboard maps 68 explicit client operations to the generated contract.
- The product-claim gate discovers public surfaces from reviewed include/exclude globs, validates
  exact registry coverage, maps implemented assertions to executable evidence, checks each
  command/selector against the collected-test manifest, verifies the generated claim table, and
  rejects prohibited unqualified wording.
- The aggregate contract gate is wired into both GitHub Actions workflows and the pull-request
  checklist records product, API, authorization, lifecycle, migration, stability, client, docs,
  and ADR impact.

## Verification record

The following checks passed from this checkout on 2026-08-31:

- `cargo fmt --all -- --check`
- locked kernel, core, and API test suites (17 kernel tests, 66 core tests, and 87 API tests)
- `cargo run -p capsulet-api --bin export-openapi --locked -- --check`
- `powershell -File scripts/check-contracts.ps1` using the local Windows PowerShell 5.1 fallback;
  CI runs the same script with PowerShell 7 (20 claims, 57 surfaces, 90 OpenAPI paths, 116
  operations, eight Python client tests, and ten dashboard contract tests)
- PowerShell 7.5 schema validation and all 19 negative/positive product-contract fixtures in the
  pinned `mcr.microsoft.com/powershell:7.5-alpine-3.20` container
- dashboard TypeScript checking and production build (25 routes)

The exact executable evidence commands collected by the Task 1 gate are:

- `cargo test -p capsulet-kernel --locked`
- `cargo test -p capsulet-core --test typed_hypergraph --locked`
- `cargo test -p capsulet-application --test agents --locked`
- `cargo test -p capsulet-core --test memory_core --locked`
- `cargo test -p capsulet-worker --locked`
- `cargo test -p capsulet-api --locked creates_and_returns_workflow_dag_dependencies`
- `cargo test -p capsulet-api --locked verifies_timestamp_bound_hmac_and_rejects_tampering`
- `cargo test -p capsulet-api --locked authenticates_configured_roles_without_exposing_tokens`
- `npm test --prefix dashboard`
- `python -m unittest discover -s sdk/python/tests -v`
- `cargo test -p capsulet-application --test execution_contracts --locked`
- `cargo test -p capsulet-api --test openapi_contract --locked`
- `pwsh ./scripts/tests/check-product-claims.ps1`
- `docker compose config --quiet`
- `scripts/compose-smoke.ps1 -KeepExistingQueue -TimeoutSeconds 300` with
  `CAPSULET_POSTGRES_HOST_PORT=56432` (all application services healthy; served OpenAPI 90
  paths/116 operations; run `run_1788159544642` succeeded with logs)
- Helm lint and render with Helm 3.18.6
- offline Markdown link validation (74 links checked, no errors)
- non-mutating inspection of the existing `kind-kind` cluster (Kubernetes v1.35.0; control-plane
  node `Ready`)

The default Compose PostgreSQL host port `55432` fell inside a Windows reserved-port range in this
environment. `compose.yaml` now permits `CAPSULET_POSTGRES_HOST_PORT` to select a free host port;
internal service addressing is unchanged.

## M1 follow-ups

M0 deliberately does not promote Capsulet to public alpha. M1 must make a PostgreSQL-backed
clean-checkout suite mandatory and non-skippable, resolve remaining scope and ownership
inconsistencies, move client transports toward generation, add dashboard lint and dependency-audit
gates, and document environment-specific full-workspace/database verification. A dedicated agent
worker is not present; the current queued agent surface remains orchestration metadata and must not
be described as autonomous agent execution.
