# Engineering Integrity Implementation Plan

> **For agentic workers:** Implement this plan task-by-task with test-first changes and one focused
> commit per task. M0 contract remediation is the entry gate for M1, not optional cleanup. Do not
> begin M2 IR work while any M1 exit criterion is red.

**Goal:** Complete M1 by making a clean checkout reproducibly verifiable, persistence tests
mandatory, authorization coherent, public contracts structurally complete, dependencies and build
artifacts auditable, and every released component traceable to one version and source revision.

**Architecture:** Rust route declarations and typed wire models are the sole HTTP-contract
authority. A small Rust `xtask` is the cross-platform verification orchestrator and all CI workflows
call it rather than reimplementing checks in YAML. PostgreSQL integration tests use isolated,
explicitly provisioned databases and fail when their required infrastructure is absent. Typed IAM
policy metadata connects route registration, runtime authorization, OpenAPI, and generated
authorization tests. Release metadata is declared once and checked against every package, image,
chart, and served version. CI builds an artifact once, then tests, scans, attests, and signs that
same digest.

**Tech Stack:** Rust 1.96, Axum 0.8, Utoipa 5.5, `utoipa-axum` 0.2, PostgreSQL, Docker Compose,
Helm, Kind, TypeScript/Vitest/Playwright, Python `unittest`, GitHub Actions, Trivy, cargo-deny,
secret scanning, Syft/SPDX or CycloneDX SBOMs, and Sigstore/Cosign attestations.

---

## Review-derived entry conditions

The M0 implementation is broad and its current focused tests pass, but M0 is not yet closed. M1
starts by correcting these contract failures:

- runtime Axum routes, endpoint metadata, and OpenAPI schemas are independently handwritten;
- parity is tested by parsing `http/internal.rs` source text rather than by sharing typed route
  declarations;
- several nested request/response schemas are generic objects or absent, and the service-account
  response schema rejects fields emitted by the real response;
- broad implemented claims cite evidence for only one part of the statement;
- the public-surface check validates a hard-coded list but does not discover new dashboard pages;
- dashboard/Python checks prove operation-name presence but not full request/response compatibility;
- project roles, scopes, and ownership behavior disagree on newer graph, reasoning, memory, and
  ingestion resources;
- PostgreSQL tests silently return when no database URL exists;
- dashboard lint and dependency audit are red;
- image names, dashboard Dockerfiles, and release versions have multiple authorities.

Do not alter the M0 constitution or lower its gate to make these findings pass. Correct the
implementation and update `docs/contracts/m0-baseline.md` only after Tasks 1 and 2 are green.

## Chosen approach

Use one executable verification graph with typed authorities.

- `cargo run -p capsulet-xtask --locked -- verify --profile full` is the supported clean-checkout
  command. The command reports prerequisite failures explicitly, provisions disposable test
  infrastructure, runs each required gate, and always attempts cleanup.
- `verify --profile fast` is an optional developer loop. It never represents the M1 release gate
  and must clearly list excluded checks.
- Runtime route construction is the source of truth for path/method/operation metadata. Rust wire
  types are the source of truth for schemas. Checked-in OpenAPI is deterministic generated output.
- IAM uses typed permissions and resource ownership rules. No authorization decision relies on
  unvalidated string prefixes or treats missing ownership as access.
- CI workflows may schedule and cache work, but they must call `capsulet-xtask` subcommands. Logic
  that determines pass/fail belongs in repository code, not duplicated workflow steps.
- The supported M1 Kubernetes smoke target is Kind. Broader Kubernetes matrices, upgrade/restore,
  high availability, and disaster recovery remain M6 work.

## Scope boundaries

- Preserve current endpoint URLs and response behavior unless a documented contract is already
  false or insecure. Use the M0 stability policy for any incompatible correction.
- Do not implement the M2 verified-computation IR, platform assurance verdict, or trust-typed
  values.
- Do not implement the M3 durable graph worker, loops, waits, compensation, or recovery semantics.
- Do not claim production readiness from passing M1. M1 establishes engineering integrity, not
  runtime correctness or operational maturity.
- Do not make network-dependent vulnerability databases part of the ordinary fast loop. The full
  local/CI gate uses pinned or explicitly refreshed scanner inputs and records their versions.
- Do not silently skip checks because Docker, PostgreSQL, Node, Python, Helm, or Kind is absent. The
  full verifier either provisions the dependency or fails with an actionable prerequisite message.

