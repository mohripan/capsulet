# Capsulet Roadmap

This roadmap defines the long-term direction for Capsulet: a Kubernetes-native distributed job queue and sandboxed script execution platform, distributed as an installable Helm chart.

The project should grow in stages. Each stage should leave the repository in a coherent, demoable state, with documentation and release quality improving alongside the runtime features.

## Guiding Principles

- The Helm chart is the product distribution, not an afterthought.
- Kubernetes is part of the execution model, not only the hosting layer.
- Security controls for running user code should be visible and configurable from the first public release.
- The smallest public version should be useful: submit a script, run it, inspect status, retrieve logs, and retry failures.
- Production-grade does not mean feature-heavy. It means installable, observable, configurable, tested, documented, and honest about limits.
- Defaults should support local evaluation, while chart values should support external production dependencies.

## Non-Goals for the First Releases

These are valuable later, but should not block early releases:

- visual workflow builder
- billing
- hosted SaaS control plane
- multi-tenant organization model
- dozens of integrations
- WASM execution
- exactly-once execution guarantees
- complex policy UI
- custom Kubernetes operator

## Phase 0: Project Foundation

Goal: establish the repository as a serious cloud-native product before runtime work begins.

Deliverables:

- Rust workspace skeleton
- dashboard application skeleton
- Helm chart skeleton
- docs directory
- examples directory
- GitHub Actions CI skeleton
- architecture decision records
- contribution guide
- development environment guide
- local Kubernetes testing guide for kind or minikube

Expected repository shape:

```text
crates/
  api/
  worker/
  scheduler/
  runner/
  cli/
  core/
dashboard/
charts/
  capsulet/
examples/
docs/
.github/
  workflows/
```

Engineering checklist:

- `cargo test` runs in CI
- formatting and linting are defined
- Docker build strategy is documented
- chart naming and image naming are finalized
- local development workflow is documented
- license and project metadata are complete

Exit criteria:

- A contributor can clone the repository, run tests, and understand the intended architecture.
- The chart exists, even if it only installs placeholder services.

## Phase 1: Minimal Job System

Goal: build the smallest durable job queue that can execute one script type end to end.

Scope:

- API service
- core domain model
- PostgreSQL persistence
- worker service
- Kubernetes Job runner
- basic CLI
- Python script execution
- job status tracking
- log capture

Core job states:

- `queued`
- `leased`
- `running`
- `succeeded`
- `failed`
- `cancelled`
- `timed_out`

API capabilities:

- submit a job
- get job status
- list jobs
- fetch logs
- cancel a queued or running job

Worker capabilities:

- lease queued jobs
- create Kubernetes Jobs
- watch pod status
- collect logs
- mark final state
- handle worker restart without losing jobs

CLI capabilities:

- `capsulet submit`
- `capsulet status`
- `capsulet logs`
- `capsulet cancel`

Helm capabilities:

- API deployment
- worker deployment
- PostgreSQL dependency option
- service account
- minimal RBAC
- ConfigMap and Secret templates
- configurable image registry, repository, tag, and pull policy

Exit criteria:

- A user can install Capsulet into a local cluster and run a Python hello-world job.
- Job metadata survives API and worker restarts.
- Basic logs are retrievable after completion.

## Phase 2: Public Alpha v0.1.0

Goal: publish the first installable version as a real Kubernetes application.

Runtime features:

- script job submission
- durable job attempts
- retry policy
- job timeout
- basic artifact upload to MinIO or S3-compatible storage
- dashboard with job list and job detail
- execution image selection from safe defaults
- example jobs

Chart features:

- bundled PostgreSQL for local installs
- bundled MinIO for local installs
- external database support
- external object storage support
- dashboard deployment and service
- scheduler deployment
- configurable resource requests and limits
- configurable probes
- optional ingress
- optional network policies
- minimal RBAC
- `values.schema.json`
- `helm lint` clean
- `helm template` smoke tests

Security defaults:

- non-root containers
- privilege escalation disabled
- dropped capabilities
- seccomp `RuntimeDefault`
- dedicated service account for script jobs
- configurable job resource limits
- default job timeout

Release automation:

- build container images on tags
- push images to GHCR
- package Helm chart
- publish GitHub Pages Helm repository
- optionally publish OCI Helm chart to GHCR
- generate release notes

