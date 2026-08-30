# Correctness-First Agent Workflow Platform Design

## Status

Approved product direction. This document is the product constitution and maturity roadmap for
Capsulet. It defines the product Capsulet is becoming, the language used to describe it, the
architectural boundaries that protect its correctness claims, and the release gates for its first
public alpha.

This is a target-state design, not a description of everything implemented today. Implemented
behavior remains documented in [ARCHITECTURE.md](../../../ARCHITECTURE.md). Detailed implementation
plans must preserve the decisions and gates in this document unless a later ADR explicitly changes
them.

## Product Definition

> **Capsulet is an open-source, self-hosted platform for building and operating AI-agent workflows
> whose outputs carry explicit, machine-verifiable assurance.**

Capsulet combines three systems that are usually separated:

1. A **workflow engine** for durable sequences, branches, joins, loops, retries, events, schedules,
   human gates, and recovery.
2. An **agent platform** in which models, tools, retrieval systems, memory, and people can propose
   and transform work.
3. A **correctness plane** that records obligations, runs deterministic checks, preserves evidence,
   emits certificates, and enforces policy at consequential boundaries.

The design center is not a chatbot and not a generic DAG scheduler with model calls added. It is a
durable computation system in which nondeterministic components may propose work, but acceptance is
owned by explicit contracts and checkers.

## Audience and Distribution

The first customers are developers building AI-agent systems and enterprise platform teams that
need to operate those systems under stronger correctness, security, and audit constraints.

The first distribution is open source and self-hosted. Helm is the primary production installation
path; local development may use Docker Compose. A hosted Capsulet cloud may be built later, but the
open-source product must not be intentionally crippled to create a cloud upsell. Managed identity,
fleet operations, billing, elastic capacity, and managed upgrades are valid future cloud services.

The first public alpha is therefore judged by whether an external developer can install Capsulet,
run a nontrivial verified workflow, understand its verdict, and operate the installation without
private assistance.

## Product Thesis

AI models are useful proposers, planners, extractors, and repair generators. They are not reliable
authorities on whether their own output is correct. Capsulet makes that separation structural:

```text
model, tool, or human
        |
        v
     proposal -----> deterministic verifier(s) -----> verdict + certificate
        |                         |
        |                         v
        +-------------------- evidence and residual obligations
```

The model decides what to try. A verifier decides what the available evidence justifies. Policy
decides where a result with that verdict is allowed to flow.

This does not make every problem mechanically decidable. It makes the boundary between verified
facts, declared assumptions, incomplete checks, and failures visible and enforceable. That is the
core value proposition.

## Principles

### Correctness claims are scoped

Capsulet must never present “the workflow completed” as “the result is correct.” Every correctness
claim identifies the exact contract, inputs, verifier versions, evidence, assumptions, and verdict
to which it applies.

Natural-language meaning, relevance, retrieval completeness, the suitability of an ontology, and
arbitrary program correctness are not generically decidable. A domain pack may supply a compiler,
test suite, scanner, theorem prover, policy engine, differential harness, or human review step that
discharges a specific obligation. Capsulet composes and preserves those results; it does not
silently strengthen them.

### Models are replaceable proposers

No core guarantee depends on a particular model vendor, model size, or prompt. Model and learned
scores may guide search, planning, extraction, prioritization, and repair. They may not be treated
as proof merely because they are confident or because another model agrees.

### Structural safety is mandatory

All workflows receive baseline admission checks regardless of assurance mode: graph validity,
typed ports, declared effects and permissions, bounded retries and loops, time/token/cost budgets,
durable transitions, and provenance requirements. “Observe” means domain obligations are not
enforced; it does not mean malformed or unbounded execution is accepted.

### Execution is permissive; trust propagation is strict

Users may run exploratory work and inspect unverified outputs. Consequential operations such as
publishing a patch, deploying software, writing governed memory, releasing an artifact, or feeding a
protected downstream node can require a minimum assurance verdict through policy.

