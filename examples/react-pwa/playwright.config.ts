import { fileURLToPath } from "node:url";
import { defineConfig, devices } from "@playwright/test";
import type { HardlessOptions, HardlessTestOptions } from "@hardless/playwright";

/**
 * PRD M1, M2 and M3 acceptance: the example app prints through the Web Serial shim and the
 * HTTP printer endpoint, scans through the keyboard wedge, reads and writes NFC cards, and the
 * dashboard shows the results. M3 runs a production build of the app (no shim in it) through
 * `@hardless/playwright`, once with the devices in the page (standalone) and once against a
 * Hardless server. Needs the server binary with the dashboard built in:
 * `pnpm --filter @hardless/dashboard build && cargo build -p hardless-cli`, plus
 * `pnpm --filter @hardless/shim-web build` and `cargo xtask build-wasm`.
 *
 * Set `HARDLESS_SERVER` (e.g. `http://127.0.0.1:8080`) to run against a server that is
 * already running, such as the Docker image, instead of starting the binary.
 */
const exe = process.platform === "win32" ? ".exe" : "";
const hardless = fileURLToPath(new URL(`../../target/debug/hardless${exe}`, import.meta.url));
export const SERVER_PORT = 18282;
/** Where the production build of the app is served for M3. */
export const APP_PORT = 4181;
const external = process.env.HARDLESS_SERVER;
const server = external ?? `http://127.0.0.1:${String(SERVER_PORT)}`;
const bridge = `${server.replace(/^http/, "ws")}/bridge`;

/** The devices M3 uses; server mode creates any the server lacks. */
const M3_DEVICES = [
  { kind: "printer-escpos", id: "printer-1", transport: "serial", autoSelect: true },
  { kind: "cash-drawer", id: "drawer-1", attachedTo: "printer-1" },
  { kind: "scanner-wedge", id: "scanner-1" },
  { kind: "nfc-reader", id: "nfc-1" },
];
const standalone: HardlessOptions = { mode: "standalone", devices: M3_DEVICES };
const serverMode: HardlessOptions = { mode: "server", url: bridge, devices: M3_DEVICES };

export default defineConfig<HardlessTestOptions>({
  testDir: "e2e",
  forbidOnly: Boolean(process.env.CI),
  // One test at a time: the wedge types into every connected page, so parallel tests would
  // receive each other's scans.
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["github"]] : "list",
  use: { trace: "retain-on-failure" },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] }, testIgnore: "m3.spec.ts" },
    // Safari's engine, for the routes Safari has: HTTP printers and the keyboard wedge (M2).
    { name: "webkit", use: { ...devices["Desktop Safari"] }, testMatch: "m2.spec.ts" },
    {
      name: "m3-standalone",
      use: { ...devices["Desktop Chrome"], hardlessOptions: standalone },
      testMatch: "m3.spec.ts",
    },
    {
      name: "m3-server",
      use: { ...devices["Desktop Chrome"], hardlessOptions: serverMode },
      testMatch: "m3.spec.ts",
    },
  ],
  webServer: [
    ...(external
      ? []
      : [
          {
            command: `"${hardless}" start --port ${String(SERVER_PORT)}`,
            url: `http://127.0.0.1:${String(SERVER_PORT)}/api/v1/health`,
            reuseExistingServer: false,
            // No HTTPS in tests: it would use port 8443 and the user's certificate folder.
            env: { HARDLESS_HTTPS: "0" },
          },
        ]),
    {
      // M3: a production build, which contains no shim; the fixture injects it.
      command: `pnpm exec vite build --outDir dist-m3 && pnpm exec vite preview --outDir dist-m3 --host 127.0.0.1 --port ${String(APP_PORT)} --strictPort`,
      url: `http://127.0.0.1:${String(APP_PORT)}`,
      reuseExistingServer: false,
    },
    {
      command: "pnpm exec vite --port 5173 --strictPort",
      url: "http://localhost:5173",
      reuseExistingServer: !process.env.CI,
      env: { VITE_HARDLESS_URL: `ws://127.0.0.1:${String(SERVER_PORT)}/bridge` },
    },
  ],
});
