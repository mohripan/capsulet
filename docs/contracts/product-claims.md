# Product Claims

This file is generated from `docs/contracts/product-claims.json`. Do not edit it directly.

## Assurance decisions

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-ASSURANCE-003` | implemented | capability | A boundary's required contract is satisfied by covering it, not by naming it: the gate reads the contract's obligations from the definition and denies the crossing when the certificate leaves any of them unaccounted for, naming which. |
| `CAP-ASSURANCE-004` | implemented | capability | A boundary is judged on the contract it names: an undecided obligation of another contract does not deny the crossing, while an obligation that was checked and failed denies it wherever it sits, and a contract the certificate says nothing about is unverified. |
| `CAP-ASSURANCE-005` | implemented | capability | A required verifier is satisfied by a run, not by a name: a verifier that concluded rejected, or ran at a version or in an environment the policy did not pin, does not meet the requirement. |

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
| `CAP-CORRECTNESS-003` | implemented | capability | The kernel decides every proposal it is given: a derivation nested past the depth bound it states is rejected with that reason, rather than exhausting the stack. |
| `CAP-CORRECTNESS-004` | implemented | limitation | A cited term is matched by substring containment within a bounded span, so a short term is grounded by a longer word that contains it; matching on word boundaries needs Unicode segmentation, because a flanking-character rule would reject correct citations in scripts written without spaces. |
| `CAP-CORRECTNESS-005` | implemented | capability | A citation is judged on text, not bytes: Unicode composition, case and whitespace run-length do not change whether a document says something, and a span too large to point at anything in particular is not a citation. |

## Crash recovery

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-RUNTIME-004` | implemented | capability | Killing the worker at every step boundary of a run that drives a bounded loop and performs a protected effect leaves no duplicated effect, no repeated or lost iteration, no lost committed state, and a certificate that replays. |

## Dashboard and SDK

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-DASHBOARD-001` | experimental | capability | The Next.js dashboard provides authenticated operational and governed-memory views backed by handwritten API calls. |
| `CAP-DASHBOARD-002` | implemented | limitation | Some dashboard overview data, settings, and future controls are demonstrative or static and are not implemented runtime behavior. |
| `CAP-SDK-001` | implemented | compatibility | The experimental Python SDK compiles decorated Python functions into compatibility workflow and job API payloads. |

## Durable run state

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-RUNTIME-001` | implemented | capability | A run's state is a fold over an append-only event log, and the status the database reports is a projection of that log rather than a second answer. |

## Fencing

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-RUNTIME-002` | implemented | capability | A worker that lost its lease cannot append to the run it thought it owned; the write is refused and the worker stops. |

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

## HTTP API

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-OPENAPI-001` | implemented | capability | The OpenAPI 3.1 document is generated from typed endpoint metadata and actual Rust wire schemas, exactly matches runtime registration, validates through utoipa, and is checked for deterministic drift. |

## Helm and self-hosting

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-HELM-001` | experimental | capability | An alpha-readiness Helm chart installs the current control plane, execution services, dashboard, and optional bundled dependencies, but is not yet a public-alpha distribution. |

## IR adapters

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-ADAPTERS-001` | implemented | capability | Today's job DAGs and agent graphs translate into the IR and pass structural admission, and a dependency cycle the current model rejects is refused by admission too. |

## Identity and IAM

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-IAM-001` | experimental | capability | Bearer and OIDC authentication, roles/scopes, project selection, memberships, service accounts, and durable mutation audits exist, with known ownership inconsistencies on newer resources. |

## Jobs and runners

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-JOB-001` | experimental | capability | The job worker leases queued work, renews ownership, executes through stub, process, WASI Python, or Kubernetes adapters, and stores logs and artifacts. |

## Lifecycle and assurance

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-LIFECYCLE-001` | implemented | compatibility | Execution status and assurance verdict are independent; every current persisted status is explicitly mapped without treating successful execution as verified output. |

## Loop budgets

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-RUNTIME-005` | implemented | capability | What a loop spent before a crash is still spent after it, and a measure that stopped moving — or moved against its declared direction — stops the loop. |

## Loops the worker drives

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-RUNTIME-008` | implemented | capability | The graph worker runs a declared loop until its continuation goes false or its iteration budget refuses another round, and a node after the loop waits for the loop rather than for one iteration. |

## Observability and operations

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-OBSERVABILITY-001` | experimental | capability | Services expose health, readiness, Prometheus metrics, structured logs, and starter dashboards and alerts for current execution paths. |

## Offline certificate replay

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-REPLAY-001` | implemented | capability | A certificate bundle replays offline to its recorded verdict, and one changed byte of evidence makes replay report rejected instead. |
| `CAP-REPLAY-002` | implemented | capability | The replay binary cannot reach a database, an HTTP client, an async runtime, or a model provider, asserted over its resolved dependency closure. |

## Once-only effects

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-RUNTIME-003` | implemented | capability | A protected effect is claimed before it is attempted, and an effect whose outcome nobody can determine stops the run when the IR declares it must not be repeated. |

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

## Starting a run

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-RUNTIME-007` | implemented | capability | A registered IR definition version can be enqueued as a run over the API, and the run is created queued and unleased so the graph worker is what advances it. |

## Stopping safely

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-RUNTIME-006` | implemented | capability | Cancellation stops a run at a point where no effect is in flight, and a reversible effect that happened is undone before the run ends. |

## Verified computation IR

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-IR-001` | implemented | capability | Two structurally equal IR documents produce identical canonical bytes and therefore one digest, and floating point is refused before any digest is computed. |
| `CAP-IR-002` | implemented | capability | A value's trust class cannot be strengthened by assertion: establishing one requires a verification record admitted against a certificate that exists and covers the contract claimed, and the verdict and residual count are read from that certificate rather than from the document. |
| `CAP-IR-003` | implemented | capability | Structural admission applies in every assurance mode, including observe, and returns a decision for every definition without panicking. |
| `CAP-IR-004` | implemented | capability | A loop must declare finite bounds, and exhausting a budget is reported as a stop reason rather than as completion. |
| `CAP-IR-006` | implemented | limitation | Whether a value reached a verification record without crossing an unmodelled boundary is supplied by the caller, not derived: no certificate records provenance loss, so that one input to the verified trust class is taken on trust. |

## Workflow compatibility

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-WORKFLOW-001` | implemented | compatibility | Compatibility workflow DAG creation validates and returns declared step dependencies. |
