# Product Claims

This file is generated from `docs/contracts/product-claims.json`. Do not edit it directly.

## Automations

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-AUTOMATION-001` | experimental | capability | Automations can consume manual, cron, read-only SQL, signed webhook, and isolated custom-plugin triggers to create compatibility workflow runs. |

## Correctness kernel

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-CORRECTNESS-001` | implemented | capability | The deterministic kernel checks pinned proposals and emits accepted, conditional, or rejected certificates without calling a model or network. |
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
| `CAP-GRAPH-001` | implemented | capability | Capsulet validates typed agent graph definitions with nodes, ports, hyperedges, transition policy, budgets, and deterministic static ordering. |

## Helm and self-hosting

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-HELM-001` | experimental | capability | An alpha-readiness Helm chart installs the current control plane, execution services, dashboard, and optional bundled dependencies, but is not yet a public-alpha distribution. |

## HTTP API

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-OPENAPI-001` | implemented | limitation | The current handwritten OpenAPI document is incomplete and is not yet an exact contract for the running HTTP API. |

## Identity and IAM

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-IAM-001` | experimental | capability | Bearer and OIDC authentication, roles/scopes, project selection, memberships, service accounts, and durable mutation audits exist, with known ownership inconsistencies on newer resources. |

## Jobs and runners

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-JOB-001` | implemented | capability | The job worker leases queued work, renews ownership, executes through stub, process, WASI Python, or Kubernetes adapters, and stores logs and artifacts. |

## Observability and operations

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-OBSERVABILITY-001` | experimental | capability | Services expose health, readiness, Prometheus metrics, structured logs, and starter dashboards and alerts for current execution paths. |

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

## Workflow compatibility

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-WORKFLOW-001` | implemented | compatibility | Compatibility workflow DAGs support validated dependencies, parallel roots, fan-out/fan-in scheduling, durable step state, cancellation, and checkpoint resume. |
