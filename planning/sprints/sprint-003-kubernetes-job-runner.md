# Sprint 003: Kubernetes Job Runner

## Sprint Goal

Turn the Sprint 002 durable manual runner into the first real local-cluster execution path: submit `job_hello_python`, have a worker create a Kubernetes Job, wait for completion, capture bounded logs, and inspect the result through API and CLI.

By the end of this sprint, Capsulet should support this local evaluation flow:

1. Start local PostgreSQL.
2. Start the API with example job definitions seeded.
3. Start a local Kubernetes cluster such as kind or minikube.
4. Run a worker configured for the Kubernetes runner.
5. Submit `job_hello_python`.
6. The worker creates a Kubernetes Job in the configured execution namespace.
7. The run reaches `succeeded` or `failed` based on the script container exit result.
8. The run's bounded logs are retrievable through API and CLI.

This sprint should prove the execution boundary and local-cluster workflow without adding object storage, retries, schedules, or dashboard integration yet.

## Sprint Length

Recommended: 1 week.

## Sprint Theme

Real execution, narrow surface.

The goal is one working Kubernetes Job path, not a production-grade runner. Keep the implementation deliberately small: one job container, static seeded job definition, bounded log capture, and a documented local cluster path.

## Current Context

Sprint 002 completed:

- PostgreSQL persistence for job definitions, job runs, and attempts.
- API create/list/get run endpoints.
- Built-in `job_hello_python` seed support.
- Worker lease-and-run use case.
- `capsulet-runner` trait with success/failure stub runners.
- CLI commands for submit, list, and get.

The main Sprint 003 gap is that the runner still returns a synthetic result. The worker does not yet create Kubernetes Jobs, watch completion, or expose logs.

## Committed Scope

### 1. Kubernetes Runner Boundary

Add a concrete Kubernetes runner implementation behind the existing `capsulet-runner::Runner` trait.

Expected work:

- Choose a Rust Kubernetes client stack.
- Add a Kubernetes runner implementation without changing the worker use-case boundary.
- Resolve Kubernetes namespace and execution settings from environment/config.
- Render a Kubernetes `batch/v1 Job` from the leased run and known job definition fields.
- Use deterministic Kubernetes Job names derived from the run ID.
- Preserve the stub runner for local tests and failure simulation.

Recommended implementation:

- Use `kube` and `k8s-openapi`.
- Keep Kubernetes-specific code inside `capsulet-runner` or a clearly named runner module.
- Avoid adding Kubernetes client dependencies to `capsulet-core`.

Acceptance criteria:

- A unit test proves a run maps to the expected Kubernetes Job metadata and pod spec.
- Worker code can choose stub runner or Kubernetes runner by configuration.
- Kubernetes dependencies do not leak into `capsulet-core`.
- Job names are stable and Kubernetes-safe for normal run IDs.

### 2. Job Definition Resolution for Workers

The worker currently leases a `JobRun`, but real execution needs the referenced job definition.

Expected work:

- Extend the worker store boundary to load a job definition by ID.
- Add PostgreSQL adapter support for finding job definitions.
- Pass enough execution spec into the runner: runtime image, command, bundle object key placeholder, execution pool, and run ID.
- Keep the API job definition validation behavior unchanged.

Acceptance criteria:

- Worker returns a clear error if a leased run references a missing job definition.
- Tests cover successful definition lookup and missing definition handling.
- Existing API and worker tests still pass.

### 3. Kubernetes Job Lifecycle

Make the Kubernetes runner create the Job and wait for a terminal outcome.

Expected behavior:

- Create the Kubernetes Job if it does not already exist.
- Treat an already-existing Job with the same run-derived name as idempotent for a retrying worker tick.
- Wait until the Job reports succeeded or failed.
- Map Kubernetes success to `RunOutcome::Succeeded`.
- Map Kubernetes failure or timeout to `RunOutcome::Failed` or `RunOutcome::TimedOut` if the runner outcome is expanded in this sprint.
- Apply active deadline seconds from the execution pool or a conservative default.

Acceptance criteria:

- Local kind/minikube smoke test can run `job_hello_python` to `succeeded`.
- A failing image or command can produce a failed run in manual testing.
- Worker does not create duplicate Jobs for the same run ID.
- Kubernetes creation/watch errors are reported clearly in worker output.

### 4. Execution Pool Application

Apply the static execution pool concept to Kubernetes Job specs.

Expected work:

- Load execution pool configuration from the chart ConfigMap or a local YAML/env path.
- Apply pool `nodeSelector`, `tolerations`, resource requests/limits, and timeout to the created Job pod.
- Continue rejecting unknown pools in the API from configured pool names.
- Document that pool configuration is static in Sprint 003.

Acceptance criteria:

- A rendered Job for pool `mini` includes the mini pool resources and node selector.
- A rendered Job for pool `large` includes the large pool resources and toleration.
- Missing pool configuration fails before creating a Kubernetes Job.
- Unit tests cover pool-to-pod-spec mapping.

### 5. Bounded Log Capture

Capture enough logs to inspect a completed hello-world run.

Expected work:

- Add a narrow persistence model for run logs or attempt logs.
- Add migration for a log storage table with a size cap documented in code and docs.
- Have the Kubernetes runner read pod logs after completion.
- Store bounded logs in PostgreSQL for Sprint 003 only.
- Document that large logs move to object storage in a later sprint.

