import { expect, test } from "@playwright/test";

test("shows the authenticated identity and durable audit trail", async ({ page }) => {
  await page.goto("/security");
  await expect(page.getByRole("heading", { name: "Review sandbox and access controls" })).toBeVisible();
  await expect(page.getByText("Authenticated Session")).toBeVisible();
  await expect(page.getByText("local-admin", { exact: true })).toBeVisible();
  await expect(page.getByText("admin", { exact: true }).first()).toBeVisible();
  await expect(page.getByText("Recent Audit Events")).toBeVisible();
  await expect(page.getByText(/POST \/v1\//).first()).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
});

test("renders production trigger contracts in the automation wizard", async ({ page }) => {
  await page.goto("/automations");
  await page.getByRole("button", { name: "Automation", exact: true }).first().click();
  const dialog = page.getByRole("dialog", { name: "Create automation" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "triggers" }).click();

  const kind = dialog.getByText("Kind").locator("..").locator("select");
  await kind.selectOption("schedule");
  await expect(dialog.getByText("Cron expression")).toBeVisible();
  await expect(dialog.getByText("Timezone")).toBeVisible();

  await kind.selectOption("sql");
  await expect(dialog.getByText(/query runs read-only with a five-second timeout/i)).toBeVisible();

  await kind.selectOption("webhook");
  await expect(dialog.getByText(/Send signed JSON to \/v1\/webhooks/i)).toBeVisible();

  await kind.selectOption("custom");
  await expect(dialog.getByText(/isolated plugin must print a final JSON line/i)).toBeVisible();
  expect(await dialog.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true);
});