---

### Task 1: Repair public-claim evidence and surface discovery

**Files:**

- Modify: `docs/contracts/product-claims.schema.json`
- Modify: `docs/contracts/product-claims.json`
- Regenerate: `docs/contracts/product-claims.md`
- Create: `docs/contracts/public-surfaces.json`
- Create: `docs/contracts/public-surfaces.schema.json`
- Modify: `scripts/check-product-claims.ps1`
- Modify: `scripts/tests/check-product-claims.ps1`
- Add fixtures under: `scripts/fixtures/contracts/invalid-claims/`
- Modify claim markers in current public surfaces, including `dashboard/app/memory/ingestion/page.tsx`
- Modify only after the gate passes: `docs/contracts/m0-baseline.md`

- [ ] Add failing fixtures for a newly created public page that is absent from the registry, a
  compound implemented claim with partial evidence, an evidence selector that exists but is never
  executed, and an evidence command whose selected test does not exist.
- [ ] Define public-surface include/exclude globs for root docs, current product docs, package/chart
  metadata, SDK docs, examples, served API descriptions, and user-facing dashboard pages. Keep
  generated/build/vendor/history exclusions explicit and reviewed.
- [ ] Make the checker discover the current surface set from those globs and fail when any discovered
  surface is unregistered. Do not use a hard-coded expected-file list in tests.
- [ ] Split compound implemented claims such as job execution adapters, automation trigger kinds,
  governed-memory behavior, and IAM behavior into independently testable statements.
- [ ] Require every implemented capability/guarantee evidence record to declare the exact gate
  subcommand and selector that runs it. Validate evidence against the gate's collected-test output
  or a machine-readable test manifest rather than source-string presence alone.
- [ ] Downgrade statements without adequate executable evidence to `experimental` or `planned`;
  preserve them as visible work instead of weakening validation.
- [ ] Inventory and mark the memory-ingestion review copy and every other newly discovered surface.
- [ ] Regenerate the claims table deterministically and run negative fixtures plus the repository
  inventory check.
- [ ] Update the M0 completion report to state the corrected claim/surface totals and the exact
  evidence commands only after the enhanced checker passes.

**Task gate:**

```powershell
pwsh ./scripts/tests/check-product-claims.ps1
pwsh ./scripts/check-product-claims.ps1
git diff --check
```

### Task 2: Make runtime routes and Rust wire types the only HTTP authority

**Files:**

- Modify: `crates/api/src/http/internal.rs`
- Modify: `crates/api/src/models.rs`
- Modify wire types in: `crates/api/src/{automations,graphs,ingestion,memory,reasoning,webhooks}.rs`
- Replace: `crates/api/src/endpoint_contract.rs`
- Replace: `crates/api/src/openapi.rs`
- Modify: `crates/api/src/auth.rs`
- Modify: `crates/api/src/lib.rs`
- Modify: `crates/api/tests/openapi_contract.rs`
- Modify: `crates/api/src/bin/export-openapi.rs`
- Regenerate: `crates/api/openapi.json`
- Modify: `dashboard/tests/openapi-contract.test.ts`
- Modify: `sdk/python/tests/test_openapi_contract.py`
- Modify: `scripts/check-openapi.ps1`
- Modify: `scripts/check-sdk-contracts.ps1`
- Modify: `docs/contracts/m0-baseline.md`

- [ ] Add a failing regression test that serializes a real `CreateServiceAccountResponse` and
  validates it against the generated schema, including nullable `expires_at`, `revoked_at`, and
  `last_used_at` fields.
- [ ] Add failing tests for every request/response type currently represented as a generic object,
  including workflow steps/dependencies, automation triggers, compiled memory policies, relation
  and claim policies, retrieval policies, and local-text connector configuration.
- [ ] Add failing compile-time or structural tests proving that a public `/v1` route cannot be
  registered without stable operation ID, stability, authentication mode, typed required
  permission, project-context rule, request schema, response schema, and error contract.
- [ ] Build public routes through `utoipa_axum::router::OpenApiRouter` and annotated handlers. Merge
  explicitly internal routes through a small typed allowlist; fail when a new public raw Axum route
  bypasses contract registration.
- [ ] Derive `ToSchema`/`IntoParams` on the actual Serde wire types. Remove handwritten schema
  mirrors and the large string-keyed schema match functions.
