# Product Constitution and Contract Inventory Implementation Plan

> **For agentic workers:** Implement this plan task-by-task with test-first changes and one focused
> commit per task. This milestone establishes enforceable product contracts; it does not implement
> the verified-computation IR or durable graph runtime planned for M2 and M3.

**Goal:** Complete M0 by making Capsulet's approved correctness-first product definition, public
claims, lifecycle vocabulary, compatibility rules, HTTP contract, and SDK boundary explicit,
machine-checkable, and continuously enforced.

**Architecture:** Human-readable policy lives under `docs/contracts/`. A machine-readable product
claim registry connects every public capability or guarantee to executable evidence or an explicit
experimental/planned label. The Axum route declarations and Rust wire models generate OpenAPI so
runtime and specification cannot drift. Stable operation metadata connects endpoint authorization,
OpenAPI, the dashboard client, and the Python SDK. One contract-check entry point runs locally and
in CI.

**Tech Stack:** Rust 1.96, Axum 0.8, Serde/JSON Schema, Utoipa 5.5,
`utoipa-axum` 0.2, PowerShell 7, OpenAPI 3.1, TypeScript/Vitest, Python `unittest`, GitHub Actions.

---

## Chosen Approach

Use executable registries and code-generated contracts.

- A docs-only rewrite would be quick but would immediately drift again.
- Generating all product prose and clients from one schema would create a large custom
  documentation system before the underlying product contracts are stable.
- The selected middle path keeps narrative documentation handwritten, but makes claims, endpoint
  inventory, schemas, authorization metadata, stability labels, and evidence links enforceable.

The authoritative sources are:

| Contract | Authority |
| --- | --- |
| Product definition and architectural direction | `docs/superpowers/specs/2026-08-30-correctness-first-agent-workflow-platform-design.md` plus accepted ADRs |
| Public capability and guarantee status | `docs/contracts/product-claims.json` |
| Runtime HTTP paths, methods, and wire schemas | Rust endpoint declarations and wire models in `capsulet-api` |
| Published HTTP contract | generated `crates/api/openapi.json` |
| Execution and assurance terminology | `docs/contracts/lifecycle-and-assurance.md` |
| Stability, versioning, deprecation, migration, and SDK rules | documents under `docs/contracts/` plus their ADRs |

## Scope Boundaries

M0 must not silently pull later milestones forward.

- Do not replace `state_json`, add the v1 workflow IR, or implement trust-typed values; those are M2.
- Do not add the dedicated graph worker, loops, waits, compensation, or recovery behavior; those
  are M3.
- Do not rename current persisted run-status enums merely to match target vocabulary. Document the
  mapping and debt first.
- Do not add unimplemented target endpoints to OpenAPI. OpenAPI describes the running API only.
- Do not fix every IAM, packaging, CI, or dashboard issue found by the repository audit. Inventory
  and test the contract now; M1 owns engineering-integrity fixes.
- If a public claim is false or only partly implemented, split it into precise claims and label the
  unimplemented part. Do not implement a feature just to preserve marketing copy.

## Baseline to Preserve in the Completion Report

The implementation starts from these measured facts:

- the runtime registers 90 distinct literal HTTP paths;
- the handwritten OpenAPI file lists 66 paths and omits 24 runtime paths;
- its 89 documented operations have no `operationId`, request body, or parameter definitions;
- it defines one reusable schema (`Error`);
- `scripts/check-openapi.ps1` checks only eight selected paths;
- public docs and package metadata variously describe Capsulet as a memory platform, automation
  platform, workflow SDK, or job runner;
- current kernel verdicts are three-valued, while the approved platform assurance model adds
  `unverified` outside the kernel;
- current dashboard and Python clients are handwritten and have no complete OpenAPI conformance
  check.

These numbers are baseline observations, not permanent assertions. The final report records the
new generated counts and proves set equality instead of hard-coding a desired route count.

---

### Task 1: Establish the public-contract registry and validation tools

**Files:**

- Create: `docs/contracts/README.md`
- Create: `docs/contracts/product-claims.schema.json`
- Create: `docs/contracts/product-claims.json`
- Create: `docs/contracts/product-claims.md`
- Create: `scripts/check-product-claims.ps1`
- Create: `scripts/render-product-claims.ps1`
- Create: `scripts/tests/check-product-claims.ps1`
- Create: `scripts/fixtures/contracts/valid-claims.json`
- Create: `scripts/fixtures/contracts/invalid-claims/`