### Evidence is a durable product object

Evidence, proposals, verifier decisions, approvals, and certificates are immutable or append-only.
Mutable projections may make them easy to query, but must never erase the history required to
explain or replay a decision.

### Domain independence comes from protocols, not vagueness

Capsulet should work across programming languages and problem domains by defining an extensible
typed intermediate representation and verifier protocol. It should not begin by inventing a novel
universal programming language or by putting domain-specific exceptions in the runtime kernel.

## Non-Goals

- Capsulet does not guarantee that every vulnerability is found, every migration preserves all
  intended behavior, or arbitrary code is safe.
- Capsulet does not treat a successful model call, scanner invocation, test run, or human approval
  as stronger evidence than the declared contract allows.
- Capsulet does not make a visual editor the source of truth. Visualizations are projections of the
  same versioned workflow definition used by the API and SDKs.
- Capsulet does not allow an agent to invoke undeclared tools, effects, providers, or network access.
- Capsulet does not require every workload to use AI. Deterministic workflows are a first-class
  subset of the same runtime.
- The initial open-source alpha does not include a managed multi-tenant cloud control plane,
  billing, or a proprietary marketplace.

## Canonical Vocabulary

| Term | Meaning |
| --- | --- |
| **Workflow definition** | A versioned typed graph, including nested regions, contracts, policies, budgets, and provider/tool bindings. |
| **Run** | One durable execution of an immutable workflow-definition version against pinned inputs and configuration. |
| **Proposal** | A candidate value or state transition plus the derivation and evidence claimed to justify it. It is never silently promoted to accepted output. |
| **Contract** | A machine-readable statement of input requirements, output properties, allowed effects, and proof obligations. |
| **Obligation** | A specific proposition that must be discharged, explicitly assumed, waived by authorized policy, or left residual. |
| **Verifier** | A deterministic checker or a declared external oracle that evaluates obligations and returns structured evidence. |
| **Domain pack** | Versioned contracts, schemas, verifiers, adapters, policies, and presentation metadata for a problem domain. |
| **Evidence** | Content-addressed material used by a verifier: source spans, logs, test results, scanner reports, signatures, artifacts, or approvals. |
| **Verdict** | The assurance result: `unverified`, `accepted`, `conditional`, or `rejected`. |
| **Certificate** | The immutable, replayable record connecting inputs, contract, verifier versions, evidence, assumptions, discharged obligations, residual obligations, and verdict. |
| **Protected boundary** | A side effect or trust transition whose policy requires a minimum verdict before it may occur. |
| **Trust class** | A type-level description of the assurance attached to a value, such as `Unverified` or `Verified<ContractId>`. |

“Agent,” “workflow,” and “automation” are not competing runtime abstractions. An agent is a workflow
whose next actions may be selected dynamically by a constrained planner. An automation binds a
trigger and policy to a workflow. Deterministic pipelines, agent loops, and human review processes
all compile to the same workflow IR.

## Runtime and Correctness Architecture

### Planes

The target architecture separates five cooperating planes:

```text
Authoring plane       API, SDKs, CLI, UI, versioning, validation
        |
Control plane         admission, scheduling, policy, identity, budgets
        |
Execution plane       durable graph worker, tool/model/job adapters, effects
        |
Correctness plane     obligations, verifiers, evidence, certificates, trust
        |
Data plane            PostgreSQL metadata/events, object artifacts, governed memory
```

These are architectural responsibilities, not necessarily one service each. The domain and
application layers own invariants and use cases; HTTP, PostgreSQL, object storage, model providers,
Kubernetes, and verifier processes remain adapters.

### Typed verified-computation IR

The workflow IR is the common substrate for deterministic jobs, agents, and automations. Version 1
must represent:

