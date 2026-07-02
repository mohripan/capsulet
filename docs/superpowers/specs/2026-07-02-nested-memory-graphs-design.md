# Nested Memory Graphs Design

Date: 2026-07-02

## Purpose

Capsulet needs nested graph memory as a first-class model, not as a visualization feature or a tag on existing records. A graph is a memory context, a subgraph is a bounded memory module, and a nested subgraph is memory inside memory. The platform must support both context-centered navigation and entity-centered zooming from day one.

The chosen approach is Strict Hybrid Core:

- Contexts are first-class through `MemorySubgraph`.
- Shared real-world identity is first-class through `CanonicalEntity`.
- Local entities remain context-specific and resolve to canonical identities with evidence.
- A canonical entity can open into an attached nested graph.
- Cross-subgraph links are explicit boundary objects, not implicit ordinary relationships.

This gives Capsulet multi-resolution memory, bounded retrieval, graph-native permissions, contextual contradictions, traceable summaries, and modular memory ownership.

## Goals

- Enforce that every active subgraph has an owner, schema, permissions, summary, and traceable evidence.
- Allow subgraphs to contain memory records and child subgraphs.
- Allow canonical entities to expose nested entity graphs.
- Require explicit cross-subgraph edges for relationships that cross memory boundaries.
- Preserve context-specific disagreement instead of flattening everything into one global truth.
- Make summaries compact enough for retrieval while preserving traceability to inner claims and evidence.

## Future Roadmap Goals

These are product goals, but they should follow the first nested memory graph implementation instead of being bundled into the initial milestone.

- Build a full graph query language for traversing nested contexts, canonical entities, boundary edges, summaries, evidence, and time.
- Build a visual graph workbench for inspecting, debugging, merging, rejecting, approving, and auditing nested graph memory.
- Deeply integrate claim-level memory, evidence, memory contracts, nested subgraphs, canonical identity, and runtime retrieval into one coherent memory system.
- Implement a complete enterprise policy engine for graph-native permissions, access review, retention, audit, and compliance. The first implementation stores permissions as structured policy data and enforces required presence.

## Domain Model

### MemorySubgraph

`MemorySubgraph` is a bounded memory context. It represents objects such as company memory, legal memory, engineering memory, customer memory, project memory, incident memory, or personal memory.

Fields:

- `id`
- `scope`
- `parent_subgraph_id`
- `name`
- `description`
- `owner_kind`
- `owner_id`
- `contract_id`
- `summary_claim_id`
- `permissions`
- `status`
- `created_at`
- `updated_at`

Status values:

- `draft`
- `active`
- `archived`

Draft subgraphs can be incomplete. Active subgraphs must satisfy all activation invariants.

### CanonicalEntity

`CanonicalEntity` represents the stable identity for a shared real-world thing across subgraphs. Local `Entity` records remain useful because different contexts can use different names, aliases, and evidence.

Fields:

- `id`
- `scope`
- `entity_type`
- `display_name`
- `aliases`
- `created_at`
- `updated_at`

### EntityResolution

`EntityResolution` maps a local `Entity` to a `CanonicalEntity` inside a subgraph.

Fields:

- `id`
- `scope`
- `subgraph_id`
- `entity_id`
- `canonical_entity_id`
- `confidence`
- `status`
- `evidence_ids`
- `created_at`
- `updated_at`

Resolution status values:

- `candidate`
- `confirmed`
- `rejected`

### SubgraphMembership

`SubgraphMembership` declares that a memory object belongs to a subgraph.

Member kinds:

- `source`
- `evidence`
- `entity`
- `canonical_entity`
- `claim`
- `event`
- `relationship`
- `subgraph`

Roles:

- `member`
- `summary`
- `inner_claim`
- `evidence`
- `canonical_identity`
- `child_context`

The implementation should start with a constrained enum and allow new roles only through code changes, not arbitrary user strings.

### SubgraphEdge

`SubgraphEdge` is the explicit boundary object for cross-subgraph connections.

Fields:

- `id`
- `scope`
- `edge_type`
- `from_subgraph_id`
- `to_subgraph_id`
- `from_member_kind`
- `from_member_id`
- `to_member_kind`
- `to_member_id`
- `claim_ids`
- `evidence_ids`
- `created_at`
- `updated_at`

Examples:

- `contradicts`
- `depends_on`
- `summarizes`
- `supersedes`
- `references`
- `exposes_summary_to`

Ordinary `Relationship` records are local to their memory context. If the relationship crosses a subgraph boundary, it must be represented by `SubgraphEdge`.

### SummaryTrace

`SummaryTrace` makes a summary claim auditable.

Fields:

- `id`
- `scope`
- `subgraph_id`
- `summary_claim_id`
- `inner_claim_ids`
- `evidence_ids`
- `created_at`

Every active subgraph must have at least one summary trace linking its summary to inner claims or evidence.

### EntityGraphAttachment

