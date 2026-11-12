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
| `CAP-ASSURANCE-010` | experimental | capability | A validator speaks `capsulet.verifier/v1`: one request in, naming the obligations and the read-only inputs by digest, and one response out with `holds`, `does_not_hold` or `cannot_decide` for each. The host checks its registration first, so a replayable tool granted the network or a model, a model not declared as one, or an image not pinned by digest is refused. It then holds the run to its limits, refuses changed inputs, oversized outputs and anything but one result per obligation as typed failures owned by the verifier, digests the outputs itself, and builds the verifier record. An isolated tool's `holds` discharges on basis `replayable_tool`, a model's endorses or disputes, and an unisolated development run decides nothing. Container isolation is not implemented yet. |
| `CAP-ASSURANCE-001` | implemented | capability | Under enforce, a protected boundary is denied when no certificate exists, because an absent certificate is unverified and unverified never satisfies a higher minimum. |
| `CAP-ASSURANCE-002` | implemented | capability | A certificate's verdict is derived from its obligations under its recorded mode; a certificate recording a verdict its obligations do not justify cannot be sealed. |
| `CAP-ASSURANCE-009` | implemented | capability | A certificate records what kind of checker discharged each obligation: mechanical, a replayable tool, or a person reviewing under the authority the obligation names. A model-backed verifier only endorses or disputes, both of which leave the obligation outstanding, so no combination of them makes a certificate accepted, and the seal refuses a certificate in which a model discharges anything. An obligation may restrict which kinds of checker count. A boundary may refuse to cross over a dispute, or require every open obligation to be endorsed by a model of another family than the proposer's. Certificates sealed before bases existed read unchanged. |