Documentation:

- quickstart
- architecture
- Helm values reference
- security model
- local development
- troubleshooting

Exit criteria:

- A user can run `helm install capsulet ...`, submit an example Python script, inspect logs, and retrieve an artifact.
- The release is reproducible from a Git tag.

## Phase 3: Operability and Chart Maturity

Goal: make Capsulet feel like a product operators can evaluate seriously.

Observability:

- structured logs
- Prometheus metrics
- optional `ServiceMonitor`
- health and readiness endpoints
- job duration metrics
- queue depth metrics
- worker lease metrics
- Kubernetes Job failure metrics

Chart maturity:

- chart README generation
- Artifact Hub annotations
- `artifacthub-repo.yml`
- chart tests
- example values files
- image pull secrets
- pod annotations and labels
- node selectors
- tolerations
- affinity
- topology spread constraints
- priority class support
- pod disruption budgets

Operational features:

- database migrations job
- graceful shutdown for workers
- lease expiry and recovery
- configurable concurrency per worker
- execution pool definitions for routing jobs to different classes of Kubernetes nodes
- default execution pool selection for jobs that do not specify one
- cleanup policy for completed Kubernetes Jobs
- retention settings for job records, logs, and artifacts

Exit criteria:

- Capsulet can be installed with internal or external dependencies.
- Operators have enough knobs to run it in a realistic cluster.
- Users can define at least one named execution pool that maps jobs onto a specific class of Kubernetes nodes.
- Metrics and logs make failures diagnosable.

## Phase 4: Workflow Engine Capabilities

Goal: evolve from single script execution into lightweight workflows.

Features:

- automations as the user-facing object for trigger-driven job creation
- job definitions stored as reusable templates
- parameter schema for jobs
- default execution pool selection per automation
- manual triggers from API, CLI, and dashboard
- scheduled jobs with cron-like triggers
- one-time delayed triggers
- webhook triggers for external systems
- dependent triggers from successful or failed upstream jobs
- boolean trigger expressions with `AND`, `OR`, and grouped parentheses
- dependent jobs
- fan-out and fan-in execution
- simple DAG model
- retry policy per step
- artifact passing between steps
- environment variable injection from secrets
- manual re-run from a previous attempt

API and CLI:

- create automation
- update automation metadata, trigger expression, target job, and execution pool
- enable, disable, and delete automation
- create job definition
- submit job definition with parameters
- create, update, pause, and delete triggers
- list schedules
- pause and resume schedules
- inspect workflow run graph

Dashboard:

- automation builder
- trigger condition builder
- execution pool selector per automation
- workflow run detail
- step timeline
- attempt history
- schedule management
- trigger management
- artifact browser

Exit criteria:

- Users can create an automation in the UI by choosing a target job, execution pool, and trigger conditions.
- Capsulet can run a small multi-step workflow such as extract, transform, and report.
- Users can trigger jobs manually, on a schedule, from a webhook, or from another job's result.
- Workflow state is durable and understandable through API, CLI, and dashboard.

## Phase 5: Security Hardening

Goal: improve the trust boundary around untrusted script execution.

Execution controls:

- namespace-per-job or namespace-per-pool option
- execution image allowlist
- restricted environment variable policy
- secret mounting controls
- network egress policy presets
- read-only root filesystem for job pods where possible
- configurable temporary volume strategy
- active deadline enforcement
- pod security admission documentation

Policy and audit:

- audit log for job submission and cancellation
- API authentication foundation
- role-based API permissions
- admission-style validation for job specs
- signed image guidance
- supply chain documentation

Documentation:

- threat model
- production security guide
- hardening checklist
- known limitations

Exit criteria:

- The project clearly documents what it protects against and what it does not.
- Administrators can restrict runtime images, network access, resource usage, and secret exposure.

## Phase 6: Reliability and Scale

Goal: make the queue and workers robust under load and failure.

Queue behavior:

- robust leasing protocol
- lease renewal
- lease expiry recovery
- idempotent state transitions
- backoff and retry scheduling
- dead letter state
- priority queues
- concurrency limits by queue or job type
- concurrency limits by execution pool

Worker scaling:

