import { fileURLToPath } from "node:url";
import { defineConfig } from "@playwright/test";

/**
 * FR-17 / M3 acceptance: the extension lets an unmodified (production) build of the example
 * app print through the shim. Needs `pnpm --filter @hardless/extension build`, the server
 * binary (`cargo build -p hardless-cli`) and Playwright's Chromium.
 */
const exe = process.platform === "win32" ? ".exe" : "";
const hardless = fileURLToPath(new URL(`../../target/debug/hardless${exe}`, import.meta.url));
export const SERVER_PORT = 18383;
export const APP_PORT = 4180;

export default defineConfig({
  testDir: "e2e",
  forbidOnly: Boolean(process.env.CI),
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["github"]] : "list",
  use: { trace: "retain-on-failure" },
  webServer: [
    {
      command: `"${hardless}" start --port ${String(SERVER_PORT)}`,
      url: `http://127.0.0.1:${String(SERVER_PORT)}/api/v1/health`,
      reuseExistingServer: false,
      // No HTTPS in tests: it would use port 8443 and the user's certificate folder.
      env: { HARDLESS_HTTPS: "0" },
    },
    {
      // A production build: no shim in the app, only what the extension injects.
      command: `pnpm --filter @hardless/example-react-pwa exec vite build --outDir dist-extension-test && pnpm --filter @hardless/example-react-pwa exec vite preview --outDir dist-extension-test --host 127.0.0.1 --port ${String(APP_PORT)} --strictPort`,
      url: `http://127.0.0.1:${String(APP_PORT)}`,
      reuseExistingServer: false,
    },
  ],
});
