import { expect, test } from "@playwright/test";
import { fixture, storageState } from "./fixture";

test("shows the authenticated identity and durable audit trail", async ({ page }) => {
  const admin = fixture.identities.admin;
  await page.goto("/security");
  await expect(page.getByRole("heading", { name: "Review sandbox and access controls" })).toBeVisible();
  await expect(page.getByText("Authenticated Session")).toBeVisible();
  await expect(page.getByText(admin.principal, { exact: true }).first()).toBeVisible();
  await expect(page.getByText(admin.role, { exact: true }).first()).toBeVisible();
  await expect(page.getByText("Recent Audit Events")).toBeVisible();
  // The seed's writes to the selected project are in its audit trail.
  await expect(page.getByText(/POST \/v1\//).first()).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
});

test.describe("as a viewer", () => {
  test.use({ storageState: storageState("viewer") });

  test("the session names the viewer and its role", async ({ page }) => {
    const viewer = fixture.identities.viewer;
    await page.goto("/security");
    await expect(page.getByText("Authenticated Session")).toBeVisible();
    await expect(page.getByText(viewer.principal, { exact: true }).first()).toBeVisible();
    await expect(page.getByText(viewer.role, { exact: true }).first()).toBeVisible();
  });
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