- [ ] Replace unrestricted method, status, content-type, stability, scope, and schema strings with
  enums/newtypes whose invalid states are unrepresentable.
- [ ] Generate authorization policy and OpenAPI extensions from the typed endpoint declaration.
  Remove the path-prefix authorization fallback once every public route is migrated.
- [ ] Delete source-text parsing of `.route(...)`. Compare the generated OpenAPI operation set
  directly with the route contract returned while building the runtime router.
- [ ] Validate representative real handler success/error payloads against every response schema,
  including nullable/required semantics and streaming/binary content types.
- [ ] Upgrade dashboard and Python conformance tests from component-name checks to method, templated
  path, parameters/headers, request body, success body, nullable/required fields, and shared error
  compatibility.
- [ ] Regenerate canonical OpenAPI and prove `/openapi.json`, the checked-in artifact, runtime
  registration, SDK maps, and dashboard maps all describe the same operations.
- [ ] Update `m0-baseline.md` and mark M0 complete only when Tasks 1 and 2 satisfy every original M0
  exit criterion.

**Task gate:**

```powershell
cargo fmt --all -- --check
cargo clippy -p capsulet-api --all-targets --all-features --locked -- -D warnings
cargo test -p capsulet-api --locked
cargo run -p capsulet-api --bin export-openapi --locked -- --check
pwsh ./scripts/check-openapi.ps1
pwsh ./scripts/check-sdk-contracts.ps1
pwsh ./scripts/check-contracts.ps1
git diff --check
```

### Task 3: Introduce the cross-platform verification orchestrator

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/xtask/Cargo.toml`
- Create: `crates/xtask/src/main.rs`
- Create modules under: `crates/xtask/src/verify/`
- Create: `crates/xtask/tests/verify_cli.rs`
- Create: `docs/contracts/verification-matrix.md`
- Modify: `docs/development.md`
- Modify: `README.md`
- Convert to thin compatibility wrappers: `scripts/check-contracts.ps1`
- Convert to thin compatibility wrappers: `scripts/check-openapi.ps1`
- Convert to thin compatibility wrappers: `scripts/check-sdk-contracts.ps1`

- [ ] Add failing CLI tests for unknown profiles, absent prerequisites, failed child commands,
  timeouts, cleanup after failure, and an attempted skipped required gate.
- [ ] Define named gates for format, lint, unit, API contracts, SDK, dashboard, PostgreSQL,
  migrations, security, Compose, Helm, and Kind. Record each gate's command, prerequisites,
  timeout, artifacts, and whether it belongs to `fast` and/or `full`.
- [ ] Implement `verify --profile full` with stable ordering, concise progress output, captured logs,
  non-zero exit on any required failure, and a final summary that cannot report green when a gate
  was omitted.
- [ ] Implement explicit provisioning and teardown hooks for disposable services. Teardown must run
  after success, failure, Ctrl+C, and child timeout where the operating system permits it.
- [ ] Add `verify --profile fast` for deterministic checks that require no service containers. Print
  the exact omitted full gates at the start and end.
- [ ] Add `verify --list --format json` so CI and contract evidence can inspect the executable gate
  graph without parsing human output.
- [ ] Keep existing PowerShell commands as thin delegates during the compatibility window; they
  must not contain independent pass/fail logic.
- [ ] Document supported tool versions and make `verify doctor` check Rust, Docker, Node, Python,
  Helm, Kind, kubectl, and scanners without mutating the host.

**Task gate:**

```powershell
cargo test -p capsulet-xtask --locked
cargo run -p capsulet-xtask --locked -- verify --profile fast
cargo run -p capsulet-xtask --locked -- verify --list --format json
git diff --check
```

### Task 4: Make PostgreSQL and migration tests mandatory and isolated

**Files:**

- Move integration coverage from: `crates/postgres/src/tests.rs`
- Create integration tests under: `crates/postgres/tests/`
- Create: `crates/postgres/tests/support/mod.rs`
- Modify: `crates/postgres/Cargo.toml`
- Add migration fixtures under: `crates/postgres/tests/fixtures/`
- Modify: `crates/xtask/src/verify/postgres.rs`
- Modify: `crates/xtask/src/verify/migrations.rs`
- Modify: `compose.yaml`
- Modify: `.github/workflows/ci.yml`

- [ ] Add a failing harness test proving an absent test database configuration is an error, not an
  early return, ignored test, warning, or zero-test success.
- [ ] Replace every `let Some(database_url) = ... else { return; }` path with a required integration
  fixture that provides an actionable failure when invoked outside the verifier/CI environment.
- [ ] Have the verifier provision an ephemeral PostgreSQL instance and create an isolated database
  per test process or suite. Use deterministic fixture content and guaranteed teardown; never share
  mutable application tables between parallel tests.
- [ ] Centralize tenant, project, user, workflow, run, graph, memory, and audit fixture builders.
  Give fixtures stable semantic IDs inside their isolated database rather than wall-clock-derived
  business identifiers.
- [ ] Test migrations from an empty database and from a checked-in schema/data snapshot of every
  source version supported by `docs/contracts/database-migrations.md`.
- [ ] Assert migrations are forward-only, transactional where PostgreSQL permits, repeatably
  detectable, and leave schema/version metadata at the expected release.
- [ ] Add negative tests for malformed connection configuration, unavailable PostgreSQL, migration
  failure, and cleanup failure.
- [ ] Make the CI PostgreSQL job required and ensure its test-count artifact proves integration and
  migration suites actually executed.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate postgres --gate migrations
git diff --check
```