## Automations

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-AUTOMATION-006` | experimental | capability | Cron schedules fire automations. Cron expressions are parsed and stored, but no test fires one. |
| `CAP-AUTOMATION-007` | experimental | capability | A SQL trigger's query runs in a read-only transaction under a statement timeout. No test shows either holding. |
| `CAP-AUTOMATION-008` | experimental | capability | Custom trigger plugins run isolated through the Kubernetes runner. Their output parsing and claim exclusivity are tested; the isolated run is not. |
| `CAP-AUTOMATION-001` | implemented | capability | A signed webhook delivery is accepted only with a timestamp-bound HMAC bound to its automation, trigger and delivery; a changed body or timestamp, a replay against another trigger, and a replay under a fresh delivery ID are refused. |
| `CAP-AUTOMATION-002` | implemented | capability | An automation can be triggered manually, including while it is disabled, and a manual trigger is refused when the workflow queue is overloaded. |
| `CAP-AUTOMATION-003` | implemented | capability | A due interval schedule creates exactly one compatibility workflow run for its automation, owned by the automation's project. |
| `CAP-AUTOMATION-004` | implemented | capability | A SQL trigger's query must be a single SELECT or WITH statement. |
| `CAP-AUTOMATION-005` | implemented | capability | An automation cannot use a trigger plugin that belongs to another project. |

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
| `CAP-CORRECTNESS-015` | implemented | capability | An answer can be held to the question it was asked. Pinned to a typed query (`POST /v1/reasoning/ask` `query`), the goal must state the asked relation, name each bound term as written, by exact value, or through a confirmed name group the step names, and fill the wanted slot; a grounded claim that answers another question is rejected as `goal_not_responsive`. A question pinned in words alone needs the proposal's own reading of it, which the kernel holds the answer to and records as a residual of kind `question`; an answer that gives none is refused. A snapshot with no question decides as before. Whether the words ask what a reading says is not decided. |
| `CAP-CORRECTNESS-016` | implemented | capability | A contract can declare how its documents mark out provisions, refer to one another, qualify one another (`subject_to`, `overrides`), make a sentence conditional, and define terms. A cited clause that is qualified then states nothing outright: trusting it is rejected as `relation_qualified`, and reading it leaves a residual for each qualifying provision the derivation does not read it with. A referenced provision or a defined term's definition left out is a residual too, and citing it with `read_with` removes that residual and not the reading. Every cue and template comes from the contract, pinned for replay; a contract declaring none decides as before. Whether a reading of the clause is right is not decided. |
| `CAP-CORRECTNESS-017` | implemented | capability | A reading a reasoning certificate leaves open can be decided by a person holding `review:write`: `POST /v1/reasoning/certificates/{id}/readings/{reading}/review` records `holds` or `does_not_hold` with a rationale, bound to that reading by a digest over it and its certificate. The outcome (`GET /v1/reasoning/certificates/{id}/review`) is re-derived from the kernel's certificate and the recorded reviews: every reading held is `accepted` on basis `human_review`, any reading refuted is `rejected`, and a rejected certificate stays rejected. `GET /v1/reasoning/readings` lists the open ones. A decision is recorded once and never rewritten, and another project's certificate is not found. |
| `CAP-CORRECTNESS-018` | implemented | capability | Across a labelled corpus of answers to questions about one agreement the project wrote, no answer a reader marks misread or non-responsive reaches accepted, apart from the one recorded limitation it names, and each case is caught by the mechanism it names: a qualifier, a defined term, a cue, or the pinned query. Both sound answers reach accepted. This is a fixture, not a measurement on real contracts or questions. |
| `CAP-CORRECTNESS-019` | implemented | limitation | A qualification written as a sentence of its own, with no cue or reference the contract declares, is not read with the clause it qualifies, so an exact quote of that clause can still be accepted. |

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
| `CAP-MEMORY-001` | implemented | capability | A memory claim cannot be created without evidence, and an evidence excerpt must be exactly the bytes at its span in the version of the source it cites. |
| `CAP-MEMORY-002` | implemented | capability | Sources and claims are created, listed and read over the API, are stored with their evidence, and a stored memory record is found only in its own project. |
| `CAP-MEMORY-003` | implemented | capability | Conflicting active claims can coexist, and approving a claim that conflicts with an active one opens a conflict for review that can be resolved or dismissed. |
| `CAP-MEMORY-004` | implemented | capability | Entity resolution candidates are confirmed or rejected in a review queue, and confirming one requires evidence. |
| `CAP-MEMORY-005` | implemented | capability | Memory contracts written in the contract language compile to runtime policy, every bundled example compiles, and contracts are stored and read over the API. |
| `CAP-MEMORY-006` | implemented | capability | Local text ingestion produces evidence and candidate claims, and an ingestion review queue approves or rejects each candidate. |
| `CAP-MEMORY-007` | implemented | capability | A nested memory subgraph activates only when its required invariants hold, and a summary claim in it needs a summary trace that names inner claims and evidence. |

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
| `CAP-AGENT-009` | implemented | capability | An agent run can be exported with one command (`capsulet agent-runs bundle`) as a digest-checked bundle of its definition, its whole log and what the installation reports, carrying none of the values it was given or produced, and capsulet-replay reads one with no database or network: it refuses a changed bundle, re-folds the log, makes every decision in it again from the log before it and names any the runtime would not have made, says when the log does not support the report, and explains a run that succeeded, failed, was interrupted mid-call or ran out of budget, naming which steps cannot be reproduced offline; with `--html` it writes the same account as one self-contained page that walks run, node, observed action, evidence and verification outcome. |
| `CAP-AGENT-010` | implemented | capability | An agent run's explanation (`GET /v1/agent-runs/{id}/explanation`, shown on the run's dashboard page) is made from the same bundle, by the same code, as an offline replay of its export. It labels each node's last attempt as proposed, checked, waived, disputed, unverified, failed, interrupted or not run, with a check's rules and the verdict the log records for it; lists what each node was observed to do and the evidence digests that names; and says whether the run's result is unverified, abstained or pending. No label certifies a result, and another project's run is not found. |
| `CAP-GRAPH-001` | implemented | capability | Capsulet returns a deterministic static order for valid acyclic typed agent graph definitions. |

## HTTP API

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-OPENAPI-001` | implemented | capability | The OpenAPI 3.1 document is generated from typed endpoint metadata that names the Rust wire type of every request, response, query and header, and every route is registered from that metadata, so its method, path and authentication are the contract's; it has no unresolved references, validates through utoipa and is checked for deterministic drift. Every response the API's own tests and the graph-worker suites receive is checked against the contract of the route that produced it, failures included, and the dashboard and Python transports are checked against the operations they call, including the query parameters they send. |

## Helm and self-hosting

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-HELM-001` | experimental | capability | A Helm chart installs the current control plane, execution services, dashboard, and optional bundled dependencies. The `kind` gate installs it on a Kind cluster from images built from the checkout, runs its Helm test and an in-cluster smoke, and uninstalls it with nothing left behind. It is not a public-alpha distribution: upgrade, rollback, backup and restore are untested. |

## IR adapters

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-ADAPTERS-001` | implemented | capability | Today's job DAGs and agent graphs translate into the IR and pass structural admission, and a dependency cycle the current model rejects is refused by admission too. |

