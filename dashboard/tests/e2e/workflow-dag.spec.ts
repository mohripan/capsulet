import { expect, test } from "@playwright/test";

test("creates and reloads a two-cell Python notebook workflow", async ({ page }) => {
  const workflowName = `Playwright notebook ${Date.now()}`;
  await page.goto("/workflows/new");
  await expect(page.getByRole("heading", { name: "Create workflow" })).toBeVisible();

  await page.getByLabel("Workflow name").fill(workflowName);
  const cells = page.locator(".notebookCell");
  await expect(cells).toHaveCount(2);
  await expect(cells.nth(1).getByRole("checkbox")).toBeChecked();

  let createdJobs = 0;
  page.on("response", (response) => {
    if (response.url().endsWith("/v1/job-definitions") && response.request().method() === "POST" && response.status() === 201) createdJobs += 1;
  });
  const created = page.waitForResponse((response) =>
    response.url().endsWith("/v1/workflows") && response.request().method() === "POST"
  );
  await page.getByRole("button", { name: "Create workflow" }).click();
  expect((await created).status()).toBe(201);
  expect(createdJobs).toBe(2);

  await expect(page.getByRole("status")).toContainText(`Created ${workflowName}`);
  await page.getByRole("link", { name: "View workflow" }).click();
  await expect(page.getByRole("heading", { name: workflowName })).toBeVisible();
  await expect(page.getByText("2 cells · 1 edge").first()).toBeVisible();
  const catalogItems = page.locator(".workflowCatalogItem");
  await expect(catalogItems.filter({ hasText: workflowName })).toHaveCount(1);
  await expect
    .poll(() => catalogItems.count(), { message: "catalog page should render at most one page of workflows" })
    .toBeLessThanOrEqual(8);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);

  await page.reload();
  await expect(page.getByRole("heading", { name: workflowName })).toBeVisible();
  await expect(page.getByText("2 cells · 1 edge").first()).toBeVisible();

  await page.getByRole("link", { name: "Edit notebook" }).click();
  await expect(page.getByRole("heading", { name: "Edit workflow" })).toBeVisible();
  await expect(page.locator(".notebookCell")).toHaveCount(2);
  await expect(page.locator(".notebookCell").first().locator("textarea").first()).toHaveValue(/customers\.csv/);
  const updatedName = `${workflowName} updated`;
  await page.getByLabel("Workflow name").fill(updatedName);
  const updated = page.waitForResponse((response) =>
    response.url().includes("/v1/workflows/") && response.request().method() === "PUT"
  );
  await page.getByRole("button", { name: "Save changes" }).click();
  expect((await updated).status()).toBe(200);
  await expect(page.getByRole("status")).toContainText(`Updated ${updatedName}`);
  await page.getByRole("link", { name: "View workflow" }).click();
  await expect(page.getByRole("heading", { name: updatedName })).toBeVisible();
});

test("locks notebooks used by queued workflow runs", async ({ page }) => {
  const suffix = Date.now();
  const jobId = `job-lock-e2e-${suffix}`;
  const workflowId = `workflow-lock-e2e-${suffix}`;
  const api = "http://127.0.0.1:8080/v1";
  const headers = { Authorization: `Bearer ${process.env.CAPSULET_E2E_TOKEN ?? "capsulet-local-admin-token-change-me"}` };
  const job = await page.request.post(`${api}/job-definitions`, { headers, data: { id: jobId, name: "Slow lock job", python_script: "import time\ntime.sleep(30)" } });
  expect(job.status()).toBe(201);
  const workflow = await page.request.post(`${api}/workflows`, { headers, data: { id: workflowId, name: "Locked notebook E2E", steps: [{ id: `${workflowId}-step`, name: "Slow step", job_definition_id: jobId, execution_pool: "mini" }] } });
  expect(workflow.status()).toBe(201);
  const automation = await page.request.post(`${api}/automations`, { headers, data: { name: `Lock automation ${suffix}`, workflow_id: workflowId } });
  expect(automation.status()).toBe(201);
  const automationBody = await automation.json();
  const triggered = await page.request.post(`${api}/automations/${automationBody.id}/trigger`, { headers });
  expect(triggered.status()).toBe(201);

  await page.goto(`/workflows/new?workflow=${workflowId}`);
  await expect(page.getByText("Notebook locked")).toBeVisible();
  await expect(page.getByLabel("Workflow name")).toBeDisabled();
  await expect(page.getByRole("button", { name: "Save changes" })).toBeDisabled();
});

test("renders overview topology from the live endpoint", async ({ page }) => {
  const topologyResponse = page.waitForResponse((response) => response.url().endsWith("/v1/topology"));
  await page.goto("/");
  expect((await topologyResponse).status()).toBe(200);
  await expect(page.getByText("No topology endpoint exists for the overview.")).toHaveCount(0);
  await expect(page.locator(".overviewTopologyStage").first()).toBeVisible();
});
