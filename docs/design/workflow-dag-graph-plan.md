# Workflow DAG Graph Plan

## Goal

Introduce graph data structures and algorithms into Capsulet by evolving workflow definitions from a strictly linear ordered list into a directed acyclic graph.

This is intentionally planned as post-MVP work. The current linear workflow model remains useful and should become the simplest graph shape: a chain.

## Why This Fits Capsulet

Workflows are naturally graph-shaped once a user wants branching, fan-out, fan-in, or independent steps that can run in parallel. A graph model would improve the product while creating a real place to learn and apply graph algorithms.

Examples:

```text
extract_customers ─┐
                   ├─> merge_reports ─> send_email
extract_orders ────┘

cleanup_temp_files ────────────────┘
```

The graph should be a DAG:

- directed: dependencies point from prerequisite step to dependent step
- acyclic: a workflow cannot depend on itself through any path
- finite: all nodes and edges belong to one workflow definition

## Product Model

### Workflow Step

Existing workflow steps remain the graph nodes.

Current fields that still matter:

- `id`
- `workflow_id`
- `name`
- `job_definition_id`
- `execution_pool`

The existing `position` field can remain for display ordering, legacy linear workflows, or deterministic tie-breaking.

### Workflow Step Dependency

Add an edge model:

```rust
pub struct WorkflowStepDependency {
    from_step_id: WorkflowStepId,
    to_step_id: WorkflowStepId,
}
```

Meaning:

- `from_step_id` must finish successfully before `to_step_id` can start.
- A step with no incoming dependencies is a root step.
- A step with multiple incoming dependencies is a fan-in step.
- A step with multiple outgoing dependencies is a fan-out step.

### Workflow Graph

Add a domain object that is built from a workflow definition, its steps, and its dependencies:

```rust
pub struct WorkflowGraph {
    nodes: BTreeMap<WorkflowStepId, WorkflowStep>,
    outgoing: BTreeMap<WorkflowStepId, Vec<WorkflowStepId>>,
    incoming: BTreeMap<WorkflowStepId, Vec<WorkflowStepId>>,
}
```

This object should own graph validation and scheduling decisions. Persistence should store rows; domain code should decide whether the graph is valid and what can run next.

## Algorithms To Implement

### 1. Graph Construction

Build adjacency maps from steps and dependencies.

Validation rules:

- every dependency references existing steps
- no duplicate dependency edges
- no self-edge such as `A -> A`
- every step belongs to the workflow being validated

### 2. Cycle Detection

Reject cycles before storing or enabling a workflow.

Example invalid graph:

```text
A -> B -> C -> A
```

Recommended algorithm:

- depth-first search with temporary/permanent marks
- or Kahn's algorithm and compare visited count with node count

Good first implementation:

```text
if topological_sort(graph).len() != graph.node_count() {
    return Err(WorkflowGraphError::CycleDetected);
}
```

### 3. Topological Sort

Produce a deterministic valid execution order.

Use cases:

- validate that a workflow can be executed
- display a stable graph order in the API/dashboard
- provide deterministic scheduling tie-breakers

Recommended algorithm:

- Kahn's algorithm
- when multiple nodes are ready, sort by current `position` then `id`

### 4. Ready Step Discovery

During a workflow run, find all steps that can start now.

A step is ready when:

- it has not already started
- all incoming dependencies have succeeded
- the workflow run is not terminal

This enables parallel execution later, even if the first implementation still starts only one ready step per scheduler tick.

### 5. Failure Propagation

When a step fails, decide what happens to downstream steps.

Initial policy:

- if any prerequisite fails, dependent steps become skipped or blocked
- the workflow run becomes failed once no more runnable work remains

Future policies:

- continue-on-failure edges
- optional dependencies
- cleanup steps that always run

### 6. Critical Path

Later, compute the longest dependency path through the workflow.

Use cases:

- explain why a workflow takes as long as it does
- identify bottleneck steps
- estimate completion time when step duration history exists

This can be deferred until DAG execution works.

## API Shape

Extend workflow create/update requests with dependencies:

```json
{
  "id": "workflow_daily_report",
  "name": "Daily report",
  "steps": [
    {
      "id": "extract_customers",
      "name": "Extract customers",
      "job_definition_id": "job_extract_customers",
      "execution_pool": "mini"
    },
    {
      "id": "extract_orders",
      "name": "Extract orders",
      "job_definition_id": "job_extract_orders",
      "execution_pool": "mini"
    },
    {
      "id": "merge_reports",
      "name": "Merge reports",
      "job_definition_id": "job_merge_reports",
      "execution_pool": "mini"
    }
  ],
  "dependencies": [
    {
      "from_step_id": "extract_customers",
      "to_step_id": "merge_reports"
    },
    {
      "from_step_id": "extract_orders",
      "to_step_id": "merge_reports"
    }
  ]
}
```