- structural value schemas rather than nominal tags around opaque JSON;
- typed input and output ports, including trust classes;
- nodes with declared capabilities, effects, resources, providers, and idempotency behavior;
- hyperedges that combine or distribute values without losing provenance;
- sequence, parallel branches, joins, conditions, and nested regions;
- explicit proposals, contracts, obligations, evidence, verdicts, artifacts, and certificates;
- durable wait points for events, timers, and human decisions;
- retry, timeout, cancellation, compensation, and escalation policies;
- bounded loops with explicit state and progress conditions;
- schema and semantic version identifiers on every persisted definition.

An escape hatch for opaque values may exist only when its lost guarantees are reflected in the
trust class and certificate. `Json` must not make type compatibility or provenance claims vacuous.

### Workflow semantics

The runtime supports both statically selected and dynamically selected transitions. A planner may
choose only among actions admitted by the workflow definition and current policy. It cannot create
new capabilities by emitting their names as text.

The durable unit of progress is a committed transition with an idempotency key, input digests,
output references, event position, and ownership record. A worker crash may cause safe replay or
reattachment, but must not cause an externally visible effect to be silently duplicated.

Execution status and assurance verdict are independent dimensions:

| Execution status | Meaning |
| --- | --- |
| `queued` | Admitted and waiting for execution. |
| `running` | At least one transition is active or ready to advance. |
| `waiting` | Durably suspended for a timer, event, approval, or external condition. |
| `completed` | Execution reached a normal terminal state. |
| `failed` | Execution could not reach its requested terminal state. |
| `cancelled` | An authorized cancellation ended execution. |

| Verification verdict | Meaning |
| --- | --- |
| `unverified` | Domain obligations were not evaluated. No correctness inference is permitted. |
| `accepted` | Every obligation required by the governing policy was mechanically discharged. |
| `conditional` | The result is justified only under named assumptions or residual obligations. |
| `rejected` | At least one required premise or obligation failed. |

A run can therefore be `completed + rejected`, `completed + conditional`, or `failed + unverified`.
Dashboards, APIs, SDKs, metrics, and automation conditions must not collapse these dimensions.

### Loops as first-class regions

Loops are not hidden inside a model prompt or an unbounded node implementation. A loop region
declares:

- typed entry state and exit values;
- its body graph and continuation condition;
- maximum iterations, wall time, tokens, cost, and effect budgets;
- invariants checked before and after each iteration;
- a progress or termination measure when the domain can provide one;
- permitted repair and escalation transitions;
- evidence and verdicts for each iteration.

The runtime preserves the iteration history and makes non-progress visible. Exhausting a budget is
an explicit stop reason, not successful completion. An invariant failure routes to a typed repair,
rejection, or escalation path defined by policy.

### Assurance modes

Every workflow or protected subgraph selects an assurance policy:

| Mode | Behavior |
| --- | --- |
| **Observe** | Execute after mandatory structural admission, capture evidence, and label outputs `unverified`. |
| **Verify** | Evaluate declared obligations and emit `accepted`, `conditional`, or `rejected`; callers decide how to use the result. |
| **Enforce** | Evaluate obligations and prevent protected boundaries unless the policy's minimum verdict is met. |

Assurance may strengthen along a typed edge:

```text
Artifact<Unverified>
    + Contract<C>
    + VerificationRecord<C>
    -> Artifact<Verified<C>>
```

It must not strengthen through a cast, UI action, model assertion, or score. Derived artifacts
inherit the weakest relevant trust unless a declared verifier establishes a stronger contract for
the derivation.

### Propose, check, certify

Node executors return proposals. Admission to trusted state is a runtime rule rather than an
optional validator node. The pure kernel validates locally decidable rules with no model and no
network. External verifiers run through a restricted protocol and are treated as named oracles:
their identity, version, environment, input/output digests, and trust policy appear in the
certificate.

Offline certificate replay means the kernel can recompute the same verdict from the pinned
certificate inputs and evidence without calling a model or the network. It does not mean every
nondeterministic proposal or external tool execution can be recreated byte-for-byte.

### Failure and repair routing

