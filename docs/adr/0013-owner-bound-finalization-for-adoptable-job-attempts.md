# ADR 0013: Owner-Bound Finalization for Adoptable Job Attempts

Status: Accepted

## Context

Workers maintain a PostgreSQL lease while a runner is active. A Kubernetes-backed run can outlive
its worker, so after lease expiry another worker adopts the existing `running` attempt and
reattaches to the deterministic Kubernetes Job. Adoption intentionally leaves `attempt_count`
unchanged because execution has not restarted.

Previously, finalization matched only the run ID, attempt count, and `running` status. A stale
worker could therefore finish writing logs or artifacts after its lease expired, race with an
adopting worker, and commit a terminal or retry state for the attempt now owned by that worker.
The attempt count could not distinguish the owners because adoption preserves it.

Heartbeats already checked `lease_owner`, but finalization did not. The deployment contract also
needs unique worker IDs: two concurrent workers with the same ID are indistinguishable to an
owner-bound guard.

## Decision

Treat `lease_owner` as part of the optimistic concurrency fence for finalization. A terminal or
retry update must atomically match all of the following:

- the run ID;
- the execution attempt count;
- `status = running`; and
- `lease_owner` equal to the finalizing worker's ID.

Adoption replaces `lease_owner` and renews the lease without incrementing `attempt_count`. A
finalization whose ownership fence no longer matches updates no row, leaving the adopted attempt
under its current owner.

Every concurrently active worker must have a distinct `CAPSULET_WORKER_ID`. The Helm deployment
uses the pod name through the Kubernetes downward API. Operators starting workers outside Helm
must supply unique IDs themselves.

## Consequences

- A worker result cannot finalize an attempt after another worker adopts that attempt, even though
  both executions carry the same attempt count.
- Cancellation and later execution attempts remain protected by the existing status and attempt
  guards.
- No schema migration or additional ownership-generation column is required.
- Concurrent reuse of a worker ID violates the fencing invariant and remains an operator error
  outside the managed Helm deployment.
- Recovery remains at-least-once. Logs and artifact bytes may be written before finalization, so
  this decision fences run state but does not claim exactly-once external side effects.