For backward compatibility, a workflow without `dependencies` can be interpreted as the current linear chain by `position`.

## Persistence Plan

Add a table:

```sql
CREATE TABLE workflow_step_dependencies (
    workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    from_step_id TEXT NOT NULL REFERENCES workflow_steps(id) ON DELETE CASCADE,
    to_step_id TEXT NOT NULL REFERENCES workflow_steps(id) ON DELETE CASCADE,
    PRIMARY KEY (workflow_id, from_step_id, to_step_id),
    CHECK (from_step_id <> to_step_id)
);
```

Indexes:

```sql
CREATE INDEX workflow_step_dependencies_from_idx
    ON workflow_step_dependencies (workflow_id, from_step_id);

CREATE INDEX workflow_step_dependencies_to_idx
    ON workflow_step_dependencies (workflow_id, to_step_id);
```

The database can enforce referential integrity and self-edge rejection. The Rust domain should enforce cycle detection and unknown logical states.

## Scheduler Plan

Current sequential behavior:

1. workflow run starts at one current position
2. scheduler starts one job run
3. scheduler advances to the next position after success

DAG behavior:

1. scheduler loads workflow graph
2. scheduler loads completed, running, failed, cancelled, and pending step runs
3. scheduler asks `WorkflowGraph` for ready steps
4. scheduler creates job runs for ready steps
5. scheduler marks the workflow succeeded when all nodes succeeded
6. scheduler marks the workflow failed when a failed node blocks all remaining downstream work

Initial implementation can still create only one ready step per tick. That keeps concurrency changes small while validating the graph model.

## Dashboard Plan

Start with a non-visual dependency editor before building a graph canvas.

Phase 1:

- keep the existing step list
- add "depends on" multi-select per step
- show validation errors for cycles and missing dependencies
- display a topological execution preview

Phase 2:

- show a read-only graph view
- highlight root, fan-out, fan-in, running, succeeded, failed, and blocked nodes

Phase 3:

- add a visual graph editor

## Testing Plan

Core graph tests:

- accepts a linear chain
- accepts fan-out
- accepts fan-in
- rejects unknown dependency endpoints
- rejects self-dependencies
- rejects direct cycles
- rejects indirect cycles
- returns deterministic topological order
- returns all ready root steps for a new run
- returns fan-in step only after all prerequisites succeed

Persistence tests:

- saves and loads workflow dependencies
- cascades dependencies when workflow is deleted
- rejects duplicate edges

API tests:

- creates a workflow with dependencies
- rejects workflow creation when dependencies contain a cycle
- returns dependencies in workflow detail
- preserves current linear workflow behavior when dependencies are omitted

Scheduler tests:

- starts all root steps over successive ticks
- starts downstream step after prerequisites succeed
- does not start downstream step after prerequisite failure
- marks workflow succeeded after all graph nodes succeed
- marks workflow failed when no runnable work remains

## Suggested Implementation Slices

### Slice 1: Pure Domain Graph

Files:

- `crates/core/src/domain/workflow_graph.rs`
- `crates/core/src/domain/workflow.rs`
- `crates/core/src/domain/mod.rs`
- `crates/core/src/lib.rs`

Build only the in-memory graph type and tests. No database or API changes yet.

### Slice 2: Persistence

Files:

- `migrations/<timestamp>_workflow_step_dependencies.sql`
- `crates/postgres/src/workflows.rs`
- `crates/postgres/src/rows.rs`
- `crates/postgres/src/tests.rs`

Store and load dependencies with workflow definitions.

### Slice 3: API

Files:

- `crates/api/src/models.rs`
- `crates/api/src/http.rs`
- `crates/api/src/tests.rs`
- `docs/api.md`

Accept and return dependencies.

### Slice 4: Scheduler

Files:

- `crates/postgres/src/workflow_runs.rs`
- `crates/scheduler/src/lib.rs`
- `crates/api/src/tests.rs` or scheduler-specific tests

Use ready-step discovery to advance workflow runs.

### Slice 5: Dashboard

Files:

- `dashboard/app/automations/page.tsx`
- `dashboard/app/components.tsx`
- `dashboard/app/lib/api.ts`
- new workflow editor components if the page becomes too large

Add dependency selection and execution preview.

## Open Questions

- Should the first DAG scheduler start multiple ready steps in one tick, or one per tick?
- Should failed prerequisites mark downstream steps as `skipped`, `blocked`, or leave them unstarted?
- Should dependencies be allowed only within one workflow version?
- Should workflow definitions become immutable once a run exists?
- Should graph validation happen on every save, only when enabling, or both?

## Recommendation

Start with Slice 1 only. It gives a focused graph data structure and real algorithms without forcing persistence or UI decisions too early.

Once the domain graph is well tested, the remaining slices become straightforward integration work.
