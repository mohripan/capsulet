import { expect, test } from "@playwright/test";
import { alpha, asRole, beta, expectStatus, fixture } from "./fixture";

test("renders the memory studio workbench with the selected project's memory only", async ({ page }) => {
  const own = fixture.resources.alpha;
  const other = fixture.resources.beta;
  await page.goto("/memory");

  await expect(page.getByRole("heading", { name: "Graph Workbench" })).toBeVisible();
  await expect(page.getByText("Nested Memory Graph")).toBeVisible();
  await expect(page.getByText("Selected Subgraph")).toBeVisible();
  await expect(page.getByText("Claim Review Inbox")).toBeVisible();
  await expect(page.getByText(own.memory_subgraph.name).first()).toBeVisible();
  await expect(page.getByText(own.canonical_entity.name).first()).toBeVisible();
  await expect(page.getByText(other.memory_subgraph.name)).toHaveCount(0);
  await expect(page.getByText(other.canonical_entity.name)).toHaveCount(0);
  await expect(page.getByLabel("Primary").getByRole("link", { name: "Subgraphs" })).toBeVisible();
  await expect(page.getByLabel("Primary").getByRole("link", { name: "Entities" })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
});

test("reviews entity resolution candidates from the dashboard", async ({ page }) => {
  const entityName = `${alpha.name} resolution project`;
  const canonicalId = "e2e_alpha_resolution_project";
  const connectorId = "e2e_alpha_resolution_connector";

  await asRole("admin", alpha, async (api) => {
    const entities = await expectStatus(await api.get("/v1/memory/canonical-entities"), 200);
    if (!JSON.stringify(entities).includes(`"${canonicalId}"`)) {
      await expectStatus(
        await api.post("/v1/memory/canonical-entities", {
          data: { id: canonicalId, entity_type: "Project", display_name: entityName, aliases: [] }
        }),
        201
      );
    }
    if ((await api.get(`/v1/ingestion/connectors/${connectorId}`)).status() === 404) {
      await expectStatus(
        await api.post("/v1/ingestion/connectors", {
          data: {
            id: connectorId,
            name: `${alpha.name} resolution connector`,
            kind: "local_text",
            enabled: true,
            config: {
              title: `${alpha.name} resolution notes`,
              content: `# ${entityName}\n\n- ${entityName} is governed by Capsulet\n`,
              content_type: "text/markdown",
              authority: "high"
            }
          }
        }),
        201
      );
    }
    await expectStatus(await api.post(`/v1/ingestion/connectors/${connectorId}/runs`), 201);
  });

  await page.goto("/memory/entities");
  const queue = page.getByRole("heading", { name: "Resolution Queue" }).locator("xpath=ancestor::section[1]");
  const candidate = queue.locator("article").filter({ hasText: entityName });
  await expect(candidate.first()).toBeVisible();
  await candidate.first().getByRole("button", { name: "Confirm" }).click();
  await expect(candidate).toHaveCount(0);

  // What was reviewed in one project is invisible from the other.
  await asRole("admin", beta, async (api) => {
    const entities = await expectStatus(await api.get("/v1/memory/canonical-entities"), 200);
    expect(JSON.stringify(entities)).not.toContain(canonicalId);
  });
});
