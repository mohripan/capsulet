import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  CapsuletApiError,
  activateMemorySubgraph,
  capsuletStreamUrl,
  createCanonicalEntity,
  createMemorySubgraph,
  createSubgraphEdge,
  formatBytes,
  getErrorMessage,
  isTerminalStatus,
  listCanonicalEntities,
  listMemorySubgraphs
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
    assert.equal(calls[2].path, "/api/capsulet/v1/memory/subgraph-edges");
    assert.equal(calls[2].init?.method, "POST");
    assert.deepEqual(JSON.parse(String(calls[2].init?.body)), {
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
});

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" }
  });
}