- [ ] Write failing script tests for duplicate IDs, unknown maturity/kind values, missing files,
  missing evidence, invalid test selectors, unmarked public surfaces, stale generated Markdown,
  and an implemented guarantee without executable evidence.
- [ ] Define stable claim IDs such as `CAP-PRODUCT-001`, `CAP-RUNTIME-001`, and
  `CAP-CORRECTNESS-001`; IDs are never reused after retirement.
- [ ] Define claim kinds: `positioning`, `capability`, `guarantee`, `limitation`, and `compatibility`.
- [ ] Define maturity values: `implemented`, `experimental`, and `planned`. Deliberately omit
  `partial`; split partial statements into narrower implemented and planned claims.
- [ ] Require every claim to include a statement, owner area, public surfaces, maturity, and
  evidence records. Capability and guarantee claims marked `implemented` require at least one
  executable test command plus a repository path and test selector.
- [ ] Allow decisions/specs as evidence for positioning claims and source/test references as
  evidence for limitations, without treating either as proof of an implemented capability.
- [ ] Define the public-surface inventory in the same registry: root docs, current product docs,
  package/chart metadata, SDK docs, dashboard product copy, examples, and served OpenAPI.
- [ ] Make `check-product-claims.ps1` validate the JSON Schema, semantic rules, referenced paths,
  public-surface coverage, and generated Markdown determinism.
- [ ] Make `render-product-claims.ps1` produce a stable human-readable table grouped by product
  area and maturity without timestamps or machine-specific paths.
- [ ] Run `pwsh ./scripts/tests/check-product-claims.ps1`; expect every invalid fixture to fail for
  its intended reason and the valid fixture to pass.

### Task 2: Inventory every existing public claim

**Files:**

- Modify: `docs/contracts/product-claims.json`
- Regenerate: `docs/contracts/product-claims.md`
- Create: `docs/contracts/m0-baseline.md`
- Inspect and annotate: `README.md`
- Inspect and annotate: `ARCHITECTURE.md`
- Inspect and annotate: `docs/*.md`
- Inspect and annotate: `docs/operations/*.md`
- Inspect and annotate: `docs/security/*.md`
- Inspect and annotate: `sdk/python/README.md`
- Inspect and annotate: `dashboard/README.md`
- Inspect and annotate: `examples/**/*.md`
- Inspect and annotate: `charts/capsulet/Chart.yaml`
- Inspect and annotate: `charts/capsulet/values.yaml`
- Inspect and annotate: `charts/capsulet/values.schema.json`
- Inspect and annotate: user-facing copy under `dashboard/app/`

- [ ] Add failing coverage tests showing that the registry does not yet account for each declared
  public surface.
- [ ] Inventory claims by area: product identity, correctness/kernel, graphs/agents, governed
  memory, jobs/runners, workflows/automations, identity/IAM, persistence/storage, dashboard/SDK,
  observability/operations, security/isolation, and Helm/self-hosting.
- [ ] Separate claims about implemented behavior from future direction. A design document is not
  executable evidence that its behavior exists.
- [ ] Split compound claims so each can have one honest maturity and evidence set. For example,
  persistence of queued agent runs and production worker execution are separate claims.
- [ ] Mark compatibility workflow behavior as implemented where tests support it without presenting
  that behavior as Capsulet's long-term product center.
- [ ] Record known limitations as first-class claims, including the absent graph worker, opaque
  agent state, incomplete default Helm authentication path, and current verification boundary.
- [ ] Map implemented capability and guarantee claims to existing test commands. If no executable
  evidence exists, downgrade the claim to `experimental` and create an M1 follow-up in the baseline
  report rather than weakening the registry rule.
- [ ] Add lightweight claim-reference markers to the listed public surfaces. Markers attach a
  section or statement to stable claim IDs without exposing internal test paths in user-facing
  prose.
- [ ] Record totals by kind, maturity, product area, and missing-evidence reason in
  `m0-baseline.md`.
- [ ] Run `pwsh ./scripts/check-product-claims.ps1`; expect zero unregistered public surfaces,
  duplicate IDs, and invalid implemented claims.

### Task 3: Align product language and maturity labels

**Files:**