- horizontal worker scaling
- per-worker concurrency
- queue partitioning strategy
- Kubernetes API rate limit handling
- backpressure when the cluster is saturated

Execution pools:

- named execution pools such as `mini`, `standard`, `large`, or `gpu`
- pool selection on job submission
- pool defaults on reusable job definitions
- pool-level resource defaults
- pool-level timeout defaults
- pool-level concurrency limits
- pool-level Kubernetes `nodeSelector`, affinity, and toleration templates
- optional namespace-per-pool execution model
- scheduler checks for pool capacity before admitting work
- clear fallback behavior when a requested pool is unavailable
- metrics for queued, running, succeeded, and failed jobs by pool

Kubernetes placement model:

- Capsulet chooses the execution pool for a job.
- The Kubernetes scheduler chooses the specific node inside that pool.
- Node groups are represented with Kubernetes-native labels, taints, tolerations, affinity, and resource requests.
- Manual round-robin to individual Kubernetes nodes is avoided unless a future non-Kubernetes runner backend needs explicit host assignment.

Data integrity:

- migration tests
- transaction boundaries documented
- duplicate event handling
- reconciliation loop for orphaned Kubernetes Jobs
- recovery from API and worker crashes

Exit criteria:

- Capsulet behaves predictably when pods restart, workers crash, jobs fail, or Kubernetes events are missed.
- Users can route lightweight jobs to small nodes and compute-heavy jobs to large or specialized nodes.
- Load tests define practical limits and tuning recommendations.

## Phase 7: Production API, Auth, and Multi-User Operation

Goal: make Capsulet suitable for teams rather than only single-user clusters.

Features:

- API authentication
- API tokens
- user and service account model
- project or namespace grouping
- RBAC for submit, view, cancel, and administer actions
- audit events
- quotas by project or queue
- rate limiting

Integrations:

- OIDC support
- Kubernetes auth option
- webhook notifications
- inbound webhook triggers with authentication controls
- GitHub Actions submission example
- CI/CD integration examples

Exit criteria:

- Multiple users can safely share one Capsulet installation with clear permissions and auditability.

## Phase 8: Developer Experience and Ecosystem

Goal: make Capsulet pleasant to adopt and extend.

Developer experience:

- polished CLI
- shell completions
- generated API client
- OpenAPI spec
- SDK examples
- local fake runner for development without Kubernetes
- sample repositories
- richer examples

Plugin and extension points:

- custom runner interface
- custom artifact backend interface
- notification hooks
- custom trigger interface
- job validation hooks
- event sink integration

Documentation:

- tutorial series
- example gallery
- operations cookbook
- common failure guide
- chart migration guide

Exit criteria:

- New users can learn Capsulet from examples.
- Developers can build small integrations without reading the entire codebase.

## Phase 9: Advanced Runtime Options

Goal: support more execution environments while keeping Kubernetes Jobs as the default.

Possible runtime backends:

- Kubernetes Job runner
- local development runner
- container runtime runner
- external queue runner
- WASM runner

Language support:

- Python first
- shell scripts
- Node.js scripts
- containerized arbitrary command jobs

Execution features:

- streaming logs over WebSocket or server-sent events
- interactive cancellation
- large artifact handling
- job input and output schemas
- result summaries
- richer failure classification

Exit criteria:

- Capsulet can support multiple job execution styles without weakening the core Kubernetes-native story.

## Phase 10: Production Grade Release v1.0

Goal: declare a stable, supportable product surface.

Required quality bar:

- stable API versioning
- documented compatibility policy
- stable Helm values contract
- upgrade guide
- backup and restore guide
- disaster recovery guide
- security hardening guide
- load test results
- documented SLO-oriented metrics
- release signing or provenance
- dependency update automation
- vulnerability scanning
- end-to-end tests on kind
- chart installation tests

Operational guarantees:

- clear upgrade path between minor versions
- documented database migration behavior
- documented job state machine
- defined support matrix for Kubernetes versions
- defined support matrix for PostgreSQL and object storage

Exit criteria:

- Capsulet can be recommended for a serious self-hosted deployment with documented limits.
- The public documentation explains installation, operation, security, troubleshooting, upgrades, and contribution.

## Execution Pool Concept