## Identity and IAM

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-IAM-007` | experimental | capability | OIDC bearer tokens are validated against the configured issuer and audience. No test authenticates an OIDC token. |
| `CAP-IAM-001` | implemented | capability | Protected routes require a valid bearer token, and a configured token carries its role and any explicit scopes, expires, and never appears in debug output. |
| `CAP-IAM-002` | implemented | capability | Roles and scopes are enforced on every reviewed public operation, for anonymous, user and service-account callers. |
| `CAP-IAM-003` | implemented | capability | A request acts in the project it selects, a principal cannot select a project it is not a member of, and a run is read and changed only within its own project. |
| `CAP-IAM-004` | implemented | capability | Project administrators manage project memberships, and a user never inherits the membership of a service account with the same name. |
| `CAP-IAM-005` | implemented | capability | Service accounts authenticate by token hash, are listed and revoked only within their own project, stop authenticating once revoked, and never inherit a same-named user's membership. |
| `CAP-IAM-006` | implemented | capability | Mutations are recorded as durable audit events in the project the write selected, and an audit listing excludes other projects and tenants before it applies its limit. |
| `CAP-IAM-009` | implemented | capability | Memory contracts, summary traces and ingestion run outputs are read within the caller's project before any listing limit, and a memory write that names a subgraph (a member, an edge, a summary trace, an entity resolution, a graph attachment or a child subgraph) is refused when that subgraph belongs to another project. |

## Jobs and runners

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-JOB-007` | experimental | capability | A job can execute as a local process. Only the Compose agent smoke exercises this path; no test runs it or its limits. |
| `CAP-JOB-008` | experimental | capability | A job can execute a Python script under WASI and collect its artifacts. Its test runs only in the `wasm-python` gate, which is in no profile, and passes without running anything where no WASI Python runtime is configured. |
| `CAP-JOB-009` | experimental | capability | A job can execute as a Job on a Kubernetes cluster. The `kind` gate runs one: on a Kind cluster of its own, its smoke starts a job run that the worker executes as a Kubernetes Job, then reads the run's logs and artifact. |
| `CAP-JOB-001` | implemented | capability | The job worker renews a job run's lease for as long as its runner is still working. |
| `CAP-JOB-002` | implemented | capability | PostgreSQL leases a queued job run to one worker at a time, and an execution pool's concurrency limit caps how many of its runs are leased at once. |
| `CAP-JOB-003` | implemented | capability | The stub runner completes a job run, fails it, or schedules a retry when attempts remain. |
| `CAP-JOB-004` | implemented | capability | A job run's logs are stored with it, and logs larger than the inline limit are kept as a log artifact instead. |
| `CAP-JOB-005` | implemented | capability | Artifacts a runner reports are stored with their run and are not visible from another run. |
| `CAP-JOB-006` | implemented | capability | The Kubernetes runner renders a job run as a Kubernetes Job that runs as non-root on a read-only root filesystem, without privilege escalation, host networking or a mounted service-account token, within its execution pool's runtime class, node selector and resource limits, carrying its input artifacts and run labels. |

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
| `CAP-OBSERVABILITY-003` | experimental | capability | Agent run traces can be viewed in Jaeger 2.23, an optional viewer for operators, pinned by digest: the `trace-viewer` gate sends a synthetic run's trace to it over OTLP/HTTP and finds it there by the trace id the run's bundle names, with Capsulet's spans and attributes and none of the run's input. The viewer has no authentication or project isolation, so it is not offered to tenants, and a run's trace id is disclosed only to a caller allowed to export its bundle. |

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

## Supply chain

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-SUPPLY-001` | experimental | capability | Each published image is built once and pushed by digest; that digest alone is scanned, attested with SLSA build provenance and an SBOM, and signed keylessly, and it is tagged only after its signature and both attestations verify. Pull requests build and scan the same images without any credential that could publish, sign or attest. |

## Verification

| ID | Maturity | Kind | Claim |
| --- | --- | --- | --- |
| `CAP-VERIFY-001` | implemented | guarantee | CI's `Required` check passes only when every gate of the full verification profile ran and passed in some job, every job it waited for succeeded, and no verifier run changed the worktree or left behind a container, volume, network, image, Kind cluster or credential directory it made. |

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
