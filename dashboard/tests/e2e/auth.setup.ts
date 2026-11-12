import { expect, test as setup } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { AUTH_DIRECTORY, PROJECT_STORAGE_KEY, alpha, fixture, storageState } from "./fixture";

setup("the administrator signs in through the login form", async ({ page }) => {
  const login = fixture.dashboard_login;
  await page.goto("/");
  await expect(page).toHaveURL(/\/login(?:\?.*)?$/);
  await expect(page.getByRole("heading", { name: "Sign in to Capsulet" })).toBeVisible();

  await page.getByLabel("Username").fill(login.username);
  await page.getByLabel("Password").fill(`${login.password}-but-wrong`);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByText("The username or password is incorrect.")).toBeVisible();

  await page.getByLabel("Password").fill(login.password);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL(/\/memory$/);
  await page.evaluate(([key, project]) => window.localStorage.setItem(key, project), [PROJECT_STORAGE_KEY, alpha.id]);
  await mkdir(AUTH_DIRECTORY, { recursive: true });
  await page.context().storageState({ path: storageState("admin") });
});

// The dashboard's session is the API token it was given, so a role without a
// dashboard login gets its session the same way a signed-in user's is kept.
for (const role of ["operator", "viewer"] as const) {
  setup(`the ${role} has a session in ${alpha.name}`, async ({ browser, baseURL }) => {
    const identity = fixture.identities[role];
    const context = await browser.newContext();
    await context.addCookies([
      { name: "capsulet_session", value: identity.token, url: baseURL!, httpOnly: true, sameSite: "Strict" }
    ]);
    const page = await context.newPage();
    await page.goto("/security");
    await page.evaluate(([key, project]) => window.localStorage.setItem(key, project), [PROJECT_STORAGE_KEY, alpha.id]);
    await page.reload();
    await expect(page.getByText(identity.principal, { exact: true }).first()).toBeVisible();
    await mkdir(AUTH_DIRECTORY, { recursive: true });
    await context.storageState({ path: storageState(role) });
    await context.close();
  });
}
