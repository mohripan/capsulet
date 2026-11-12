import { expect, test, type APIRequestContext } from "@playwright/test";
import { alpha, asRole, expectStatus } from "./fixture";

type Workflow = { id: string; name: string; steps: { job_definition_id: string }[] };

/** Deletes the named workflows in alpha and the job definitions their steps ran. */
async function removeWorkflowsNamed(api: APIRequestContext, names: string[]) {
  const listed = await expectStatus(await api.get("/v1/workflows"), 200);
  for (const workflow of (listed.workflows as Workflow[]).filter((candidate) => names.includes(candidate.name))) {
    await expectStatus(await api.delete(`/v1/workflows/${workflow.id}`), 204);
    for (const step of workflow.steps) {
      await expectStatus(await api.delete(`/v1/job-definitions/${step.job_definition_id}`), 204);
    }
  }
}

test("creates and reloads a two-cell Python notebook workflow", async ({ page }) => {
  const workflowName = `${alpha.name} notebook workflow`;
  const updatedName = `${workflowName} updated`;
  // A run that failed before cleaning up leaves its workflow behind.
  await asRole("admin", alpha, (api) => removeWorkflowsNamed(api, [workflowName, updatedName]));

  try {
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
    await page.getByLabel("Workflow name").fill(updatedName);
    const updated = page.waitForResponse((response) =>
      response.url().includes("/v1/workflows/") && response.request().method() === "PUT"
    );
    await page.getByRole("button", { name: "Save changes" }).click();
    expect((await updated).status()).toBe(200);
    await expect(page.getByRole("status")).toContainText(`Updated ${updatedName}`);
    await page.getByRole("link", { name: "View workflow" }).click();
    await expect(page.getByRole("heading", { name: updatedName })).toBeVisible();
  } finally {
    await asRole("admin", alpha, (api) => removeWorkflowsNamed(api, [workflowName, updatedName]));
  }
});

test("locks notebooks used by queued workflow runs", async ({ page }) => {
  const jobId = "e2e_alpha_lock_job";
  const workflowId = "e2e_alpha_lock_workflow";
  const automationName = `${alpha.name} lock automation`;

  await asRole("admin", alpha, async (api) => {
    if ((await api.get(`/v1/job-definitions/${jobId}`)).status() === 404) {
      await expectStatus(
        await api.post("/v1/job-definitions", {
          data: { id: jobId, name: "Slow lock job", python_script: "import time\ntime.sleep(30)" }
        }),
        201
      );
    }
    if ((await api.get(`/v1/workflows/${workflowId}`)).status() === 404) {
      await expectStatus(
        await api.post("/v1/workflows", {
          data: {
            id: workflowId,
            name: `${alpha.name} locked notebook`,
            steps: [{ id: `${workflowId}-step`, name: "Slow step", job_definition_id: jobId, execution_pool: "mini" }]
          }
        }),
        201
      );
    }
    const automations = await expectStatus(await api.get("/v1/automations"), 200);
    const existing = (automations.automations as { id: string; name: string }[]).find(
      (automation) => automation.name === automationName
    );
    const automationId =
      existing?.id ??
      (await expectStatus(await api.post("/v1/automations", { data: { name: automationName, workflow_id: workflowId } }), 201)).id;
    await expectStatus(await api.post(`/v1/automations/${automationId}/trigger`), 201);
  });

  await page.goto(`/workflows/new?workflow=${workflowId}`);
  await expect(page.getByText("Notebook locked")).toBeVisible();
  await expect(page.getByLabel("Workflow name")).toBeDisabled();
  await expect(page.getByRole("button", { name: "Save changes" })).toBeDisabled();
});

test("redirects the root dashboard to Memory Studio", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveURL(/\/memory$/);
  await expect(page.getByRole("heading", { name: "Graph Workbench" })).toBeVisible();
});
