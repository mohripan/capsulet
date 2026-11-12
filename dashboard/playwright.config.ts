import { defineConfig, devices } from "@playwright/test";
import { fixture, storageState } from "./tests/e2e/fixture";

// Runs against a seeded stack: start one with `cargo run -p capsulet-xtask -- stack up`,
// or let the verifier's `dashboard-e2e` gate start one of its own.
export default defineConfig({
  testDir: "./tests/e2e",
  timeout: 60_000,
  expect: { timeout: 15_000 },
  // The specs share one seeded stack, and some of them change what others read.
  workers: 1,
  globalTeardown: "./tests/e2e/global-teardown.ts",
  use: {
    baseURL: process.env.PLAYWRIGHT_BASE_URL ?? fixture.dashboard_url,
    trace: "retain-on-failure",
    screenshot: "only-on-failure"
  },
  projects: [
    { name: "setup", testMatch: /.*\.setup\.ts/ },
    {
      name: "chromium",
      dependencies: ["setup"],
      testIgnore: /.*\.setup\.ts/,
      use: { ...devices["Desktop Chrome"], storageState: storageState("admin") }
    }
  ]
});