- Modify: `README.md`
- Modify: `ARCHITECTURE.md`
- Modify: `docs/README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/development.md`
- Modify: `docs/installation.md`
- Modify: `docs/api.md`
- Modify: `docs/operations.md`
- Modify: `docs/security.md`
- Modify: `sdk/python/README.md`
- Modify: `sdk/python/pyproject.toml`
- Modify: `dashboard/README.md`
- Modify: dashboard metadata and affected product-copy modules under `dashboard/app/`
- Modify: `charts/capsulet/Chart.yaml`
- Modify: `charts/capsulet/values.schema.json`

- [ ] Add failing claim-coverage checks for the current conflicting product descriptions.
- [ ] Put the approved one-sentence product definition at the root documentation entry point and
  link the complete constitution.
- [ ] Present three layers consistently: workflow engine, agent platform, and correctness plane.
- [ ] Give every major product surface an explicit “implemented now,” “experimental,” or “planned”
  boundary. Keep target architecture out of sections titled “what works today.”
- [ ] Reframe governed memory as a major subsystem rather than the complete product identity.
- [ ] Reframe current workflow DAGs and the Python workflow SDK as implemented compatibility/tool
  infrastructure rather than the destination.
- [ ] Replace the Helm chart's “automation platform and sandboxed job runner” description with
  correctness-first positioning plus an honest alpha-readiness qualifier.
- [ ] Remove “public-alpha stack” wording from installation docs until the public-alpha gate in the
  constitution passes.
- [ ] Label static/demo dashboard content and future settings explicitly; do not create new UI
  behavior in this task.
- [ ] Make historical designs visibly historical or superseded while preserving them as project
  history.
- [ ] Run claim validation, Markdown link checks, dashboard unit tests affected by copy changes,
  and `helm lint charts/capsulet`.

### Task 4: Define lifecycle and assurance contracts without changing runtime behavior

**Files:**

- Create: `docs/contracts/lifecycle-and-assurance.md`
- Create: `docs/contracts/lifecycle-mapping.json`
- Create: `docs/contracts/lifecycle-mapping.schema.json`
- Modify: `docs/contracts/product-claims.json`
- Modify: `docs/api.md`
- Modify: `ARCHITECTURE.md`
- Modify: `docs/architecture.md`
- Modify: `docs/design/correctness-architecture.md`
- Modify: `crates/api/openapi.json` only through the generator introduced later in this plan
- Modify: `scripts/check-product-claims.ps1`

- [ ] Add failing validation fixtures for a collapsed execution/assurance state, an undocumented
  persisted status, and an invalid transition reference.
- [ ] Define target execution statuses (`queued`, `running`, `waiting`, `completed`, `failed`, and
  `cancelled`) independently from platform assurance verdicts (`unverified`, `accepted`,
  `conditional`, and `rejected`).
- [ ] Explicitly distinguish the current three-valued `capsulet-kernel::Verdict` from the target
  four-valued platform assurance verdict. `unverified` means the correctness plane did not run; it
  is not a fourth kernel conclusion.
- [ ] Inventory every current persisted and API-visible job, workflow, agent, automation,
  ingestion, review, memory, and kernel status.
- [ ] Map current statuses to target concepts and record lossy or ambiguous mappings as M2/M3 debt.
- [ ] Document terminality, retryability, cancellation, and allowed transition ownership for each
  current lifecycle without changing enums or migrations.
- [ ] Define the rule that no API, dashboard view, metric, automation condition, or documentation
  may infer assurance from successful execution.
- [ ] Update correctness docs so their status accurately distinguishes implemented provenance/kernel
  slices from target runtime admission behavior.
- [ ] Run lifecycle schema checks and existing core/kernel status tests; expect documentation and
  code inventory to agree exactly.

### Task 5: Record product, compatibility, migration, and deprecation decisions

**Files:**

- Create: `docs/adr/0014-correctness-first-agent-workflow-product.md`
- Create: `docs/adr/0015-code-generated-openapi-contract.md`
- Create: `docs/adr/0016-public-contract-stability-and-versioning.md`
- Modify: `docs/adr/0012-correctness-kernel-and-proposer-checker-split.md`
- Create: `docs/contracts/stability-and-versioning.md`
- Create: `docs/contracts/database-migrations.md`
- Create: `docs/contracts/sdk-generation.md`
- Modify: `docs/contracts/README.md`
- Modify: `docs/README.md`
- Modify: `README.md`