### Task 5: Define typed IAM policy and enforce project ownership atomically

**Files:**

- Modify: `crates/api/src/auth.rs`
- Modify: `crates/api/src/endpoint_contract.rs`
- Modify: `crates/api/src/http/internal.rs`
- Modify: `crates/api/src/store.rs`
- Modify project-scoped handlers under: `crates/api/src/`
- Modify repositories under: `crates/postgres/src/`
- Modify: `crates/postgres/src/projects.rs`
- Modify: `crates/postgres/src/service_accounts.rs`
- Modify: `dashboard/app/lib/api.ts`
- Modify: `dashboard/app/artifacts/page.tsx`
- Create: `crates/api/tests/authorization_matrix.rs`
- Create: `docs/contracts/authorization-matrix.md`
- Regenerate: `crates/api/openapi.json`

- [ ] Add failing table-driven tests for anonymous, tenant member, project viewer, project operator,
  project admin, service account, wrong-project identity, and global admin across every public
  operation.
- [ ] Define typed `Permission`, `ProjectRole`, `ResourceKind`, `OwnershipRule`, and
  `EndpointPolicy` values. Remove impossible role names such as `editor` unless explicitly added to
  the documented role model.
- [ ] Define one role-to-permission matrix covering current jobs, workflows, automations, execution
  pools, artifacts/logs, graphs, agents, reasoning, certificates, memory, connectors, ingestion,
  review, audit, project membership, and service-account operations.
- [ ] Generate the human-readable authorization matrix and OpenAPI permission/project extensions
  from the same typed policy data used at runtime.
- [ ] Require tenant and project predicates in repository reads and writes for project-scoped
  resources. Missing ownership metadata must deny access or surface a contract error; it must never
  default to allow.
- [ ] Create resource and ownership association in one database transaction. Remove post-create
  best-effort ownership updates and cross-project upsert ambiguity.
- [ ] Define and test deliberate `403` versus non-disclosing `404` behavior for wrong-project and
  missing-resource access.
- [ ] Ensure artifact downloads and every dashboard direct fetch forward the selected project
  context through the proxy.
- [ ] Generate one authorization test case per public route and fail when a route lacks a case or a
  declared role has no expected outcome.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate postgres --gate api-contracts --gate authorization