Failures are typed by owning subsystem. Examples include missing evidence, stale reference,
schema mismatch, arithmetic mismatch, verifier unavailable, policy denial, budget exhaustion,
non-progress, unsafe effect, and human escalation required.

A failure type maps to an allowed repair route. Retrieval failures may invoke retrieval again;
arithmetic mismatches use the checker's result; transient verifier failures may retry within
budget; policy denials require a policy-authorized change; semantic residuals may require a human
or independent review. “Ask the model to try again” is not the universal recovery mechanism.

## Policy, Security, and Audit

Policy is versioned and evaluated as code. It can constrain:

- allowed model and tool providers, container images, and verifier identities;
- network, filesystem, secret, data-residency, and execution capabilities;
- token, cost, time, concurrency, and retry budgets;
- required contracts, verifiers, approvals, signatures, and minimum verdicts;
- who may author, run, approve, waive, publish, or administer resources;
- which memory spaces or downstream nodes may receive a value of a given trust class.

Project-scoped IAM is enforced at the API and persistence boundaries, never by dashboard filtering.
Service accounts are project-scoped by default. Cross-project access, artifact download, trace
inspection, certificate access, verifier execution, and policy management all require explicit
authorization.

The audit history records the actor or service principal, tenant and project, request identity,
definition and policy versions, proposals, tool/provider calls, effects, state transitions,
verifier decisions, waivers, approvals, artifact digests, publication actions, and outcomes.
Secrets and sensitive payloads are referenced or redacted according to policy without breaking the
cryptographic link to their protected evidence.

Execution of untrusted code uses isolated workers with least privilege, explicit capabilities,
resource limits, no control-plane credentials, network policy, and an operator-selected sandboxed
runtime where the threat model requires a VM-grade boundary. Verifiers must be isolated from
proposers so a candidate cannot rewrite its checker or evidence.

## Governed Memory

Capsulet's claim-first memory graph remains an important subsystem, not the whole product identity.
It provides durable sources, evidence, entities, claims, relationships, conflicts, contracts,
nested contexts, review state, and provenance for workflows that need governed knowledge.

Memory writes are protected effects. A proposed claim cannot become trusted memory merely because
an ingestion model produced it. Its source span, interpretation residuals, review state, contract,
and admission verdict travel with it. Workflows that do not need graph memory are not required to
adopt its ontology; they still use the shared artifact, evidence, and certificate protocol.

## Representative Workflow: Security Remediation

A repository security automation demonstrates the intended composition without making security
scanning a hard-coded product special case:

```text
repository event
  -> pin source revision and policy
  -> run declared scanners in parallel
  -> agent correlates findings and proposes a patch
  -> compile + lint + tests + scanner replay
  -> evaluate release policy and residual obligations
  -> optional human approval
  -> publish pull request only if the protected-boundary policy allows it
```

The certificate does not say “the repository is secure.” It can say, for example, that a particular
patch against a particular revision compiled, passed named tests, and had no findings under named
scanner rules and versions, while retaining any coverage assumptions and untested obligations.

The same runtime and protocol should support unrelated workloads such as data-quality remediation,
evidence-grounded research, or legacy-system migration. Domain packs supply the specific contracts
and verifiers.

## Developer Experience

The API is the authoritative remote interface. Rust domain types and OpenAPI schemas define the
wire contract; Python and future SDKs provide typed authoring layers. The CLI supports authoring,
validation, execution, inspection, replay, certificate verification, and administrative workflows.

The dashboard must make the system understandable, not merely attractive. Its primary views are:

- workflow graph and version history;
- live control flow and data flow;
- loop iterations, budgets, waits, retries, and compensations;
- artifact lineage and trust transitions;
- obligation, evidence, assumption, and verdict inspection;
- certificate replay and comparison;
- model, tool, verifier, policy, and human decisions;
- governed-memory revisions and source provenance;
- identity, permissions, audit, and operational health.

An operator should be able to explain a verdict from the product surface without querying database
tables. UI labels must distinguish implemented behavior, simulated/demo data, and planned features.

