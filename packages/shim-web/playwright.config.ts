import { defineConfig, devices } from "@playwright/test";

/**
 * Shim conformance tests in a real Chromium (PRD section 15): every row of section 8.1.
 * Run `pnpm --filter @hardless/shim-web build` and `cargo xtask build-wasm` first.
 */
export default defineConfig({
  testDir: "e2e",
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["github"]] : "list",
  use: { baseURL: "http://127.0.0.1:4174", trace: "retain-on-failure" },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: "node e2e/serve.mjs",
    url: "http://127.0.0.1:4174/e2e/page.html",
    reuseExistingServer: !process.env.CI,
  },
});
