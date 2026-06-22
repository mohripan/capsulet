import { expect, test as setup } from "@playwright/test";
import { mkdir } from "node:fs/promises";

const authFile = "playwright/.auth/admin.json";

setup("authenticate as an administrator", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveURL(/\/login(?:\?.*)?$/);
  await expect(page.getByRole("heading", { name: "Connect with an access token" })).toBeVisible();

  const tokenInput = page.getByRole("textbox", { name: "Access token" });
  await tokenInput.fill("invalid-token");
  await page.getByRole("button", { name: "Connect" }).click();
  await expect(page.getByText("The access token is invalid or expired.")).toBeVisible();

  await tokenInput.fill(process.env.CAPSULET_E2E_TOKEN ?? "capsulet-local-admin-token-change-me");
  await page.getByRole("button", { name: "Connect" }).click();
  await expect(page).toHaveURL(/\/$/);
  await mkdir("playwright/.auth", { recursive: true });
  await page.context().storageState({ path: authFile });
});