`EntityGraphAttachment` lets a canonical entity open into a nested graph.

Fields:

- `id`
- `scope`
- `canonical_entity_id`
- `subgraph_id`
- `attachment_type`
- `created_at`
- `updated_at`

Attachment types:

- `primary`
- `supporting`
- `historical`

There should be at most one primary graph attachment per canonical entity within a tenant/project scope.

## Invariants

Activation invariants for `MemorySubgraph`:

- `owner_kind` and `owner_id` are required.
- `contract_id` is required.
- `permissions` is required and must be valid structured policy data.
- `summary_claim_id` is required.
- The summary claim must belong to the subgraph.
- At least one `SummaryTrace` must exist for the summary claim.
- A summary trace must reference at least one inner claim or evidence record.

Boundary invariants:

- Cross-subgraph links must be represented by `SubgraphEdge`.
- `SubgraphEdge` endpoints must be in different subgraphs or explicitly describe parent-child exposure.
- A normal `Relationship` must not be treated as cross-subgraph unless there is a corresponding `SubgraphEdge`.

Identity invariants:

- Shared entities should resolve through `CanonicalEntity`.
- A local entity can map to one canonical entity per subgraph when resolution is confirmed.
- Candidate resolutions may coexist until review.
- Confirmed resolutions require evidence and confidence.

Summary invariants:

- Summaries are claims, not free-floating text.
- Summaries must be traceable to inner claims or evidence.
- Parent graphs consume child summaries by default and expand into child subgraphs only when retrieval requires more detail.

## Data Flow

Ingestion flow:

```text
raw source
  -> evidence
  -> local entities and claims
  -> subgraph membership
  -> canonical entity resolution
  -> summary claim
  -> summary trace
  -> subgraph activation
```

Retrieval flow:

```text
query
  -> choose seed subgraph or canonical entity
  -> inspect summary
  -> expand child subgraph or entity graph if needed
  -> follow explicit cross-subgraph edges
  -> collect allowed claims and evidence
```

Contradiction flow:

```text
claims inside one subgraph
  -> evaluate using the local memory contract

claims across subgraphs
  -> preserve contextual disagreement
  -> compare globally only through explicit boundary edges, canonical identity policy, or retrieval policy
```

## API Surface

Add these endpoints:

- `POST /v1/memory/subgraphs`
- `GET /v1/memory/subgraphs`
- `GET /v1/memory/subgraphs/{id}`
- `POST /v1/memory/subgraphs/{id}/members`
- `POST /v1/memory/subgraphs/{id}/activate`
- `POST /v1/memory/canonical-entities`
- `GET /v1/memory/canonical-entities`
- `POST /v1/memory/entity-resolutions`
- `POST /v1/memory/subgraph-edges`
- `POST /v1/memory/summary-traces`
- `POST /v1/memory/entity-graph-attachments`

Activation must fail with a validation error when required owner, schema, permissions, summary, or traceability data is missing.

## Storage

Add PostgreSQL tables:

- `memory_subgraphs`
- `memory_subgraph_members`
- `memory_canonical_entities`
- `memory_entity_resolutions`
- `memory_subgraph_edges`
- `memory_summary_traces`
- `memory_entity_graph_attachments`

Use relational constraints for required IDs, tenant/project scope, uniqueness, and lifecycle status. Store permissions as `jsonb` for the first implementation so policy shape can evolve without immediate schema churn.

## Testing

Core tests:

- draft subgraph can be created with partial activation data
- active subgraph requires owner, schema, permissions, summary, and trace
- summary trace requires at least one inner claim or evidence
- canonical entity resolution requires evidence and confidence
- confirmed entity resolution is unique per local entity and subgraph
- cross-subgraph edges are explicit
- canonical entity can attach a primary nested graph

Postgres/API tests:

- create subgraph
- add member claim and entity
- create canonical entity
- resolve local entity to canonical entity
- attach entity graph
- create summary trace
- activate subgraph
- create explicit cross-subgraph edge

End-to-end test:

Create Sales and Engineering subgraphs. Both refer to the same canonical customer or project entity. Sales claims an August target. Engineering claims a September target. Add summaries and traces, activate both subgraphs, then create an explicit `contradicts` subgraph edge. The test proves nested contexts, canonical identity, traceable summaries, activation gates, and boundary edges work together.

## Implementation Order

1. Add core domain IDs, enums, and structs for nested memory graphs.
2. Add domain validation for activation, traceability, identity resolution, and cross-subgraph boundaries.
3. Add PostgreSQL migration and store methods.
4. Add API request/response models and routes.
5. Add documentation and OpenAPI entries.
6. Add end-to-end tests using Docker-backed PostgreSQL.

## Deferred Roadmap Work

- Retrieval ranking across nested subgraphs.
- Permission enforcement beyond required policy presence.
- UI and graph visualization.
- Query convenience features after the core memory model is correct.