## Compatibility and Evolution

Workflow definitions, contracts, certificates, trace events, domain packs, policy documents, and
API payloads are versioned. Persisted definitions are immutable; changes create new versions. Runs
pin every definition and dependency version they consume.

Before alpha, internal APIs may change rapidly, but migrations and examples must remain deterministic.
At alpha:

- public APIs and SDK surfaces are explicitly labeled stable, experimental, or internal;
- stable alpha APIs receive documented deprecation and migration paths;
- database migrations are forward-only, tested from every supported release, and covered by
  backup/restore and rollback procedures;
- the IR and certificate formats use explicit schema versions and compatibility readers;
- domain packs declare compatible runtime and protocol ranges;
- release artifacts, container images, Helm charts, and packs are signed and reproducible enough to
  trace back to source and build metadata.

Alpha does not promise a permanently frozen API. It promises that change is intentional, visible,
tested, and accompanied by a viable migration path.

## Relationship to Existing Designs

This document sets product scope and precedence; it does not discard the useful detailed work in
earlier designs.

| Existing document | Relationship |
| --- | --- |
| [Correctness architecture](../../design/correctness-architecture.md) and [ADR 0012](../../adr/0012-correctness-kernel-and-proposer-checker-split.md) | Retained as the proposer/checker/certificate foundation and natural-language verifiability boundary. This design generalizes it to all workflow artifacts and adds assurance modes, protected boundaries, and trust propagation. |
| [Typed Agent RAG Runtime Design](2026-06-29-typed-agent-rag-runtime-design.md) | Retained for typed hypergraphs, budgets, constrained planners, traces, and provider boundaries. Superseded where it permits opaque JSON state, static-only execution, or optional correctness through validator nodes. RAG becomes one domain composition, not the product identity. |
| [Hybrid Project IAM Design](2026-06-24-hybrid-project-iam-design.md) | Retained and expanded to policies, certificates, artifacts, verifiers, approvals, and cross-project trust boundaries. |
| [Backend DDD Refactor Design](2026-06-26-backend-ddd-refactor-design.md) | Retained as the code-boundary strategy. Domain invariants remain pure; application services orchestrate; infrastructure remains behind ports. |
| [Production Readiness Plan](../plans/2026-06-22-production-readiness.md) | Treated as a source of reliability and security requirements. Incomplete items must be revalidated against the milestone gates below rather than assumed complete. |
| [MVP Implementation Plan](../../design/mvp-implementation-plan.md) | Historical. Its workflow/job MVP is useful compatibility infrastructure but no longer defines the product destination or release gate. |

If an earlier document conflicts with the vocabulary, assurance semantics, or release gates here,
this document wins until an ADR records a replacement decision.

## Maturity Roadmap

The roadmap is dependency-ordered, not date-driven. Passing a milestone requires its gate to remain
green in subsequent milestones. The goal is a credible mature foundation, not the quickest release
that can be called alpha.

### M0 — Product constitution and contract inventory

Deliver:

- adopt this product definition and canonical vocabulary across repository documentation;
- inventory every public product claim and map it to an executable test, an implemented fact, or a
  clearly marked future/experimental label;
- document execution-status versus verification-verdict semantics;
- establish API, IR, certificate, domain-pack, migration, and deprecation policies;
- reconcile architecture documents and record material decisions as ADRs;
- make OpenAPI the complete public HTTP contract and define SDK generation/verification rules.

Gate: every public claim is evidence-backed or visibly labeled as future work, and one source of
truth defines every public term and lifecycle.

### M1 — Engineering integrity

Deliver:

- one documented clean-checkout command path for formatting, linting, unit, integration, security,
  SDK, dashboard, migration, Compose, Helm, and cluster checks;
- mandatory PostgreSQL integration tests with no silent skip when infrastructure is absent;
- complete route/OpenAPI parity and coverage checks that cannot pass over an incomplete route list;
- deterministic fixtures, migrations, seeds, and end-to-end test identities;
- dependency and supply-chain policy, vulnerability scanning, secret scanning, SBOMs, and signed CI
  provenance;
