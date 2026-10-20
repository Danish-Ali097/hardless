import { fileURLToPath } from "node:url";
import { defineConfig, devices } from "@playwright/test";

/**
 * PRD M1 acceptance: the example app prints through the Web Serial shim to the Hardless
 * server, and the dashboard shows the receipt and the drawer. Needs the server binary with the
 * dashboard built in: `pnpm --filter @hardless/dashboard build && cargo build -p hardless-cli`.
 */
const exe = process.platform === "win32" ? ".exe" : "";
const hardless = fileURLToPath(new URL(`../../target/debug/hardless${exe}`, import.meta.url));
export const SERVER_PORT = 18282;

export default defineConfig({
  testDir: "e2e",
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["github"]] : "list",
  use: { trace: "retain-on-failure" },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: [
    {
      command: `"${hardless}" start --port ${String(SERVER_PORT)}`,
      url: `http://127.0.0.1:${String(SERVER_PORT)}/api/v1/health`,
      reuseExistingServer: false,
      // No HTTPS in tests: it would use port 8443 and the user's certificate folder.
      env: { HARDLESS_HTTPS: "0" },
    },
    {
      command: "pnpm exec vite --port 5173 --strictPort",
      url: "http://localhost:5173",
      reuseExistingServer: !process.env.CI,
      env: { VITE_HARDLESS_URL: `ws://127.0.0.1:${String(SERVER_PORT)}/bridge` },
    },
  ],
});
