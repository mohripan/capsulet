# Capsulet Dashboard Memory Studio Design

## Status

Approved direction for implementation planning.

## Context

Capsulet is moving from a workflow automation console toward a local-first AI memory platform. The dashboard should stop presenting the product as a job orchestration system and instead become the primary operating surface for governed graph memory.

The approved visual direction is a hybrid of:

- Memory Studio workbench: governance, review, schema, permissions, and evidence controls.
- Graph Explorer first: nested subgraphs and memory relationships are the main workspace.

The approved style is flat, dark, minimal, and Docker-blue accented. The latest approved mockup is `flat-dark-docker-memory-studio-v4.html` in the brainstorming companion session.

## Product Goal

The dashboard should make Capsulet feel like a memory operating console for AI agents. Users should be able to inspect, create, govern, and debug memory structures: nested subgraphs, canonical entities, claims, summaries, cross-subgraph edges, permissions, and traceability.

This implementation should replace the dashboard's current automation-first framing. Existing workflow pages can remain reachable while the product transition is underway, but the main navigation and home experience should orient around graph memory.

## Visual System

Use Tailwind CSS as the styling foundation.

The interface should use:

- Dark flat surfaces: app background around `#07131c`, panels around `#0b1822`, graph canvas around `#08151f`.
- Docker blue primary accent: `#2496ed`.
- Soft blue active states, for example `#0c2d44` backgrounds and light blue text.
- Low-contrast dividers rather than heavy solid outlines.
- Minimal shadows only for depth, not glow.
- Dashed or subtle graph edges instead of hard solid connection lines.
- Compact operational UI density similar to Grafana, but less visually noisy.
- 6-8px radii for panels, nav items, badges, graph nodes, and controls.

The design should avoid:

- Neon/glowing blue borders.
- Large marketing hero sections.
- White/light main dashboards for this first direction.
- Heavy one-color blue walls where every boundary is saturated.

## Information Architecture

The primary navigation should be memory-first:

- Graph Workbench
- Subgraphs
- Claims
- Entities
- Contradictions
- Schema Studio
- Agent Sessions
- Retrieval Policies
- Evaluations
- Settings

Legacy operational pages can remain available during migration, but should not dominate the primary dashboard framing.

The root route should redirect to or render the memory workbench experience. The old automation overview should no longer be the first product signal.

## Main Workbench

The main workbench uses a three-part layout:

1. Left sidebar navigation.
2. Central graph canvas.
3. Right inspector and governance panels.

The central graph canvas should show nested memory concepts:

- Root memory graph.
- Subgraphs as bounded memory modules.
- Summary nodes exposed to parent graphs.
- Canonical entities shared across subgraphs.
- Explicit cross-subgraph edges.
- Trace nodes or trace markers connecting summaries to inner claims.

The first implementation can use a deterministic SVG/HTML graph rendering rather than a full graph layout engine. The goal is to expose backend functionality and validate the product direction, not build a complete graph visualization package immediately.

## Governance Panels

The right-side inspector should show details for the selected graph object:

- Object type and identifier.
- Owner.
- Schema identifier.
- Parent graph.
- Summary node.
- Permission policy.
- Activation status.
- Traceability status.
- Timestamps where available.

Secondary panels should surface:

- Memory health metrics.
- Claim review inbox.
- Entity resolution candidates.
- Cross-subgraph edge audit.
- Summary trace evidence.

Empty states should be operational and direct. They should explain what is missing and offer the relevant creation action when the backend supports it.

## Frontend Data Model

Add dashboard API client support for the nested memory endpoints already added to the backend:

- Create and list memory subgraphs.
- Activate subgraphs.
- Create and list canonical entities.
- Create entity resolutions.
- Attach entities to subgraphs.
- Create summary traces.
- Create and list cross-subgraph edges.

Frontend types should mirror backend JSON names closely. Keep conversion logic small and localized in `dashboard/app/lib/api.ts` or focused memory-specific modules.

## Pages For First Implementation

The first implementation should include these user-facing pages:

- `/memory`: Graph Workbench overview with graph canvas, metrics, inspector, review inbox, and entity resolution panel.
- `/memory/subgraphs`: List/create/activate subgraphs and inspect owner/schema/permissions/summary-node fields.
- `/memory/entities`: List/create canonical entities, show aliases, and attach entities to subgraphs.
- `/memory/edges`: Create and inspect explicit cross-subgraph edges.
- `/memory/traces`: Create and inspect summary-to-claim trace records.

If time or complexity requires reducing scope, keep `/memory` and `/memory/subgraphs` first, then add the other pages in the same style.

## Forms And Interaction

Use dense but readable forms:

- Text inputs for IDs, owners, labels, schema IDs, and claim IDs.
- Selects for graph IDs and source/target graph IDs when data exists.
- Small primary buttons with icons for create/activate actions.
- Tables for lists.
- Badges for active/inactive, valid/missing, public/restricted, and traceable/untraced states.
- Tabs for workbench modes: Explore, Review, Evidence, Permissions.

Forms should validate required fields before submission and display backend errors without hiding the current data.

## Error Handling

Each page should:

- Load all needed API data with clear loading states.
- Display partial results when one API call fails and another succeeds.
- Show compact error banners with the failing operation name.
- Avoid crashing the full page because one memory section is unavailable.

Creation actions should:

- Disable submit while pending.
- Preserve form values on failure.
- Refresh affected data on success.

## Testing

Verification should include:

- `npm`/Next build or equivalent dashboard type/build check.
- Dashboard unit/API tests if existing patterns are available.
- Playwright smoke test for the memory workbench route.
- A live end-to-end test against the local Docker Compose backend and Postgres on the Capsulet-specific host port, not host `5432`.

The end-to-end test should prove that the frontend can load memory data created through the backend and that the new routes render without runtime errors.

## Open Decisions Resolved

- Visual direction: flat dark Docker-blue console.
- Product direction: Memory Studio plus Graph Explorer hybrid.
- First graph rendering approach: deterministic lightweight graph visualization.
- Styling framework: Tailwind CSS.
- Main product surface: memory-first dashboard, not automation-first dashboard.