Execution pools are Capsulet's routing layer for different classes of compute. A pool should describe where and how a job may run, while Kubernetes remains responsible for selecting the exact node.

Example Helm values shape:

```yaml
executionPools:
  defaultPool: mini
  pools:
    mini:
      description: Lightweight jobs such as email, webhooks, and small scripts
      nodeSelector:
        capsulet.dev/pool: mini
      tolerations: []
      resources:
        requests:
          cpu: 100m
          memory: 128Mi
        limits:
          cpu: 500m
          memory: 512Mi
      timeoutSeconds: 120
      maxConcurrentJobs: 50

    large:
      description: Compute-heavy jobs such as model inference and batch processing
      nodeSelector:
        capsulet.dev/pool: large
      tolerations:
        - key: capsulet.dev/pool
          operator: Equal
          value: large
          effect: NoSchedule
      resources:
        requests:
          cpu: "2"
          memory: 4Gi
        limits:
          cpu: "8"
          memory: 16Gi
      timeoutSeconds: 3600
      maxConcurrentJobs: 10
```

Example job submission:

```sh
capsulet submit examples/send-email \
  --pool mini \
  --input '{"to":"team@example.com"}'

capsulet submit examples/model-inference \
  --pool large \
  --input '{"model":"forecast-v1"}'
```

The first implementation should treat pools as static Helm configuration. Later versions can promote them to API-managed objects if runtime pool changes become important.

## Automation and Trigger Model Concept

Capsulet should not be limited to cron. The user-facing object should be an automation: a named rule that evaluates one or more triggers, decides whether a job or workflow should run, and applies default execution settings such as the execution pool.

An automation should answer four questions:

- What is this automation called?
- What job or workflow does it create?
- Which execution pool should the created run use by default?
- Which trigger expression must evaluate to true?

The execution path after an automation fires should stay the same: validate input, create a durable run, route it to an execution pool, execute it, and record the result.

Initial trigger types:

- `manual`: created by API, CLI, or dashboard action
- `schedule`: recurring cron-like schedule
- `delay`: one-time run at a future timestamp
- `webhook`: inbound HTTP event from another system
- `dependency`: created when another job or workflow step succeeds, fails, or completes

Later trigger types:

- `event`: events from systems such as object storage, GitHub, GitLab, queues, or message brokers
- `custom`: user-defined trigger adapters loaded through a plugin or extension interface

Trigger expression model:

- triggers are named inside an automation
- `AND` means all referenced trigger conditions must be satisfied
- `OR` means any referenced trigger condition can fire the automation
- parentheses allow grouping, such as `(business_hours AND webhook_received) OR manual_override`
- the first implementation should use a structured expression tree internally instead of storing raw user-entered text
- the UI should build expressions through controls, while the API can expose a JSON or YAML representation

Example automation shape:

```yaml
name: nightly-report
enabled: true
target:
  kind: job
  name: generate-report
execution:
  pool: mini
  timeoutSeconds: 600
triggers:
  nightly:
    type: schedule
    cron: "0 2 * * *"
    timezone: UTC
condition:
  trigger: nightly
input:
  reportDate: "{{ date.yesterday }}"
```

Example webhook-triggered job:

```yaml
name: resize-uploaded-image
enabled: true
target:
  kind: job
  name: resize-image
execution:
  pool: mini
triggers:
  upload_event:
    type: webhook
    path: /hooks/images/resize
    auth:
      type: shared-secret
condition:
  trigger: upload_event
```

Example grouped trigger condition:

```yaml
name: train-model-after-data-refresh
enabled: true
target:
  kind: workflow
  name: train-model
execution:
  pool: large
triggers:
  data_ready:
    type: dependency
    after:
      job: prepare-training-data
      condition: succeeded
  approved:
    type: webhook
    path: /hooks/model-training/approved
  manual_override:
    type: manual
condition:
  or:
    - and:
        - trigger: data_ready
        - trigger: approved
    - trigger: manual_override
```

The first production-shaped implementation should support automations with one trigger and a simple condition. Boolean trigger expressions, webhooks, and dependency triggers should follow once authentication, idempotency, retries, and audit logging are in place.

## Helm Chart Quality Checklist

The chart should eventually include:

- `helm lint` passing
- `helm template` passing
- `values.schema.json`
- generated chart README
- configurable image registry, repository, tag, and pull policy
- configurable resources for every workload
- configurable probes
- configurable service accounts
- minimal RBAC
- bundled PostgreSQL option
- external PostgreSQL option
- bundled MinIO option
- external object storage option
- optional ingress
- optional network policies
- optional `ServiceMonitor`
- optional pod disruption budgets
- optional autoscaling
- execution pool configuration with node selectors, affinity, tolerations, resources, timeouts, and concurrency limits
- migration job
- chart tests
- Artifact Hub metadata
- example values files

## Documentation Checklist

Docs should grow alongside the product:

- `docs/installation.md`
- `docs/quickstart.md`
- `docs/architecture.md`
- `docs/helm-values.md`
- `docs/security.md`
- `docs/sandboxing.md`
- `docs/operations.md`
- `docs/troubleshooting.md`
- `docs/development.md`
- `docs/release.md`
- `docs/artifacthub.md`
- `docs/threat-model.md`
- `docs/upgrade-guide.md`

## Example Jobs

The examples directory should eventually include:

- hello Python script
- CSV report generation
- image resize job
- retry failure demo
- artifact upload demo
- scheduled report demo
- multi-step workflow demo
- external S3 configuration demo
- mini execution pool email job demo
- large execution pool model inference job demo

## Suggested Public Milestones

`v0.1.0` public alpha:

- installable Helm chart
- API, worker, scheduler, dashboard
- PostgreSQL and MinIO options
- Python script execution via Kubernetes Jobs
- logs, attempts, retries, artifacts
- GHCR images
- GitHub Pages Helm repository

`v0.2.0` operability:

- metrics
- ServiceMonitor
- retention settings
- migration job
- chart schema
- better dashboard
- better troubleshooting docs

`v0.3.0` workflow basics:

- job templates
- schedules
- step dependencies
- artifact passing
- workflow run detail

`v0.4.0` security:

- image allowlists
- stronger network policy presets
- audit events
- authentication foundation
- threat model

`v0.5.0` reliability:

- lease recovery
- reconciliation
- priority queues
- load tests
- backpressure controls

`v1.0.0` production grade:

- stable API and chart values
- upgrade guarantees
- hardened security documentation
- complete operations guide
- compatibility matrix
- release provenance

## Open Product Questions

These should be resolved through design notes before implementation locks them in:

- Should Capsulet expose workflows as a custom YAML format, JSON API objects, or both?
- Should the Kubernetes Job runner create jobs in the Capsulet namespace by default or in a separate execution namespace?
- Should logs be stored primarily in PostgreSQL, object storage, or both?
- How strict should execution image allowlists be by default?
- Should the CLI talk only to the API, or also support direct Kubernetes discovery for local installs?
- Should the dashboard be required for the default install or enabled by default but optional?
- What should the minimum supported Kubernetes version be?
- What state machine guarantees should be documented before v1.0?
- Should execution pools be a Capsulet API object, a Helm-only static configuration, or both?
- Should execution pools map only to Kubernetes node labels and taints, or should they also support external runner agents later?
- Should pool selection be required for every job, or should Capsulet apply a default pool when none is provided?

## Current Status

Capsulet has completed the first five implementation sprints:

- Sprint 001 scaffolded the Rust workspace, dashboard prototype, Helm chart, docs, and CI foundation.
- Sprint 002 added manual job submission, PostgreSQL persistence, API basics, and a worker with a stub runner.
- Sprint 003 added Kubernetes Job execution, execution pools, bounded logs, CLI status/log commands, and a minikube smoke path.
- Sprint 004 added cancellation, timeout classification, retry scheduling, lease recovery, and Kubernetes Job cleanup policy.
- Sprint 005 added object storage for script bundles, large logs, and artifacts, plus artifact API/CLI commands and MinIO-backed smoke coverage.
- Sprint 006 connected the dashboard runs surface to the live API for run listing, seeded job and script submission, run detail, cancellation, logs, artifact listing, and artifact download.

The current planned next step is to choose the Sprint 007 path: bundled PostgreSQL and MinIO chart maturity, release automation, observability metrics, or security hardening foundations.
