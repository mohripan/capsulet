# Product Claims

This file is generated from `docs/contracts/product-claims.json`. Do not edit it directly.

## Assurance decisions

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-ASSURANCE-003` | implemented | capability | A boundary's required contract is satisfied by covering it, not by naming it: the gate reads the contract's obligations from the definition and denies the crossing when the certificate leaves any of them unaccounted for, naming which. |
| `CAP-ASSURANCE-004` | implemented | capability | A boundary is judged on the contract it names: an undecided obligation of another contract does not deny the crossing, while an obligation that was checked and failed denies it wherever it sits, and a contract the certificate says nothing about is unverified. |
| `CAP-ASSURANCE-005` | implemented | capability | A required verifier is satisfied by a run, not by a name: a verifier that concluded rejected, or ran at a version or in an environment the policy did not pin, does not meet the requirement. |
| `CAP-ASSURANCE-006` | implemented | limitation | A certificate does not record which verdict rule decided it, so a change to that rule would make already-sealed certificates fail to deserialize; the rule is pinned by test rather than by the document, because recording it on the sealed body would break every existing seal. |
| `CAP-ASSURANCE-007` | implemented | capability | A certificate can go stale or be withdrawn: a boundary may require the evidence behind it to be no older than a stated age, one that cannot be dated is not fresh, and a revoked certificate is refused however well formed it is. Time and revocations are arguments to the decision, never read inside it. |
| `CAP-ASSURANCE-008` | implemented | capability | Every field of an assurance policy is accounted for by a test that shows it changing a decision, or documented as descriptive; a field added without either stops the check compiling. |

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
| `CAP-CORRECTNESS-001` | implemented | capability | The deterministic kernel accepts a trusted pinned citation when its excerpt re-derives byte for byte from the stored source, states the proposition in a phrasing its contract declares, meets the authority floor, and leaves no unresolved interpretation or typed-value reading. |
| `CAP-CORRECTNESS-002` | implemented | limitation | Current kernel certificates are an isolated slice; the runtime does not yet admission-control all protected effects or represent platform-level unverified assurance. |
| `CAP-CORRECTNESS-003` | implemented | capability | The kernel decides every proposal it is given: a derivation nested past the depth bound it states is rejected with that reason, rather than exhausting the stack. |
| `CAP-CORRECTNESS-004` | implemented | capability | A citation finds an entity under another name only when confirmed entity resolutions join the two names under one canonical entity, and the discharged step names the resolutions it relied on. Unreviewed aliases and candidate or rejected resolutions join nothing, and a typed value has no other names. |
| `CAP-CORRECTNESS-005` | implemented | capability | A citation is judged on text, not bytes: Unicode composition, case (by full case folding), typographic quotes and dashes, invisible characters and whitespace run-length do not change whether a document says something; a term is found only as whole words, including in scripts written without spaces; and a span too large to point at anything in particular is not a citation. |
| `CAP-CORRECTNESS-006` | implemented | capability | An interpretation must say why: a reading recorded without a rationale is refused, the residual it leaves names the premise, the conclusion and the evidence to re-read, and no level of source authority discharges it. |
| `CAP-CORRECTNESS-007` | implemented | capability | Kernel arithmetic is exact fixed-point, not floating point: the result does not depend on the order of the operands, neighbouring values are distinguished at any magnitude, and a result that could not be represented exactly is refused rather than rounded. |
| `CAP-CORRECTNESS-008` | implemented | capability | Persisted kernel proposals always serialize their schema version; versionless proposals read as v1 and replay, unsupported proposal majors are unreadable before their shape is interpreted, and malformed v1 inputs remain separately diagnosed as malformed. |
| `CAP-CORRECTNESS-009` | implemented | capability | A citation states a relation only in a phrasing its contract declares, found within one sentence with each term in its own role and no negation, hedge, condition or question in that sentence; otherwise it records only that the terms are mentioned, which a trust step will not lift. |
| `CAP-CORRECTNESS-010` | implemented | capability | A derivation reaches its goal only by concluding that the goal holds, compared field by field: a citation without a trust step attributes a statement without asserting it, and two propositions that render alike are told apart. |
| `CAP-CORRECTNESS-011` | implemented | capability | The authority floor a trust step enforces is the caller's policy, carried in the snapshot a decision is made against and re-decided with it: a proposal may raise the floor and cannot lower it. |
| `CAP-CORRECTNESS-012` | implemented | limitation | Cue lists are finite and English-first, so a declared phrasing in a sentence that denies or doubts it in words the list does not carry still states the relation. |
| `CAP-CORRECTNESS-013` | implemented | capability | For contract-declared number, money and date object endpoints, citation terms and declared phrasing slots compare exact values under closed grammars. Shared currency symbols and numeric day/month dates leave evidence-linked residuals unless explicit contract policies resolve them. Types and policies are pinned for offline replay; legacy snapshot bytes are unchanged. |
| `CAP-CORRECTNESS-014` | implemented | capability | Across a labelled corpus written by the project, no sentence a reader marks as not stating a fact reaches accepted, apart from the one recorded limitation it names. This is a fixture, not a measurement on real questions. |

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
| `CAP-AGENT-001` | experimental | capability | Agent definitions and agent runs are persisted with their owning project, and a run reports its status, state and usage from its durable log rather than from a stored status. |
| `CAP-AGENT-002` | implemented | capability | Starting an agent run enqueues it on the durable path, and the graph worker executes it: deterministic nodes, local-model nodes, memory reads, validators, and job and memory-write effects that happen once per run however many workers it takes. |
| `CAP-AGENT-003` | implemented | capability | A validator that fails sends an agent round again with what its last attempt left, and a run ended by a budget, by a lack of progress or by a safety check reports stopped, never succeeded. |
| `CAP-AGENT-004` | implemented | capability | An agent run can be cancelled over the API, and a run waiting at a human gate can be approved, which waives the failure, or refused, which stops the run. |
| `CAP-AGENT-005` | implemented | capability | What a model reports spending counts against a run as soon as it is known, and usage nobody measured is counted as unknown rather than as zero. |
| `CAP-AGENT-006` | implemented | capability | An agent cannot propose a memory claim that is active, cannot run a job its project does not own, and cannot be read, started or cancelled from another project. |
| `CAP-AGENT-007` | implemented | limitation | A graph where a model chooses the next node cannot run and is refused at run start; agent runs execute in observe mode, so a finished run is unverified; and retrieval is lexical, finding shared words and nothing about meaning. |
| `CAP-AGENT-008` | implemented | capability | A project admin can erase what a finished agent run was given and produced, its stored inputs and outputs, the values those name and its starting state, while its event log, definition and certificates stay whole; bytes another run still holds are kept for that run and go when it is erased too, so erasing one run never breaks another; a run that has not ended is refused; an optional retention schedule erases runs a set number of days after they end; and the log itself records no content, since schema failures and model refusals are described without the value. |
| `CAP-AGENT-009` | implemented | capability | An agent run can be exported as a digest-checked bundle of its definition, its whole log and what the installation reports, carrying none of the values it was given or produced, and capsulet-replay reads one with no database or network: it refuses a changed bundle, re-folds the log, says when the log does not support the report, and explains a run that succeeded, failed, was interrupted mid-call or ran out of budget, naming which steps cannot be reproduced offline. |
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
| `CAP-OBSERVABILITY-002` | experimental | capability | Agent runs export OpenTelemetry traces whose model, retrieval and tool spans follow the generative-AI semantic conventions of release 1.41.0: every lease of a run, on any worker, continues the trace of the request that started it; node attempts, model calls, retrievals and effects carry run, node, attempt and log-position identifiers, token usage only where a provider reported it, and failures by kind; no prompt, completion, retrieved text, input value or failure detail is exported, metric labels come from closed sets, and a collector that is down does not change how a run ends. |

## Offline certificate replay

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-REPLAY-001` | implemented | capability | A certificate bundle replays offline to its recorded verdict, and one changed byte of evidence makes replay report rejected instead. |
| `CAP-REPLAY-002` | implemented | capability | The replay binary cannot reach a database, an HTTP client, an async runtime, or a model provider, asserted over its resolved dependency closure. |

## Offline replay

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-REPLAY-003` | implemented | capability | Replay does not overstate what it established: a divergent replay yields no verdict, inputs that are present and unreadable are reported separately from absent ones and disqualify the result, a bundle with nothing to check says so, and a deterministic decision re-made under rules other than the ones recorded for it says so. |

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
