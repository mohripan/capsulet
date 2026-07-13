import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  CapsuletApiError,
  approveReviewClaim,
  activateMemorySubgraph,
  capsuletStreamUrl,
  confirmEntityResolution,
  createCanonicalEntity,
  createIngestionConnector,
  createMemorySubgraph,
  createSubgraphEdge,
  formatBytes,
  getErrorMessage,
  isTerminalStatus,
  dismissClaimConflict,
  listIngestionConnectors,
  listIngestionRuns,
  listReviewClaims,
  listCanonicalEntities,
  listEntityResolutions,
  listClaimConflicts,
  listMemorySubgraphs,
  rejectEntityResolution,
  rejectReviewClaim,
  resolveClaimConflict,
  runIngestionConnector
} from "../app/lib/api";

describe("dashboard API helpers", () => {
  it("formats artifact sizes", () => {
    assert.equal(formatBytes(20), "20 B");
    assert.equal(formatBytes(1536), "1.5 KiB");
    assert.equal(formatBytes(2 * 1024 * 1024), "2.0 MiB");
  });

  it("classifies terminal run statuses", () => {
    assert.equal(isTerminalStatus("queued"), false);
    assert.equal(isTerminalStatus("running"), false);
    assert.equal(isTerminalStatus("retry_scheduled"), false);
    assert.equal(isTerminalStatus("succeeded"), true);
    assert.equal(isTerminalStatus("failed"), true);
    assert.equal(isTerminalStatus("cancelled"), true);
    assert.equal(isTerminalStatus("timed_out"), true);
  });

  it("renders API errors with code and message", () => {
    const error = new CapsuletApiError("job artifact not found", 404, "job_artifact_not_found");
    assert.equal(getErrorMessage(error), "job_artifact_not_found: job artifact not found");
  });

  it("builds proxied stream URLs", () => {
    assert.equal(capsuletStreamUrl("/v1/events/stream"), "/api/capsulet/v1/events/stream");
    assert.equal(capsuletStreamUrl("v1/events/stream"), "/api/capsulet/v1/events/stream");
  });

  it("calls nested memory subgraph endpoints with backend JSON fields", async () => {
    const calls: Array<{ path: string; init: RequestInit | undefined }> = [];
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async (input, init) => {
      calls.push({ path: String(input), init });
      if (String(input).endsWith("/activate")) {
        return jsonResponse({
          id: "graph_project_atlas",
          tenant_id: "tenant",
          project_id: "project",
          parent_subgraph_id: null,
          name: "Project Atlas",
          description: "Migration memory",
          owner_kind: "team",
          owner_id: "engineering",
          contract_id: "contract_project",
          summary_claim_id: "claim_summary",
          permissions: { visibility: "restricted" },
          status: "active"
        });
      }
      if (init?.method === "POST") {
        return jsonResponse({
          id: "graph_project_atlas",
          tenant_id: "tenant",
          project_id: "project",
          parent_subgraph_id: null,
          name: "Project Atlas",
          description: "Migration memory",
          owner_kind: null,
          owner_id: null,
          contract_id: null,
          summary_claim_id: null,
          permissions: null,
          status: "draft"
        }, 201);
      }
      return jsonResponse({ subgraphs: [] });
    };

    try {
      await listMemorySubgraphs();
      await createMemorySubgraph({
        id: "graph_project_atlas",
        name: "Project Atlas",
        description: "Migration memory"
      });
      await activateMemorySubgraph("graph_project_atlas", {
        owner_kind: "team",
        owner_id: "engineering",
        contract_id: "contract_project",
        permissions: { visibility: "restricted" },
        summary_claim_id: "claim_summary"
      });
    } finally {
      globalThis.fetch = originalFetch;
    }

    assert.equal(calls[0].path, "/api/capsulet/v1/memory/subgraphs");
    assert.equal(calls[1].path, "/api/capsulet/v1/memory/subgraphs");
    assert.equal(calls[1].init?.method, "POST");
    assert.deepEqual(JSON.parse(String(calls[1].init?.body)), {
      id: "graph_project_atlas",
      name: "Project Atlas",
      description: "Migration memory"
    });
    assert.equal(calls[2].path, "/api/capsulet/v1/memory/subgraphs/graph_project_atlas/activate");
    assert.equal(calls[2].init?.method, "POST");
    assert.deepEqual(JSON.parse(String(calls[2].init?.body)), {
      owner_kind: "team",
      owner_id: "engineering",
      contract_id: "contract_project",
      permissions: { visibility: "restricted" },
      summary_claim_id: "claim_summary"
    });
  });

  it("calls canonical entity and cross-subgraph edge endpoints", async () => {
    const calls: Array<{ path: string; init: RequestInit | undefined }> = [];
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async (input, init) => {
      calls.push({ path: String(input), init });
      if (String(input).includes("subgraph-edges")) {
        return jsonResponse({
          id: "edge_sales_engineering",
          tenant_id: "tenant",
          project_id: "project",
          edge_type: "contradicts",
          from_subgraph_id: "sales",
          to_subgraph_id: "engineering",
          from_member_kind: "claim",
          from_member_id: "claim_sales",
          to_member_kind: "claim",
          to_member_id: "claim_eng",
          claim_ids: ["claim_sales", "claim_eng"],
          evidence_ids: []
        }, 201);
      }
      if (String(input).includes("entity-resolutions") && init?.method === "POST") {
        return jsonResponse({
          id: "resolution_customer_a",
          tenant_id: "tenant",
          project_id: "project",
          subgraph_id: "sales",
          entity_id: "entity_customer_a",
          canonical_entity_id: "canonical_customer_a",
          confidence: 0.88,
          status: String(input).endsWith("/reject") ? "rejected" : "confirmed",
          evidence_ids: ["evidence_customer_a"]
        });
      }
      if (String(input).includes("entity-resolutions")) {
        return jsonResponse({
          entity_resolutions: [
            {
              id: "resolution_customer_a",
              tenant_id: "tenant",
              project_id: "project",
              subgraph_id: "sales",
              entity_id: "entity_customer_a",
              canonical_entity_id: "canonical_customer_a",
              confidence: 0.88,
              status: "candidate",
              evidence_ids: ["evidence_customer_a"]
            }
          ]
        });
      }
      if (init?.method === "POST") {
        return jsonResponse({
          id: "canonical_customer_a",
          tenant_id: "tenant",
          project_id: "project",
          entity_type: "Customer",
          display_name: "Customer A",
          aliases: ["ACME"]
        }, 201);
      }
      return jsonResponse({ canonical_entities: [] });
    };

    try {
      await listCanonicalEntities();
      await createCanonicalEntity({
        id: "canonical_customer_a",
        entity_type: "Customer",
        display_name: "Customer A",
        aliases: ["ACME"]
      });
      const resolutions = await listEntityResolutions("candidate");
      assert.equal(resolutions.entity_resolutions[0].status, "candidate");
      await confirmEntityResolution("resolution_customer_a");
      await rejectEntityResolution("resolution_customer_a");
      await createSubgraphEdge({
        id: "edge_sales_engineering",
        edge_type: "contradicts",
        from_subgraph_id: "sales",
        to_subgraph_id: "engineering",
        from_member_kind: "claim",
        from_member_id: "claim_sales",
        to_member_kind: "claim",
        to_member_id: "claim_eng",
        claim_ids: ["claim_sales", "claim_eng"],
        evidence_ids: []
      });
    } finally {
      globalThis.fetch = originalFetch;
    }

    assert.equal(calls[0].path, "/api/capsulet/v1/memory/canonical-entities");
    assert.equal(calls[1].path, "/api/capsulet/v1/memory/canonical-entities");
    assert.equal(calls[1].init?.method, "POST");
    assert.deepEqual(JSON.parse(String(calls[1].init?.body)), {
      id: "canonical_customer_a",
      entity_type: "Customer",
      display_name: "Customer A",
      aliases: ["ACME"]
    });
    assert.equal(calls[2].path, "/api/capsulet/v1/memory/entity-resolutions?status=candidate");
    assert.equal(calls[3].path, "/api/capsulet/v1/memory/entity-resolutions/resolution_customer_a/confirm");
    assert.equal(calls[3].init?.method, "POST");
    assert.equal(calls[4].path, "/api/capsulet/v1/memory/entity-resolutions/resolution_customer_a/reject");
    assert.equal(calls[4].init?.method, "POST");
    assert.equal(calls[5].path, "/api/capsulet/v1/memory/subgraph-edges");
    assert.equal(calls[5].init?.method, "POST");
    assert.deepEqual(JSON.parse(String(calls[5].init?.body)), {
      id: "edge_sales_engineering",
      edge_type: "contradicts",
      from_subgraph_id: "sales",
      to_subgraph_id: "engineering",
      from_member_kind: "claim",
      from_member_id: "claim_sales",
      to_member_kind: "claim",
      to_member_id: "claim_eng",
      claim_ids: ["claim_sales", "claim_eng"],
      evidence_ids: []
    });
  });

  it("calls claim conflict review endpoints", async () => {
    const calls: Array<{ path: string; init: RequestInit | undefined }> = [];
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async (input, init) => {
      calls.push({ path: String(input), init });
      if (String(input).endsWith("/resolve")) {
        return jsonResponse({
          id: "conflict_launch_date",
          tenant_id: "tenant",
          project_id: "project",
          subject_id: "entity_project",
          canonical_entity_id: null,
          predicate: "launch_date",
          claim_ids: ["claim_july", "claim_august"],
          status: "resolved",
          reason: "Multiple active values for launch_date",
          preferred_claim_id: "claim_august"
        });
      }
      if (String(input).endsWith("/dismiss")) {
        return jsonResponse({
          id: "conflict_launch_date",
          tenant_id: "tenant",
          project_id: "project",
          subject_id: "entity_project",
          canonical_entity_id: null,
          predicate: "launch_date",
          claim_ids: ["claim_july", "claim_august"],
          status: "dismissed",
          reason: "Multiple active values for launch_date",
          preferred_claim_id: null
        });
      }
      return jsonResponse({
        conflicts: [
          {
            id: "conflict_launch_date",
            tenant_id: "tenant",
            project_id: "project",
            subject_id: "entity_project",
            canonical_entity_id: null,
            predicate: "launch_date",
            claim_ids: ["claim_july", "claim_august"],
            status: "candidate",
            reason: "Multiple active values for launch_date",
            preferred_claim_id: null
          }
        ]
      });
    };

    try {
      const conflicts = await listClaimConflicts("candidate");
      assert.equal(conflicts.conflicts[0].predicate, "launch_date");
      await resolveClaimConflict("conflict_launch_date", "claim_august");
      await dismissClaimConflict("conflict_launch_date");
    } finally {
      globalThis.fetch = originalFetch;
    }

    assert.equal(calls[0].path, "/api/capsulet/v1/memory/conflicts?status=candidate");
    assert.equal(calls[1].path, "/api/capsulet/v1/memory/conflicts/conflict_launch_date/resolve");
    assert.equal(calls[1].init?.method, "POST");
    assert.deepEqual(JSON.parse(String(calls[1].init?.body)), { preferred_claim_id: "claim_august" });
    assert.equal(calls[2].path, "/api/capsulet/v1/memory/conflicts/conflict_launch_date/dismiss");
    assert.equal(calls[2].init?.method, "POST");
  });

  it("calls connector ingestion endpoints with local text payloads", async () => {
    const calls: Array<{ path: string; init: RequestInit | undefined }> = [];
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async (input, init) => {
      calls.push({ path: String(input), init });
      if (String(input).endsWith("/runs") && init?.method === "POST") {
        return jsonResponse({
          run: {
            id: "ingestion_run_project_notes",
            tenant_id: "tenant",
            project_id: "project",
            connector_id: "connector_project_notes",
            status: "succeeded",
            error: null,
            source_count: 1,
            evidence_count: 2,
            entity_count: 1,
            claim_count: 2,
            event_count: 0,
            relationship_count: 0
          },
          outputs: {
            sources: ["source_1"],
            evidence: ["evidence_1"],
            entities: ["entity_1"],
            claims: ["claim_1", "claim_2"],
            events: [],
            relationships: []
          }
        }, 201);
      }
      if (init?.method === "POST") {
        return jsonResponse({
          id: "connector_project_notes",
          tenant_id: "tenant",
          project_id: "project",
          name: "Project notes",
          kind: "local_text",
          enabled: true,
          config: {
            title: "Project Atlas Notes",
            content_type: "text/markdown",
            uri: "local://project-atlas.md",
            authority: "high"
          }
        }, 201);
      }
      if (String(input).endsWith("/runs")) {
        return jsonResponse({ runs: [] });
      }
      return jsonResponse({ connectors: [] });
    };

    try {
      await listIngestionConnectors();
      await createIngestionConnector({
        id: "connector_project_notes",
        name: "Project notes",
        kind: "local_text",
        enabled: true,
        config: {
          title: "Project Atlas Notes",
          content: "# Project Atlas",
          content_type: "text/markdown",
          uri: "local://project-atlas.md",
          authority: "high"
        }
      });
      await runIngestionConnector("connector_project_notes");
      await listIngestionRuns();
    } finally {
      globalThis.fetch = originalFetch;
    }

    assert.equal(calls[0].path, "/api/capsulet/v1/ingestion/connectors");
    assert.equal(calls[1].path, "/api/capsulet/v1/ingestion/connectors");
    assert.equal(calls[1].init?.method, "POST");
    assert.deepEqual(JSON.parse(String(calls[1].init?.body)), {
      id: "connector_project_notes",
      name: "Project notes",
      kind: "local_text",
      enabled: true,
      config: {
        title: "Project Atlas Notes",
        content: "# Project Atlas",
        content_type: "text/markdown",
        uri: "local://project-atlas.md",
        authority: "high"
      }
    });
    assert.equal(calls[2].path, "/api/capsulet/v1/ingestion/connectors/connector_project_notes/runs");
    assert.equal(calls[2].init?.method, "POST");
    assert.equal(calls[3].path, "/api/capsulet/v1/ingestion/runs");
  });

  it("calls ingestion review queue endpoints", async () => {
    const calls: Array<{ path: string; init: RequestInit | undefined }> = [];
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async (input, init) => {
      calls.push({ path: String(input), init });
      return jsonResponse({
        claims: [
          {
            id: "claim_project_blocked",
            tenant_id: "tenant",
            project_id: "project",
            subject_id: "entity_project",
            predicate: "blocked_by",
            object: "Legal Review",
            evidence_ids: ["evidence_1"],
            confidence: 0.55,
            authority: "high",
            status: init?.method === "POST" ? "active" : "candidate",
            observed_at: "ingestion",
            valid_from: null,
            valid_until: null,
            evidence: [
              {
                id: "evidence_1",
                source_id: "source_1",
                locator: "chunk:1",
                excerpt: "Project Atlas is blocked by Legal Review",
                observed_at: "ingestion"
              }
            ],
            sources: [
              {
                id: "source_1",
                kind: "local_text",
                uri: "local://project-atlas.md",
                title: "Project Atlas Notes",
                authority: "high"
              }
            ]
          }
        ]
      });
    };

    try {
      const reviewClaims = await listReviewClaims("candidate");
      assert.equal(reviewClaims.claims[0].evidence[0].excerpt, "Project Atlas is blocked by Legal Review");
      assert.equal(reviewClaims.claims[0].sources[0].title, "Project Atlas Notes");
      await approveReviewClaim("claim_project_blocked");
      await rejectReviewClaim("claim_project_blocked");
    } finally {
      globalThis.fetch = originalFetch;
    }

    assert.equal(calls[0].path, "/api/capsulet/v1/ingestion/review/claims?status=candidate");
    assert.equal(calls[1].path, "/api/capsulet/v1/ingestion/review/claims/claim_project_blocked/approve");
    assert.equal(calls[1].init?.method, "POST");
    assert.equal(calls[2].path, "/api/capsulet/v1/ingestion/review/claims/claim_project_blocked/reject");
    assert.equal(calls[2].init?.method, "POST");
  });
});

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" }
  });
}