- [ ] Add claim-registry failures for product or compatibility decisions that lack an accepted ADR.
- [ ] Accept ADR 0012 with a note that the platform assurance layer adds `unverified` while the
  deterministic kernel remains three-valued.
- [ ] Record the correctness-first workflow/agent product boundary and the unified target IR in ADR
  0014.
- [ ] Record code-generated OpenAPI, checked-in deterministic output, and exact runtime parity in
  ADR 0015.
- [ ] Record public stability classes, version coordination, deprecation windows, compatibility
  readers, and migration obligations in ADR 0016.
- [ ] Define `stable`, `experimental`, and `internal` API/SDK/IR/certificate/domain-pack surfaces.
  Before alpha, everything defaults to experimental unless explicitly promoted.
- [ ] Require stable alpha removals or incompatible changes to receive at least one published alpha
  minor release of deprecation notice, release notes, and a migration path, except urgent security
  removals documented in an advisory.
- [ ] Define one release version propagated to images, chart `appVersion`, CLI, dashboard build
  metadata, SDK compatibility metadata, and served API metadata; component package versions may
  differ only when the policy explains why.
- [ ] Define forward-only database migrations, expand/migrate/contract sequencing, supported source
  versions, pre-upgrade backup, restore verification, and rollback behavior. Do not claim SQL
  rollback scripts can undo all data migrations.
- [ ] Define future IR/certificate/domain-pack schema version fields and compatibility-reader rules
  without adding their M2 runtime models.
- [ ] Define SDK policy: generated transport bindings derive from stable `operationId` and schema
  names; handwritten ergonomic authoring layers wrap them; generated output never becomes a second
  API authority.
- [ ] Run contract validation and link checks; expect every normative policy to be indexed from
  `docs/contracts/README.md`.

### Task 6: Replace handwritten route inventory with code-generated OpenAPI

**Files:**

- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/api/Cargo.toml`
- Create: `crates/api/src/openapi.rs`
- Create: `crates/api/src/endpoint_contract.rs`
- Create: `crates/api/src/bin/export-openapi.rs`
- Create: `crates/api/tests/openapi_contract.rs`
- Modify: `crates/api/src/lib.rs`
- Modify: `crates/api/src/http/internal.rs`
- Modify: `crates/api/src/auth.rs`
- Replace generated content: `crates/api/openapi.json`
- Replace: `scripts/check-openapi.ps1`

- [ ] Add failing tests that compare runtime and OpenAPI method/path sets and reproduce the 24
  missing-path baseline.
- [ ] Add failing tests for duplicate/missing `operationId`, missing path parameters, undocumented
  request bodies, body-returning success responses without schemas, missing error schemas, and
  incorrect security declarations.
- [ ] Add `utoipa` and `utoipa-axum` through workspace dependencies and construct public endpoints
  through `OpenApiRouter`/annotated route declarations.
- [ ] Introduce endpoint metadata with method, path template, stable operation ID, stability class,
  required scope, project-context requirement, and public/protected authentication mode.
- [ ] Make authorization middleware consume the same endpoint metadata rather than independently
  inferring every scope from string prefixes. Preserve current authorization behavior in this task;
  record inconsistent policies for M1.
- [ ] Require all public routes to use the contract-aware route declaration. Keep raw Axum routes
  behind an explicit internal-route allowlist and fail CI if a new `/v1` route bypasses the
  contract.
- [ ] Generate OpenAPI 3.1 at compile/test time, serve that document at `/openapi.json`, and export a
  canonically ordered checked-in `crates/api/openapi.json`.
- [ ] Make `export-openapi --check` fail when the checked-in artifact differs from generated output.
- [ ] Replace the eight-path PowerShell check with a thin entry point that invokes the Rust contract
  tests and deterministic export check.
- [ ] Add tests proving `/openapi.json` returns the same contract as the checked-in artifact and
  that auth/public-route behavior is unchanged.
- [ ] Run `cargo test -p capsulet-api --test openapi_contract --locked` and
  `cargo run -p capsulet-api --bin export-openapi --locked -- --check`.

### Task 7: Complete control-plane and execution API schemas

**Files:**

- Modify: `crates/api/src/models.rs`
- Modify: `crates/api/src/error.rs`
- Modify: `crates/api/src/http/internal.rs`
- Modify: `crates/api/src/automations.rs`
- Modify: `crates/api/src/webhooks.rs`
- Modify: `crates/api/src/openapi.rs`
- Modify: `crates/api/tests/openapi_contract.rs`
- Regenerate: `crates/api/openapi.json`
- Modify: `docs/api.md`

- [ ] Add failing schema/operation tests for health, identity, projects, memberships, service
  accounts, audit, job definitions/runs, workflow definitions/runs, automations/triggers,
  execution pools, topology, logs, artifacts, SSE, and signed webhooks.
- [ ] Give every request and response wire type an explicit reusable schema name and field-level
  required/nullable semantics.
- [ ] Document every path, query, header, and cookie parameter, including
  `x-capsulet-project-id`, pagination/filter fields, request IDs, bearer auth, and webhook signature
  headers.
- [ ] Document request bodies and content types, including binary artifact responses and
  `text/event-stream` event shapes.
- [ ] Document all handler-observable status codes using shared error schemas; do not advertise a
  response the implementation cannot produce.
- [ ] Attach endpoint stability and current required-scope metadata as OpenAPI extensions.
- [ ] Add representative API tests that deserialize successful and error responses through their
  documented schemas.
- [ ] Regenerate OpenAPI and run the exact runtime/spec set comparison; expect no control-plane or
  execution operation to be undocumented.

### Task 8: Complete agent, correctness, memory, and ingestion API schemas

**Files:**

- Modify: `crates/api/src/graphs.rs`
- Modify: `crates/api/src/reasoning.rs`
- Modify: `crates/api/src/memory.rs`
- Modify: `crates/api/src/ingestion.rs`
- Modify: `crates/api/src/models.rs`
- Modify: `crates/api/src/openapi.rs`
- Modify: `crates/api/tests/openapi_contract.rs`
- Regenerate: `crates/api/openapi.json`
- Modify: `docs/api.md`
- Modify: `docs/contracts/product-claims.json`

- [ ] Add failing contract tests for all graph, agent, agent-run, reasoning, certificate, memory,
  connector, ingestion-run, review, conflict, and entity-resolution operations.
- [ ] Cover every route absent from the handwritten baseline, including reasoning/certificates,
  ingestion/review, conflicts, resolution actions, and connector runs.
- [ ] Model the current kernel certificate and current agent-run execution state honestly. Do not
  expose target M2 assurance types as implemented API schemas.
- [ ] Mark the agent execution APIs experimental and state that starting a run persists queued work
  without implying a production graph worker exists.
- [ ] Document project-context and authorization metadata for every resource and record detected
  scope/ownership inconsistencies as M1 findings.
- [ ] Document evidence byte ranges, source hashes, certificate residuals, and verifier output only
  where the current response actually carries them.
- [ ] Add response examples for accepted, conditional, and rejected kernel certificates, plus an
  explicit prose note that `unverified` belongs to the future platform assurance layer.
- [ ] Regenerate OpenAPI and run the complete contract tests; expect runtime and OpenAPI path/method
  sets to be identical and every operation to satisfy schema/security rules.

### Task 9: Define and enforce the SDK/client contract boundary

**Files:**

- Modify: `docs/contracts/sdk-generation.md`
- Create: `scripts/check-sdk-contracts.ps1`
- Modify: `sdk/python/src/capsulet/client.py`
- Modify: `sdk/python/tests/test_client.py`
- Create: `sdk/python/tests/test_openapi_contract.py`
- Modify: `sdk/python/pyproject.toml`
- Modify: `sdk/python/README.md`
- Modify: `dashboard/app/lib/api.ts`
- Create: `dashboard/tests/openapi-contract.test.ts`
- Modify: `dashboard/package.json`
- Modify: `dashboard/README.md`
- Modify: `crates/api/openapi.json` only through generation

- [ ] Add failing tests for each Python/dashboard method whose HTTP method/path lacks a generated
  OpenAPI operation or whose expected success/error shape is incompatible.
- [ ] Assign stable operation IDs and schema names suitable for TypeScript and Python generation;
  fail on names that collide after common language normalization.
- [ ] Create explicit operation maps in the handwritten clients so conformance tests do not depend
  on brittle source-text parsing.
- [ ] Verify existing Python client operations against OpenAPI at test time and keep ergonomic
  workflow authoring separate from transport models.
- [ ] Verify dashboard request methods, path templates, project headers, and response assumptions
  against OpenAPI.
- [ ] Mark both handwritten clients experimental until generated transport bindings are introduced
  and published under the compatibility policy.
- [ ] Document the future generation flow: checked OpenAPI -> pinned generator -> generated
  transport -> handwritten ergonomic layer -> conformance tests. M0 validates generation inputs but
  does not publish new SDK packages.
- [ ] Make `check-sdk-contracts.ps1` run both client contract suites with clear missing-operation and
  schema-mismatch errors.
- [ ] Run Python SDK tests and dashboard unit/type tests; expect every declared client operation to
  resolve to exactly one OpenAPI `operationId`.

### Task 10: Add the unified contract gate and close M0

**Files:**

- Create: `scripts/check-contracts.ps1`
- Modify: `scripts/check-openapi.ps1`
- Modify: `scripts/check-product-claims.ps1`
- Modify: `scripts/check-sdk-contracts.ps1`
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/rust.yml`
- Create or modify: `.github/pull_request_template.md`
- Modify: `docs/development.md`
- Modify: `docs/README.md`
- Modify: `README.md`
- Complete: `docs/contracts/m0-baseline.md`
- Regenerate: `docs/contracts/product-claims.md`
- Regenerate: `crates/api/openapi.json`

