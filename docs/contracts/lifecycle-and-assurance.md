# Lifecycle and Assurance Contract

Execution state and assurance are independent dimensions. A run may complete successfully while
its output remains `unverified`, and a rejected proposal may have been produced by a technically
successful process. No API, dashboard view, metric, automation condition, or documentation may
infer assurance from execution success.

The machine-readable inventory is [lifecycle-mapping.json](lifecycle-mapping.json). Its checker
compares every listed lifecycle to the corresponding Rust enum so new persisted or API-visible
statuses cannot appear without documentation.

## Target execution status

The future unified runtime uses six execution concepts:

| Status | Meaning |
| --- | --- |
| `queued` | Accepted for execution but not actively owned. |
| `running` | Actively executing or held by an execution owner. |
| `waiting` | Durably suspended for a retry, timer, event, approval, or dependency. |
| `completed` | Execution finished without an execution failure. |
| `failed` | Execution ended because it could not continue successfully. |
| `cancelled` | Execution ended because an authorized actor or policy cancelled it. |

These are target concepts, not a request to rename current enums. Current states such as `leased`,
`retry_scheduled`, `timed_out`, `skipped`, `removed`, and `stopped` retain information that a
six-state view can lose. The mapping records each loss as M2/M3 debt; future events and typed
failure reasons must preserve it.

## Platform assurance verdict

The platform assurance layer has four verdicts:

| Verdict | Meaning |
| --- | --- |
| `unverified` | The correctness plane did not run for this value or boundary. |
| `accepted` | All required checks discharged under the pinned policy and evidence. |
| `conditional` | The result is valid only with named residual assumptions or obligations. |
| `rejected` | A required premise or policy check failed. |

`capsulet-kernel::Verdict` remains deliberately three-valued: `accepted`, `conditional`, and
`rejected`. The kernel only emits a verdict after it runs, so `unverified` is not a fourth kernel
conclusion. It belongs to the surrounding platform, where it explicitly records that no kernel or
required verifier was invoked.

Execution and assurance therefore form a product, not a single ladder. Representative combinations
include `completed + unverified`, `completed + conditional`, `failed + unverified`, and
`completed + rejected`. UI and API models must render both dimensions when assurance exists.

## Current lifecycle ownership

| Lifecycle | Category | Transition owner | Important current behavior |
| --- | --- | --- | --- |
| Job run | execution | API, lease-owning worker, retry/lease recovery | Owner-bound finalization fences stale workers; timeout and retry are explicit. |
| Workflow definition | configuration | API | Draft/enabled/disabled controls compatibility scheduling. |
| Workflow run | execution | API and scheduler | Removal is distinct from cancellation; failed/timed-out runs may resume from checkpoints. |
| Workflow step run | execution | scheduler and worker reconciliation | Uses the workflow-run enum; skipped and removed mappings are lossy. |
| Agent run | execution | API and application agent runtime | Static in-process runtime only; no dedicated production agent worker. |
| Automation | configuration | API | Enabled/disabled controls trigger evaluation. |
| Ingestion run | execution | API ingestion service | No current cancellation or retry state. |
| Memory claim | review | ingestion, reviewer, memory governance | Active/contradicted are governance state, not execution or assurance. |
| Memory subgraph | configuration | API after invariant checks | Activation validates ownership, schema, permissions, and summary trace requirements. |
| Entity resolution | review | ingestion and reviewer | Confirmation requires evidence. |
| Claim conflict | review | memory governance and reviewer | Resolution/dismissal does not imply correctness-plane assurance. |
| Kernel verdict | assurance | deterministic kernel | Terminal result of one check; never inferred from a run status. |

The JSON inventory is normative for terminality, retryability, cancellability, target mapping, and
allowed transition ownership. This prose explains the model and does not replace the exact rows.

## Rules for consumers

1. Store, transmit, filter, and display execution and assurance independently.
2. Never translate `succeeded` or target `completed` into `accepted`.
3. Never translate `failed`, `timed_out`, `cancelled`, or `stopped` into `rejected`; those statuses
   say why execution ended, not what a checker concluded.
4. Absence of a certificate or required check is `unverified`, not `accepted` and not `conditional`.
5. Review/configuration statuses such as `active`, `confirmed`, or `enabled` are neither execution
   states nor assurance verdicts.
6. A future compatibility reader must preserve the original status and typed reason whenever a
   mapping to the six target execution concepts is lossy.
