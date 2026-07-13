import { expect, test } from "@playwright/test";

test("renders the memory studio workbench", async ({ page }) => {
  const suffix = Date.now();
  const api = process.env.CAPSULET_E2E_API_URL ?? "http://127.0.0.1:8080";
  const headers = { Authorization: `Bearer ${process.env.CAPSULET_E2E_TOKEN ?? "capsulet-local-admin-token-change-me"}` };
  const rootId = `ui_memory_root_${suffix}`;
  const subgraphName = `UI Memory Graph ${suffix}`;
  const entityName = `UI Customer ${suffix}`;

  const root = await page.request.post(`${api}/v1/memory/subgraphs`, {
    headers,
    data: { id: rootId, name: `UI Root ${suffix}`, description: "Created by memory studio e2e" }
  });
  expect(root.status()).toBe(201);
  const child = await page.request.post(`${api}/v1/memory/subgraphs`, {
    headers,
    data: { id: `ui_memory_child_${suffix}`, parent_subgraph_id: rootId, name: subgraphName, description: "Nested memory e2e graph" }
  });
  expect(child.status()).toBe(201);
  const entity = await page.request.post(`${api}/v1/memory/canonical-entities`, {
    headers,
    data: { id: `ui_customer_${suffix}`, entity_type: "Customer", display_name: entityName, aliases: ["customer-a", "ACME"] }
  });
  expect(entity.status()).toBe(201);

  await page.goto("/memory");

  await expect(page.getByRole("heading", { name: "Graph Workbench" })).toBeVisible();
  await expect(page.getByText("Nested Memory Graph")).toBeVisible();
  await expect(page.getByText("Selected Subgraph")).toBeVisible();
  await expect(page.getByText("Claim Review Inbox")).toBeVisible();
  await expect(page.getByText(subgraphName).first()).toBeVisible();
  await expect(page.getByText(entityName).first()).toBeVisible();
  await expect(page.getByLabel("Primary").getByRole("link", { name: "Subgraphs" })).toBeVisible();
  await expect(page.getByLabel("Primary").getByRole("link", { name: "Entities" })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
});

test("reviews entity resolution candidates from the dashboard", async ({ page }) => {
  const suffix = Date.now();
  const api = process.env.CAPSULET_E2E_API_URL ?? "http://127.0.0.1:8080";
  const headers = { Authorization: `Bearer ${process.env.CAPSULET_E2E_TOKEN ?? "capsulet-local-admin-token-change-me"}` };
  const entityName = `UI Resolution Project ${suffix}`;
  const canonicalId = `ui_resolution_canonical_${suffix}`;
  const connectorId = `ui_resolution_connector_${suffix}`;

  const canonical = await page.request.post(`${api}/v1/memory/canonical-entities`, {
    headers,
    data: { id: canonicalId, entity_type: "Project", display_name: entityName, aliases: [] }
  });
  expect(canonical.status()).toBe(201);
  const connector = await page.request.post(`${api}/v1/ingestion/connectors`, {
    headers,
    data: {
      id: connectorId,
      name: `UI resolution connector ${suffix}`,
      kind: "local_text",
      enabled: true,
      config: {
        title: `UI resolution notes ${suffix}`,
        content: `# ${entityName}\n\n- ${entityName} is governed by Capsulet\n`,
        content_type: "text/markdown",
        authority: "high"
      }
    }
  });
  expect(connector.status()).toBe(201);
  const run = await page.request.post(`${api}/v1/ingestion/connectors/${connectorId}/runs`, { headers });
  expect(run.status()).toBe(201);

  await page.goto("/memory/entities");

  const queue = page.getByRole("heading", { name: "Resolution Queue" }).locator("xpath=ancestor::section[1]");
  const candidate = queue.locator("article").filter({ hasText: entityName });
  await expect(candidate).toBeVisible();
  await candidate.getByRole("button", { name: "Confirm" }).click();
  await expect(candidate).toHaveCount(0);
});