- [ ] Add an initially failing aggregate check proving claim, lifecycle, OpenAPI, and SDK checks are
  not yet all wired into one command.
- [ ] Implement `pwsh ./scripts/check-contracts.ps1` as the local and CI entry point. It validates
  registries, generated docs, lifecycle mapping, ADR links/status, generated OpenAPI, endpoint
  parity, authorization metadata, and SDK operation maps.
- [ ] Add the aggregate check to every existing required CI workflow that currently owns Rust/API
  contract verification. Do not duplicate contract logic in workflow YAML.
- [ ] Add a pull-request checklist for product claims, public API changes, wire schemas,
  authorization, migration impact, stability labels, SDK impact, and documentation.
- [ ] Document how contributors add or change a claim, endpoint, status, policy, or SDK operation and
  how to regenerate deterministic artifacts.
- [ ] Finish `m0-baseline.md` with before/after counts, the exact commands run, remaining
  experimental/planned claims, and M1 follow-ups. Do not mark a known M1 failure as green.
- [ ] Run `rg` audits for the superseded memory-only/automation-only product descriptions and for
  unqualified “correct,” “verified,” “alpha,” “production-ready,” and “exactly once” claims.
- [ ] Run the M0 gate commands below and record their results in the completion report.
- [ ] Commit only after `git diff --check`, generated-artifact checks, and `git status --short` show
  no unintended files.

## M0 Gate Commands

```powershell
pwsh ./scripts/check-contracts.ps1
cargo fmt --all -- --check
cargo test -p capsulet-kernel --locked
cargo test -p capsulet-core --locked
cargo test -p capsulet-api --locked
cargo run -p capsulet-api --bin export-openapi --locked -- --check
Push-Location sdk/python
python -m unittest discover -s tests
Pop-Location
Push-Location dashboard
npm test
npx tsc --noEmit
npm run build
Pop-Location
helm lint charts/capsulet
helm template capsulet charts/capsulet
git diff --check
git status --short
```

The M0 completion report must also list repository-wide checks that remain red for known M1 reasons,
such as mandatory PostgreSQL CI, dashboard lint/dependency audit, or environment-specific full
workspace checks. M0 does not hide those failures and does not require solving them to prove its own
contract gate.

## M0 Exit Criteria

M0 is complete only when all of the following are true:

- every declared public product surface is inventoried;
- every implemented public capability or guarantee has executable evidence;
- every other public capability is explicitly experimental or planned;
- the canonical product definition and three-layer model are consistent across current docs and
  package metadata;
- execution state is never documented or presented as an assurance verdict;
- accepted ADRs govern product identity, OpenAPI authority, and compatibility policy;
- generated OpenAPI and runtime method/path sets are exactly equal;
- every OpenAPI operation has a stable operation ID, schemas, parameters, response/error contract,
  security metadata, stability class, and authorization metadata;
- dashboard and Python client operations are checked against OpenAPI;
- one documented local/CI command enforces the complete contract inventory;
- remaining M1-M6 work is visible as limitations or planned claims rather than presented as current
  behavior.

Passing M0 means Capsulet has one vocabulary and one inspectable contract inventory. It does not mean
Capsulet is alpha-ready; it means later engineering can no longer improve or regress the product
behind ambiguous claims.
