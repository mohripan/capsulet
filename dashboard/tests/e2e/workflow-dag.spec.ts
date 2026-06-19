import { expect, test } from "@playwright/test";

test("creates and reloads a fan-in workflow DAG", async ({ page }) => {
  const workflowName = `Playwright DAG ${Date.now()}`;
  await page.goto("/workflows");
  await expect(page.getByRole("heading", { name: "Build dependency graphs" })).toBeVisible();

  await page.getByLabel("Workflow name").fill(workflowName);
  const mergeNode = page.locator("fieldset").nth(2);
  await expect(mergeNode.getByRole("checkbox")).toHaveCount(2);
  await expect(mergeNode.getByRole("checkbox").nth(0)).toBeChecked();
  await expect(mergeNode.getByRole("checkbox").nth(1)).toBeChecked();

  const created = page.waitForResponse((response) =>
    response.url().endsWith("/v1/workflows") && response.request().method() === "POST"
  );
  await page.getByRole("button", { name: "Create workflow" }).click();
  expect((await created).status()).toBe(201);

  await expect(page.getByRole("heading", { name: workflowName })).toBeVisible();
  await expect(page.getByText("3 nodes · 2 edges")).toBeVisible();
  await expect(page.getByText("2 prerequisites · 0 downstream")).toBeVisible();

  await page.reload();
  await expect(page.getByRole("heading", { name: workflowName })).toBeVisible();
  await expect(page.getByText("3 nodes · 2 edges")).toBeVisible();
});
