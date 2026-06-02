# Sprint 004: Job Control and Recovery

## Sprint Goal

Turn the Sprint 003 Kubernetes runner from a happy-path executor into a controllable and recoverable job system: users can cancel runs, timeouts are classified clearly, failed runs can retry under a bounded policy, expired leases are recovered, and completed Kubernetes Jobs do not pile up forever.

By the end of this sprint, Capsulet should support this local evaluation flow:

1. Submit `job_hello_python` and see it run through Kubernetes.
2. Submit a long-running or failing job definition.
3. Cancel a queued or running run through API and CLI.
4. Observe timeout, failure, retry, cancellation, and success states as distinct outcomes.
5. Restart the worker without leaving leased work stuck forever.
6. Confirm completed Kubernetes Jobs are cleaned up according to a configured policy.

This sprint should finish the practical control loop around single-job execution before adding object storage, dashboard integration, or workflow features.

## Sprint Length

Recommended: 1 week.

## Sprint Theme

Control, recover, and explain.

The goal is not a full production reconciler. The goal is a small, testable set of state transitions that makes local and early alpha usage honest when jobs fail, hang, or are cancelled.

## Current Context

Sprint 003 completed:

- Kubernetes Job creation through the runner boundary.
- Static execution pool application from Helm values.
- Bounded PostgreSQL-backed run logs through a generic log storage boundary.
- API and CLI commands for submit, list, status, and logs.
- Helm install path for API and worker with local minikube smoke validation.

Remaining Phase 1 gaps from the roadmap:

- cancel queued or running jobs
- distinguish timeout from generic failure
- handle worker restart without losing leased jobs
- make failed execution behavior understandable

Phase 2 also introduces retry policy. Sprint 004 should implement a narrow retry slice now because it shares the same state machine and worker recovery work.

## Committed Scope

### 1. Job State Machine Hardening

Make job state transitions explicit enough for cancellation, timeout, retry, and recovery.

Expected work:

- Review current `JobRunStatus` transitions.
- Add or enforce transitions for `cancelled`, `timed_out`, and `retry_scheduled`.
- Ensure illegal transitions return clear domain errors.
- Make repository writes use guarded updates where a stale worker should not overwrite terminal state.
- Document the supported Sprint 004 state machine in code and docs.

Acceptance criteria:

- Unit tests cover allowed and rejected transitions.
- A worker cannot mark a run succeeded after the run has already been cancelled.
- Terminal states are stable: `succeeded`, `failed`, `cancelled`, and `timed_out` are not overwritten by normal worker ticks.
- Docs explain the state machine users can observe through API and CLI.

### 2. Run Cancellation API and CLI

Expose cancellation as a first-class user action.

Expected API:

```text
POST /v1/jobs/runs/{id}/cancel
```

Expected CLI:

```text
capsulet cancel <run-id>
```

Expected behavior:

- queued runs move directly to `cancelled`
- leased/running runs move to a cancellation state or cancellation request marker
- terminal runs return a clear no-op or conflict response
- missing runs return the existing not-found style error

Acceptance criteria:

- API tests cover queued cancellation, running cancellation request, terminal run behavior, and missing run behavior.
- CLI tests cover parsing and output formatting.
- `capsulet status <run-id>` shows cancellation state clearly.

### 3. Kubernetes Cancellation Path

Teach the worker/runner to stop Kubernetes work for cancelled running runs.

Expected work:

- Store or deterministically derive the Kubernetes Job name for a run.
- Give the running execution path a way to observe cancellation while waiting for Job completion.
- Delete or patch the Kubernetes Job according to the chosen cancellation strategy.
- Capture final logs when available, without making missing logs fatal.
- Mark the run `cancelled` after Kubernetes work is stopped or confirmed gone.

Recommended implementation:

- Keep Kubernetes deletion inside `capsulet-runner`, not the API.
- Let the worker poll cancellation state between Kubernetes Job status checks.
- Use deterministic run-derived Job names for deletion in Sprint 004; storing the Job name can be added if needed.

Acceptance criteria:

- A local long-running Kubernetes Job can be cancelled from CLI.
- The Kubernetes Job is removed or reaches a stopped state after cancellation.
- The run ends as `cancelled`, not `failed`.
- Worker logs explain cancellation progress and Kubernetes deletion errors.

### 4. Timeout Classification

Separate user-code failure from timeout.

Expected work:

- Expand runner outcome or worker mapping so active-deadline/elapsed timeout becomes `timed_out`.
- Preserve non-zero script exits as `failed`.
- Store timeout logs when Kubernetes provides them.
- Add a seeded or test-only job definition that reliably times out.

Acceptance criteria:

- Unit tests cover timeout outcome mapping.
- A manual local timeout smoke test reaches `timed_out`.
- CLI status renders `timed_out`.
- Timeout behavior is documented with the relevant execution pool setting.

### 5. Retry Policy Slice

Add a minimal retry policy for failed or timed-out runs.

Expected work:

- Add retry policy fields to the domain model or job definition shape.
- Persist retry configuration where needed.
- On `failed` or `timed_out`, schedule retry if attempts remain.
- Move retry-ready runs back to `queued`.
- Keep cancellation terminal and never retry cancelled runs.

Recommended constraints:

- Support only fixed delay and max attempts in Sprint 004.
- Keep retry scheduling in PostgreSQL/worker logic; do not introduce the scheduler service yet unless strictly necessary.
- Defer exponential backoff, jitter, dead-letter queues, and per-error retry classification.

Acceptance criteria:

- A failing job can retry up to the configured maximum.
- Attempt count increments correctly.
- Exhausted retries end in `failed` or `timed_out`.
- Tests cover retry scheduling, retry exhaustion, and no retry after cancellation.

### 6. Lease Expiry and Worker Restart Recovery

Recover runs that were leased or running when a worker died.

Expected work:

- Use existing lease owner and lease expiry fields as the recovery boundary.
- Add a repository method to requeue expired non-terminal leased/running runs.
- Have the worker call recovery before or during polling.
- Avoid requeueing runs that already have a live Kubernetes Job unless the chosen Sprint 004 behavior explicitly handles that case.

Recommended implementation:

- Start with leased runs that never reached Kubernetes Job creation.
- For running runs with an existing Job, either reattach by deterministic Job name or document deferral if that is too large.

Acceptance criteria:

- Tests cover expired lease recovery.
- A worker restart does not leave a queued-capable run stuck in `leased`.
- Recovery is idempotent.
- Any deferred running-Job reattachment case is documented as a Sprint 005 risk if not completed.

### 7. Kubernetes Job Cleanup Policy

Prevent local clusters from accumulating completed runner Jobs forever.

Expected work:

- Add Helm/worker config for cleanup behavior.
- Prefer Kubernetes `ttlSecondsAfterFinished` when available.
- Optionally add delete-after-completion behavior only if TTL is insufficient.
- Keep cleanup configurable and disabled only when useful for debugging.

Acceptance criteria:

- Rendered Jobs include configured TTL when enabled.
- Helm values schema covers cleanup settings.
- Local Kubernetes docs explain how to inspect completed Jobs before TTL cleanup.
- Unit tests cover Job spec cleanup fields.

### 8. Documentation and Smoke Checklist

Update docs for the new control and recovery behavior.

Expected work:

- Update API docs with cancel endpoint and status meanings.
- Update CLI docs/examples with `cancel`.
- Update local Kubernetes runner guide with cancel, timeout, retry, and cleanup checks.
- Update worker runner docs with cancellation, timeout, retry, and lease recovery behavior.
- Add troubleshooting notes for stuck `leased`, stuck `running`, and Kubernetes delete failures.

Acceptance criteria:

- A contributor can run manual smoke tests for success, failure, timeout, retry, and cancellation.
- Docs call out which behavior is still limited in Sprint 004.

### 9. Quality and Regression Coverage

Keep the Sprint 003 quality bar intact.

Acceptance criteria:

- `cargo fmt --check` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes.
- `helm lint charts/capsulet` passes.
- `helm template capsulet charts/capsulet` passes with local runner values.
- Manual minikube smoke checklist records success, cancellation, timeout, retry, and cleanup outcomes.

## Stretch Scope

Only do these after committed scope is complete:

- Store Kubernetes Job name and pod name on attempt records.
- Reattach to already-created Kubernetes Jobs after worker restart.
- Add dashboard controls for cancel/status/logs.
- Add object-storage-backed large logs.
- Add exponential backoff and jitter.
- Add dead-letter terminal state.
- Add metrics for cancellation, retry, timeout, and recovery counts.

## Explicit Non-Goals

- no workflow engine
- no automation triggers
- no dashboard API integration unless all committed scope is done
- no MinIO or object storage implementation
- no authentication or authorization
- no multi-worker concurrency tuning beyond correctness of guarded writes
- no full Kubernetes reconciler
- no custom Kubernetes operator
- no streaming logs

## Definition of Done

Sprint 004 is done when:

- queued runs can be cancelled through API and CLI
- running Kubernetes Jobs can be cancelled and reflected as `cancelled`
- timeouts end as `timed_out`, not generic `failed`
- failed or timed-out runs can retry with a bounded fixed-delay policy
- expired leases can be recovered without corrupting terminal state
- completed Kubernetes Jobs honor a cleanup policy
- docs and manual smoke tests cover success, failure, timeout, retry, cancellation, and recovery
- existing Sprint 003 Kubernetes execution, logs, Helm install, and CLI flows still pass

## Suggested Work Order

1. Tighten the domain state machine tests and transition rules.
2. Add guarded repository update methods for cancellation and terminal writes.
3. Add cancel API endpoint and tests.
4. Add CLI `cancel` command and tests.
5. Add runner/worker cancellation observation while waiting for Kubernetes Jobs.
6. Add local long-running seeded job definition or fixture for cancellation smoke.
7. Add timeout outcome mapping and tests.
8. Add retry policy model, persistence, and worker scheduling behavior.
9. Add expired lease recovery query and worker hook.
10. Add Kubernetes Job TTL cleanup setting and chart values/schema updates.
11. Update API, CLI, worker, Helm, and local Kubernetes docs.
12. Run fmt, clippy, workspace tests, helm lint/template, and minikube smoke.

## Sprint Review Checklist

- Can a user cancel a queued run and see `cancelled` immediately?
- Can a user cancel a running Kubernetes Job and see Kubernetes work stop?
- Are timeout, failure, cancellation, and retry states distinguishable in API and CLI output?
- Does retry behavior avoid retrying cancelled runs?
- Does a restarted worker recover expired leases safely?
- Are terminal states protected from stale worker writes?
- Do completed Jobs clean up without removing logs from Capsulet?
- Are known recovery limitations documented plainly?

## Sprint 005 Preview

Sprint 005 should choose one of these paths based on Sprint 004 results:

- object storage for script bundles, large logs, and artifacts
- dashboard integration with real run, status, cancel, and log APIs
- bundled PostgreSQL and MinIO chart dependencies for local alpha installs
- observability metrics for queue depth, attempts, retries, and worker outcomes