Push-Location dashboard
npm test
npx tsc --noEmit
Pop-Location
git diff --check
```

### Task 6: Make Compose, seeds, and end-to-end identities deterministic

**Files:**

- Modify: `compose.yaml`
- Modify: `scripts/compose-smoke.ps1` or replace it with an `xtask` compatibility wrapper
- Create: `crates/xtask/src/verify/compose.rs`
- Create: `crates/xtask/src/fixtures.rs`
- Create or modify seed support under: `crates/api/src/bin/`
- Modify: `dashboard/tests/e2e/auth.setup.ts`
- Modify: `dashboard/tests/e2e/*.spec.ts`
- Modify: `dashboard/playwright.config.ts`
- Create: `docs/development/test-identities.md`

- [ ] Add failing tests showing two consecutive full-stack runs produce the same logical fixture
  graph and leave no state that changes the next run's assertions.
- [ ] Replace checked-in shared admin credentials with per-run test secrets generated by the
  verifier and passed through an ephemeral environment file that is deleted during teardown.
- [ ] Add an idempotent seed command for one tenant, two projects, and viewer/operator/admin/service
  identities with documented stable aliases. Emit IDs and short-lived test credentials as JSON for
  Playwright rather than duplicating literals.
- [ ] Remove `Date.now()` and random business identifiers from assertions. Use a run namespace only
  for infrastructure isolation, while fixture content and expected results remain deterministic.
- [ ] Make E2E setup seed through the public/bootstrap contract, persist authenticated states for
  each role, and clean up its isolated namespace after the suite.
- [ ] Add positive and negative project-isolation flows, including artifact download, memory review,
  graph/agent operations, and audit visibility.
- [ ] Pin Compose dependency image tags or digests and add health checks so readiness never depends
  on fixed sleeps.
- [ ] Make the Compose gate validate configuration, build local images, wait on health, seed, run
  API/dashboard tests, collect logs on failure, and remove only resources carrying its unique
  project label.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate compose --gate dashboard-e2e
git diff --check
```

### Task 7: Restore zero-warning code and dependency gates

**Files:**

- Modify Rust modules reported by workspace Clippy, beginning with
  `crates/api/src/{endpoint_contract,openapi}.rs`
- Modify: `dashboard/package.json`
- Modify: `dashboard/package-lock.json`
- Modify: `dashboard/eslint.config.mjs`
- Modify affected dashboard source/tests
- Modify: `deny.toml`
- Create: `docs/contracts/dependency-policy.md`
- Create: `docs/contracts/security-exceptions.json`
- Create: `docs/contracts/security-exceptions.schema.json`
- Modify: `crates/xtask/src/verify/security.rs`

- [ ] Add failing verifier tests for a Clippy warning, ESLint configuration crash, high-severity npm
  advisory, expired exception, unreviewed duplicate dependency, and vulnerable Rust advisory.
- [ ] Make workspace format and Clippy with `-D warnings` green. Refactor oversized/manual contract
  functions rather than globally allowing the lints that expose their design problem.
- [ ] Align ESLint, `eslint-config-next`, and plugins on compatible supported versions and make
  `npm run lint` inspect application, proxy, configuration, unit, and E2E code.
- [ ] Upgrade or constrain `brace-expansion` and `nanoid` so `npm audit --audit-level=high` is green.
- [ ] Define update cadence, allowed licenses, duplicate-version policy, severity thresholds, and
  time-bounded exception fields with owner, rationale, compensating control, issue, and expiry.
- [ ] Remove the standing RustSec ignore where an upgrade is possible. Any temporary exception must
  live in the validated registry and fail automatically at expiry.
- [ ] Add Python package auditing even if the current SDK has no runtime third-party dependencies,
  so a future dependency cannot bypass the gate.
- [ ] Make generated artifacts and lockfiles deterministic and verify that dependency checks do not
  rewrite the worktree.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate lint --gate dependency-security
git diff --check
git status --short
```

### Task 8: Establish one release version and one container/chart identity

**Files:**

- Create: `release.toml`
- Create: `docs/contracts/release-process.md`
- Modify: root and crate `Cargo.toml` files
- Modify: `sdk/python/pyproject.toml`
- Modify: `dashboard/package.json`
- Modify: `charts/capsulet/Chart.yaml`
- Modify: `charts/capsulet/values.yaml`
- Modify: `compose.yaml`
- Choose and keep one: `dashboard/Dockerfile`
- Remove after migration: `Dockerfile.dashboard`
- Consolidate Rust image definitions among: `Dockerfile.rust`, `crates/Dockerfile`
- Create: `crates/xtask/src/release.rs`
- Create: `crates/xtask/tests/release_contract.rs`
- Modify: API/CLI/dashboard served version metadata

- [ ] Add failing tests for a mismatched crate, Python SDK compatibility version, dashboard version,
  chart `appVersion`, image tag, OpenAPI version, CLI version, and served API version.
- [ ] Define `release.toml` as the repository release identity: product version, source repository,
  image registry/namespace, component image names, and supported compatibility range.
- [ ] Use `[workspace.package]` for Rust package metadata and inherit it from internal crates unless a
  separately published component is explicitly justified by policy.
- [ ] Implement `xtask release check` and `xtask release set <version> --dry-run/--write`. The write
  command updates all derived manifests deterministically and refuses a dirty worktree unless an
  explicit release-maintainer override is supplied.
- [ ] Align GitHub-published image references, chart defaults, Compose defaults, docs, and examples.
  Ensure names do not gain an accidental duplicate repository segment.
- [ ] Keep one hardened standalone dashboard Dockerfile and one deliberate Rust multi-target build
  strategy. Update every workflow and manifest before removing superseded files.
- [ ] Label images with version, revision, source URL, licenses, and creation metadata; do not use
  `latest` in release or test contracts.
- [ ] Add chart render assertions proving each workload resolves to the image name/tag/digest from
  the release contract.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- release check
cargo run -p capsulet-xtask --locked -- verify --gate containers --gate helm
git diff --check
```

### Task 9: Build, scan, attest, and sign the same immutable artifacts

**Files:**

- Modify: `.github/workflows/supply-chain.yml`
- Create reusable workflow(s) under: `.github/workflows/`
- Modify: `.github/dependabot.yml` or add the selected dependency-update configuration
- Create: `docs/security/supply-chain.md`
- Modify: `docs/contracts/security-exceptions.json`
- Modify: `crates/xtask/src/verify/security.rs`
- Modify: `crates/xtask/src/verify/containers.rs`

- [ ] Add workflow contract tests that fail when a third-party action is not pinned to a full commit
  SHA, an image is rebuilt after scanning, signing targets a mutable tag, or required attestations
  are absent.
- [ ] Add secret scanning over the full Git history in CI and a documented pre-commit invocation.
  Validate committed fixtures through narrow allowlists with owner and expiry.
- [ ] Pin third-party Actions by full commit SHA and configure automated update proposals so pins do
  not become stale.
- [ ] Build each container once, load/test that image, push it once, and capture its immutable digest.
  Run vulnerability and configuration scans against that digest.
- [ ] Generate SPDX or CycloneDX SBOMs for images and source packages, upload them as build
  artifacts, and attach them to the immutable release digest.
- [ ] Produce GitHub/SLSA-compatible build provenance containing repository, revision, workflow,
  builder, parameters, and subject digest.
- [ ] Sign and attest the image digest and Helm package using keyless Sigstore in protected release
  workflows. Verify signatures, identity, issuer, provenance subject, and SBOM association before
  publishing the release manifest.
- [ ] Keep pull-request workflows unprivileged: build and scan, but never receive release signing or
  registry credentials from untrusted code.
- [ ] Publish scanner versions and policy results in the job summary; never convert a failed required
  scan to a warning.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate supply-chain-contract
git diff --check
```

### Task 10: Consolidate CI and add the real Helm/Kind smoke gate

**Files:**

- Modify: `.github/workflows/ci.yml`
- Consolidate or remove duplicated checks from: `.github/workflows/rust.yml`
- Consolidate or remove duplicated checks from: `.github/workflows/dashboard.yml`
- Consolidate or remove duplicated checks from: `.github/workflows/helm.yml`
- Modify: `.github/workflows/wasm-python-integration.yml`
- Modify: `charts/capsulet/Chart.yaml`
- Modify: `charts/capsulet/values.yaml`
- Modify templates under: `charts/capsulet/templates/`
- Create: `crates/xtask/src/verify/kind.rs`
- Add Kind fixtures under: `tests/kind/`
- Create: `docs/development/ci.md`

- [ ] Add failing workflow tests showing that duplicated ad hoc commands, silent conditional skips,
  or a green required job without its expected gate result are rejected.
- [ ] Make one primary CI workflow call named `xtask` gates. Use a matrix only for supported
  platforms/toolchains and use reusable workflows only for scheduling/caching/permissions.
- [ ] Upload a machine-readable verification summary with gate name, command version, status,
  duration, test count, and artifact digest. The final required job fails if any required gate is
  missing, skipped, cancelled, or red.
- [ ] Keep scheduled WASI/Python coverage only if it supplements a deterministic PR-safe contract
  test. Required integration behavior must not exist solely behind schedule/manual dispatch.
- [ ] Provision a disposable Kind cluster, load the exact locally built images, install the Helm
  chart with generated test credentials, wait on Kubernetes readiness, and seed through the
  supported bootstrap path.
- [ ] Test health, dashboard login, one representative project-scoped workflow operation, one
  correctness-kernel operation, cross-project denial, and clean uninstall using Kubernetes network
  paths. Do not redirect the smoke test to Compose services.
- [ ] Run Helm lint/template plus schema and Kubernetes manifest validation before cluster install.
  Assert security contexts, service accounts, probes, required secrets, and image identities.
- [ ] Collect namespace events, pod descriptions, and logs on failure, then delete only the
  verifier-created cluster.
- [ ] Document the one local command, required CI jobs, failure reproduction, cache boundaries, and
  how to inspect verification artifacts.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate helm --gate kind
git diff --check
```

### Task 11: Close M1 with a clean-checkout rehearsal

**Files:**

- Modify: `docs/contracts/product-claims.json`
- Regenerate: `docs/contracts/product-claims.md`
- Create: `docs/contracts/m1-completion.md`
- Modify: `docs/contracts/README.md`
- Modify: `docs/development.md`
- Modify: `README.md`
- Modify: `.github/pull_request_template.md`

- [ ] Add an initially failing completion check requiring every M1 deliverable and exit criterion to
  link to its executable gate and latest clean-checkout result.
- [ ] Clone or archive-export the repository into a new path with no ignored build products, local
  databases, generated secrets, or tool caches from the development checkout.
- [ ] Run `cargo run -p capsulet-xtask --locked -- verify --profile full` using only documented
  prerequisites. Record tool versions, revision, duration, gate/test counts, image digests, chart
  render digest, and verification-summary digest.
- [ ] Prove the full command leaves the source checkout clean and deletes its disposable database,
  Compose project, Kind cluster, temporary credentials, and ephemeral files.
- [ ] Confirm every public route appears in generated OpenAPI and has executed anonymous,
  authorized-role, insufficient-role, and wrong-project authorization cases where applicable.
- [ ] Confirm the CI required-job aggregator fails in a controlled test when PostgreSQL,
  authorization, migration, security, Helm, or Kind evidence is removed.
- [ ] Update public claims only for behavior the new gates actually prove. Keep M2-M6 work explicit
  as experimental/planned and do not add an `alpha-ready` claim.
- [ ] Complete `m1-completion.md` with remaining risks, time-bounded security exceptions, platform
  support, and the exact next milestone entry conditions.
- [ ] Commit only after the full gate, generated-artifact checks, `git diff --check`, and a reviewed
  `git status --short` show no unintended files.

## M1 release gate

The supported command is:

```powershell
cargo run -p capsulet-xtask --locked -- verify --profile full
```

The full profile must include, without silent omission:

1. repository cleanliness and generated-contract checks;
2. Rust format, Clippy, unit, API, integration, and documentation tests;
3. mandatory PostgreSQL and supported migration-path tests;
4. public-claim, lifecycle, OpenAPI, SDK, and authorization contract tests;
5. Python SDK tests and dependency audit;
6. dashboard lint, typecheck, unit, build, dependency audit, and Playwright tests;
7. Rust, JavaScript, Python, license, vulnerability, and secret checks;
8. deterministic container build, image scan, and SBOM/provenance contract checks;
9. Compose configuration/build/smoke tests;
10. Helm lint/schema/render checks and a real Kind install/smoke/uninstall test;
11. release-version, image-name, chart, and artifact-identity checks; and
12. cleanup and final clean-worktree verification.

## M1 exit criteria

M1 is complete only when all of the following are true:

- the original M0 exit criteria pass with route declarations and real Rust wire types as the HTTP
  authority, discovered public surfaces, and evidence that executes the claimed behavior;
- a clean checkout passes the documented full verification command;
- a required check cannot report success when PostgreSQL, migration, authorization, security,
  Compose, Helm, or Kind coverage was skipped;
- every public route is documented and has generated, executed authorization coverage;
- project-scoped reads and writes enforce tenant/project ownership atomically and wrong-project
  behavior is tested;
- fixtures, seeds, E2E identities, and migration inputs are deterministic and isolated;
- workspace lint, dependency, vulnerability, license, and secret gates are green or covered by a
  visible, owned, unexpired exception;
- CI artifacts have SBOMs and signed provenance tied to the same tested immutable digest;
- Rust crates, API metadata, SDK compatibility metadata, dashboard, images, and Helm chart agree
  with the release contract;
- Compose and a real Kind-installed Helm release both pass their distinct smoke paths; and
- documentation describes limitations honestly and does not call Capsulet alpha-ready.

Passing M1 means contributors and users can trust what was built and what was tested. It does not
yet mean agent execution is semantically verified; M2 owns the verified-computation IR and replayable
certificate model.
