# Product Claims

This file is generated from `docs/contracts/product-claims.json`. Do not edit it directly.

## Assurance policy

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-ASSURANCE-001` | implemented | capability | Under enforce, a protected boundary is denied when no certificate exists, because an absent certificate is unverified and unverified never satisfies a higher minimum. |
| `CAP-ASSURANCE-002` | implemented | capability | A certificate's verdict is derived from its obligations under its recorded mode; a certificate recording a verdict its obligations do not justify cannot be sealed. |

## Automations

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-AUTOMATION-001` | experimental | capability | Automations can consume manual, cron, read-only SQL, signed webhook, and isolated custom-plugin triggers to create compatibility workflow runs. |

## Correctness kernel

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-CORRECTNESS-001` | implemented | capability | The deterministic kernel accepts a pinned citation when it re-derives and contains the cited proposition. |
| `CAP-CORRECTNESS-002` | implemented | limitation | Current kernel certificates are an isolated slice; the runtime does not yet admission-control all protected effects or represent platform-level unverified assurance. |

## Dashboard and SDK

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-DASHBOARD-001` | experimental | capability | The Next.js dashboard provides authenticated operational and governed-memory views backed by handwritten API calls. |
| `CAP-DASHBOARD-002` | implemented | limitation | Some dashboard overview data, settings, and future controls are demonstrative or static and are not implemented runtime behavior. |
| `CAP-SDK-001` | implemented | compatibility | The experimental Python SDK compiles decorated Python functions into compatibility workflow and job API payloads. |

## Governed memory

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-MEMORY-001` | experimental | capability | Governed memory models sources, evidence, entities, claims, conflicts, contracts, ingestion review, nested subgraphs, and provenance as a major subsystem. |

## Graphs and agents

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-AGENT-001` | experimental | capability | Agent definitions and queued agent runs, opaque JSON state snapshots, and semantic trace events can be persisted and exercised through the application runtime. |
| `CAP-AGENT-002` | implemented | limitation | There is no production dedicated agent or graph worker; creating an agent run persists queued work but does not independently execute it. |
| `CAP-GRAPH-001` | implemented | capability | Capsulet returns a deterministic static order for valid acyclic typed agent graph definitions. |

## Helm and self-hosting

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-HELM-001` | experimental | capability | An alpha-readiness Helm chart installs the current control plane, execution services, dashboard, and optional bundled dependencies, but is not yet a public-alpha distribution. |

## HTTP API

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-OPENAPI-001` | implemented | capability | The OpenAPI 3.1 document is generated from typed endpoint metadata and actual Rust wire schemas, exactly matches runtime registration, validates through utoipa, and is checked for deterministic drift. |

## Identity and IAM

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-IAM-001` | experimental | capability | Bearer and OIDC authentication, roles/scopes, project selection, memberships, service accounts, and durable mutation audits exist, with known ownership inconsistencies on newer resources. |

## IR adapters

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-ADAPTERS-001` | implemented | capability | Today's job DAGs and agent graphs translate into the IR and pass structural admission, and a dependency cycle the current model rejects is refused by admission too. |

## Jobs and runners

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-JOB-001` | experimental | capability | The job worker leases queued work, renews ownership, executes through stub, process, WASI Python, or Kubernetes adapters, and stores logs and artifacts. |

## Lifecycle and assurance

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-LIFECYCLE-001` | implemented | compatibility | Execution status and assurance verdict are independent; every current persisted status is explicitly mapped without treating successful execution as verified output. |

## Observability and operations

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-OBSERVABILITY-001` | experimental | capability | Services expose health, readiness, Prometheus metrics, structured logs, and starter dashboards and alerts for current execution paths. |

## Offline certificate replay

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-REPLAY-001` | implemented | capability | A certificate bundle replays offline to its recorded verdict, and one changed byte of evidence makes replay report rejected instead. |
| `CAP-REPLAY-002` | implemented | capability | The replay binary cannot reach a database, an HTTP client, an async runtime, or a model provider, asserted over its resolved dependency closure. |

## Persistence and storage

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-PERSISTENCE-001` | experimental | capability | PostgreSQL stores control-plane and execution metadata, while filesystem or S3-compatible object storage stores scripts, large logs, and artifacts. |

## Product identity

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-PRODUCT-001` | planned | positioning | Capsulet is a correctness-first AI-agent workflow platform for building and operating workflows whose important outputs can be inspected, checked, and governed. |

## Security and isolation

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-SECURITY-001` | implemented | limitation | Capsulet constrains execution but does not claim a complete sandbox for hostile code; production isolation depends on operator-selected Kubernetes controls and runtime classes. |

## Verified computation IR

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-IR-001` | implemented | capability | Two structurally equal IR documents produce identical canonical bytes and therefore one digest, and floating point is refused before any digest is computed. |
| `CAP-IR-002` | implemented | capability | A value's trust class cannot be strengthened by assertion: a document claiming a verdict its verification record does not justify is refused. |
| `CAP-IR-003` | implemented | capability | Structural admission applies in every assurance mode, including observe, and returns a decision for every definition without panicking. |
| `CAP-IR-004` | implemented | capability | A loop must declare finite bounds, and exhausting a budget is reported as a stop reason rather than as completion. |

## Workflow compatibility

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-WORKFLOW-001` | implemented | compatibility | Compatibility workflow DAG creation validates and returns declared step dependencies. |