- coherent IAM scopes and tenant/project ownership across every current resource;
- a single release/version policy for Rust crates, APIs, SDKs, images, and charts.

Gate: a clean checkout passes the supported verification command, CI cannot silently omit
persistence tests, and every public route is documented and authorization-tested.

### M2 — Verified computation IR v1

Deliver:

- structural value types, effects, capabilities, nested regions, branches, joins, and bounded loops;
- immutable proposal, evidence, artifact, obligation, verdict, and certificate models;
- admission-controlled state transitions and trust-typed edges;
- assurance policies for Observe, Verify, and Enforce;
- versioned schemas and deterministic serialization/digests;
- a small pure correctness kernel and certificate replay tool;
- explicit adapters from existing job DAGs, agent graphs, and governed memory into the IR.

Gate: given pinned inputs and stored evidence, the kernel replays a certificate offline and produces
the same verdict without a model or network connection.

### M3 — Durable graph runtime

Deliver:

- a dedicated graph worker independent of synchronous API requests;
- durable branches, joins, loops, timers, events, nested workflows, human gates, and suspension;
- cancellation, retry, timeout, compensation, escalation, and budget semantics;
- idempotent or explicitly non-idempotent effect handling with ownership-safe finalization;
- checkpoint recovery, worker reattachment, non-progress detection, and append-only event history;
- scheduler/evaluator convergence on workflow runs rather than competing orchestration paths.

Gate: during a complex looping workflow, any control-plane process can be killed and restarted
without losing committed state, duplicating protected effects, or corrupting its certificate chain.

### M4 — Verifier and domain ecosystem

Deliver:

- a stable validator SDK and process/container protocol;
- isolated verifier execution with declared inputs, outputs, capabilities, and trust policy;
- signed, versioned domain packs and a local development harness;
- adapters for parsers, compilers, test runners, policy engines, scanners, and proof tools;
- contract composition, obligation routing, waivers, and human-review integration;
- reference packs for at least three unrelated workloads.

Gate: three unrelated workloads use the same IR and certificate protocol without domain-specific
exceptions in the kernel or runtime.

### M5 — Interpretability product

Deliver:

- authoring and inspection for versioned workflow graphs;
- live control-flow and data-flow views, including durable waits and loop history;
- end-to-end artifact lineage and trust-class visualization;
- obligation, evidence, residual, policy, approval, and certificate inspectors;
- replay and certificate diff tools in the dashboard and CLI;
- memory revision/provenance views connected to workflow evidence;
- stable Python SDK coverage and generated or contract-tested clients for public APIs.

Gate: a developer can explain why a result has its verdict, what remains assumed, and what changed
between two runs without direct database access.

### M6 — Self-hosting maturity

Deliver:

- functional dashboard identity and bootstrap paths in the default supported Helm installation;
- coherent project IAM and policy administration;
- signed images and charts with consistent names, versions, and upgrade documentation;
- tested installation, upgrade, rollback, backup, restore, and disaster-recovery procedures;
- control-plane and verifier isolation, network policy, secret rotation, and secure defaults;
- horizontal scaling and high-availability behavior for supported components;
- metrics, traces, alerts, SLOs, capacity guidance, retention, and incident runbooks;
- a real local Kubernetes smoke path that does not substitute Docker Compose execution.

Gate: automated tests cover fresh install, supported-version upgrade, worker loss, backup restore,
and cross-project isolation on the supported Kubernetes matrix.

### Public alpha release gate

The public alpha is cut only after M0-M6 gates pass together. It includes:

- versioned alpha APIs and certificate/IR formats with compatibility policy;
- a reproducible Helm installation and signed release artifacts;
- end-to-end examples and a domain-pack template;
- security policy, threat model, support matrix, contribution guide, and limitations document;
- telemetry disabled by default and no hidden dependency on a Capsulet-operated service;
- an external-user acceptance test from installation through a verified looping workflow and
  inspectable certificate.