Acceptance criteria:

- A succeeded local `job_hello_python` run has retrievable logs.
- Log capture is bounded to prevent unbounded PostgreSQL growth.
- Log persistence tests cover save and fetch behavior.
- Worker does not fail a successful run only because log capture returns no pod logs.

### 6. Logs API and CLI

Expose captured logs through the existing API and CLI surfaces.

Expected endpoints and commands:

```text
GET /v1/jobs/runs/{id}/logs
capsulet logs <run-id>
capsulet status <run-id>
```

Expected work:

- Add API route for run logs.
- Add CLI `logs` command.
- Add CLI `status` alias or command that fetches one run and prints status-focused output.
- Keep `capsulet run get <id>` working.
- Return clear errors for missing runs or missing logs.

Acceptance criteria:

- API tests cover logs found, run missing, and no logs captured.
- CLI tests cover argument parsing and output formatting.
- Manual flow can submit, wait, run `capsulet status`, and run `capsulet logs`.

### 7. Local Kubernetes and Helm Workflow

Make local cluster execution repeatable.

Expected work:

- Add a local kind/minikube guide.
- Update Helm values for worker Kubernetes runner configuration.
- Ensure RBAC permits Job create/get/list/watch/delete and pod log read in the execution namespace.
- Decide whether Sprint 003 runs API/worker via `cargo run` against the cluster or through the chart.
- Keep chart lint/template checks passing.

Recommended implementation:

- Prefer a `cargo run` local workflow first, using the current chart for RBAC/config validation.
- Treat a full Helm-installed product as stretch unless the runner lands early.

Acceptance criteria:

- `helm lint charts/capsulet` passes.
- `helm template capsulet charts/capsulet` renders worker runner configuration and RBAC.
- Documentation explains how to create a local cluster, configure kube access, submit a run, execute it, and inspect logs.
- The install docs no longer imply the chart is only a placeholder if the chart becomes part of the manual flow.

### 8. Quality and Regression Coverage

Keep the Sprint 002 quality bar intact while adding Kubernetes-specific tests where practical.

Expected work:

- Add unit tests for Job rendering and pool mapping.
- Add API tests for logs.
- Add repository tests for log persistence.
- Add worker tests for definition lookup and runner invocation.
- Add a manual smoke checklist for kind/minikube because CI may not run a cluster yet.

Acceptance criteria:

- `cargo fmt --check` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes.
- Kubernetes smoke steps are documented with expected output.

## Stretch Scope

Only do these after the committed scope is complete:

- Full Helm install of API, worker, and dashboard into kind.
- Worker loop mode instead of one tick per invocation.
- Cancel queued/running run support.
- Store Kubernetes Job name on the run or attempt record.
- OpenAPI document generation for the API.
- Dashboard reads real run list and logs.
- Object storage adapter for logs.

## Explicit Non-Goals

- no artifact upload
- no MinIO or S3 integration
- no script bundle download from object storage
- no retry policy
- no scheduler or automation triggers
- no dashboard backend integration
- no authentication
- no production-grade reconciliation loop
- no multi-namespace execution model unless required by the chosen local setup
- no custom Kubernetes operator

## Definition of Done

Sprint 003 is done when:

- `job_hello_python` can run as a Kubernetes Job in a local cluster.
- The worker records final run state from the Kubernetes Job outcome.
- Captured bounded logs are retrievable through API and CLI.
- Execution pool resource and placement settings are applied to the Job spec.
- The local Kubernetes workflow is documented and repeatable.
- Existing Sprint 002 API, CLI, worker, and persistence tests still pass.
- Sprint 004 can start on object storage, retries, dashboard integration, or install packaging without reworking the runner boundary.

## Suggested Work Order

1. Add Kubernetes client dependencies and a narrow Kubernetes runner module.
2. Add pure unit tests for Kubernetes Job spec rendering.
3. Implement minimal Job spec rendering from a job definition and execution pool.
4. Extend the worker store to load job definitions.
5. Add PostgreSQL job definition lookup support and tests.
6. Wire worker runner selection between stub and Kubernetes runner.
7. Implement Kubernetes Job create and terminal watch.
8. Run the first manual kind/minikube smoke test.
9. Add bounded log persistence migration and adapter methods.
10. Capture pod logs after Job completion.
11. Add API logs endpoint.
12. Add CLI `status` and `logs` commands.
13. Update Helm values/templates for runner config and RBAC.
14. Write local Kubernetes runner documentation.
15. Run fmt, clippy, workspace tests, helm lint/template, and the manual smoke checklist.

## Sprint Review Checklist

- Can a contributor run the documented local Kubernetes flow from a fresh checkout?
- Does the worker still work with the stub runner for fast local tests?
- Is the Kubernetes runner hidden behind the runner boundary?
- Are execution pools applied by Kubernetes-native scheduling fields instead of manual node selection?
- Are logs bounded and documented as temporary PostgreSQL-backed storage?
- Are Kubernetes errors understandable from worker output?
- Did the implementation introduce a durable architecture decision that needs an ADR?

## Sprint 004 Preview

Sprint 004 should likely choose one of these paths based on Sprint 003 results:

- object storage for script bundles, logs, and artifacts
- dashboard integration with real run and log APIs
- Helm install path with bundled local dependencies
- retry, timeout, and lease recovery hardening
