import { expect, test as setup } from "@playwright/test";
import { mkdir } from "node:fs/promises";

const authFile = "playwright/.auth/admin.json";

setup("authenticate as an administrator", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveURL(/\/login(?:\?.*)?$/);
  await expect(page.getByRole("heading", { name: "Sign in to Capsulet" })).toBeVisible();

  await page.getByLabel("Username").fill("admin");
  await page.getByLabel("Password").fill("wrong-password");
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByText("The username or password is incorrect.")).toBeVisible();

  await page.getByLabel("Username").fill(process.env.CAPSULET_E2E_USERNAME ?? "admin");
  await page.getByLabel("Password").fill(process.env.CAPSULET_E2E_PASSWORD ?? "admin");
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL(/\/$/);
  await mkdir("playwright/.auth", { recursive: true });
  await page.context().storageState({ path: authFile });
});