Alpha entrance criterion: an external developer, using only published documentation, can install
Capsulet, execute a verified looping workflow, recover it from an injected failure, inspect its
evidence and residual obligations, and export or replay its certificate.

### Beta, 1.0, and managed cloud

Beta focuses on performance envelopes, ecosystem feedback, upgrade experience, broader domain-pack
coverage, and API stabilization. Version 1.0 requires a declared long-term compatibility window,
operational evidence from real installations, security review, and measured reliability objectives.

A managed cloud follows the self-hosted product rather than redefining it. Cloud-specific work
includes tenant isolation, managed identity and keys, billing and quotas, regional data placement,
fleet operations, managed upgrades, and hosted verifier capacity. Certificates and workflows remain
portable between self-hosted and managed deployments.

## Current Baseline and Immediate Implications

The repository already contains useful foundations: a Rust domain/application/adapters split,
PostgreSQL durability, object storage, Kubernetes execution, workflow DAGs, typed graph definitions,
agent run state and traces, governed memory, a correctness kernel slice, IAM design, a dashboard,
Python SDK, Compose, and Helm.

The 2026-08-30 repository audit also found conditions that prevent calling the current tree a public
alpha:

- agent-run creation persists queued work but lacks a production dedicated agent/graph worker;
- current graph execution is static and moves opaque JSON state despite nominal port typing;
- PostgreSQL-critical tests may skip when no database URL is present, and CI does not supply the
  required database service;
- the OpenAPI parity check covers only a subset of routes;
- IAM roles/scopes and project ownership are inconsistent across newer agent, graph, memory, and
  ingestion paths;
- the default Helm dashboard lacks a complete usable authentication/bootstrap path, chart image
  naming diverges from published images, and the documented local Kubernetes smoke path does not
  independently exercise Kubernetes;
- dashboard lint/dependency security checks are not green even though unit tests, type checking, and
  production build pass;
- some UI data and labels are demonstrative or misleading, and at least one project-scoped artifact
  path omits required project context.

These are not isolated polish tasks. M0 and M1 must turn them into explicit, continuously enforced
contracts before deeper runtime work can make reliable progress. Existing functionality should be
preserved where it satisfies the target model, but compatibility code must not dictate the new IR.

## Success Metrics

Product maturity is measured by properties, not feature count:

- percentage of public claims mapped to executable evidence;
- percentage of runs with complete provenance and replayable certificates;
- accepted, conditional, rejected, and unverified verdict distribution by domain pack;
- residual-obligation count and age;
- protected-effect duplicate rate under fault injection (target: zero);
- recovery success and time after worker/control-plane loss;
- clean-install, upgrade, restore, and isolation pass rates;
- external-user time from installation to first verified result;
- policy bypass, cross-project access, and unverifiable trust-strengthening incidents (target: zero).

The accepted-to-conditional ratio is especially important. If useful workloads accumulate long,
unreviewable residuals, Capsulet may be an excellent provenance system but not yet the correctness
system it claims to be. That outcome must be measured and reported honestly.

## Decision Summary

- Capsulet's product identity is a correctness-first AI-agent workflow platform.
- The open-source, self-hosted Helm distribution comes before managed cloud.
- One typed workflow IR represents deterministic workflows, agent workflows, and automations.
- Models propose; deterministic checkers and declared oracles justify; policy governs release.
- Execution status and assurance verdict are independent.
- Observe, Verify, and Enforce are explicit assurance modes; structural admission always applies.
- Loops, durable waits, human gates, and protected boundaries are first-class runtime concepts.
- Certificates preserve exact inputs, versions, evidence, assumptions, residuals, and verdicts.
- Governed memory is a major subsystem, not the complete product identity.
- Public alpha follows mature engineering, runtime, verifier, interpretability, and self-hosting gates.
