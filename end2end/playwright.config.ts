import { defineConfig, devices } from "@playwright/test";

/**
 * The server is started by `cargo leptos end-to-end` (or, in CI, by the workflow).
 * BASE_URL must have the same origin as the server's APP_URL, otherwise the CSRF
 * Origin check rejects every POST. Default: the dev APP_URL from .env.example.
 */
const baseURL = process.env.BASE_URL ?? "http://localhost:3000";

const workers = process.env.PLAYWRIGHT_WORKERS === undefined
  ? undefined
  : Number(process.env.PLAYWRIGHT_WORKERS);
if (workers !== undefined && (!Number.isInteger(workers) || workers < 1)) {
  throw new Error("PLAYWRIGHT_WORKERS must be a positive integer");
}

export default defineConfig({
  workers,
  testDir: "./tests",
  globalSetup: "./tests/support/global-setup.ts",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  timeout: 30_000,
  expect: { timeout: 10_000 },
  use: {
    baseURL,
    locale: "en-US",
    timezoneId: "Europe/Berlin",
    trace: "retain-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
