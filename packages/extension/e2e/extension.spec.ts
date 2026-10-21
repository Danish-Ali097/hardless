import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium, expect, test, type BrowserContext } from "@playwright/test";
import { APP_PORT, SERVER_PORT } from "../playwright.config.js";

/**
 * FR-17 and NFR-03 in a real Chromium with the extension loaded: off by default, then turned
 * on for one origin from the popup, after which a production build of the example app (which
 * contains no shim) prints to the emulated printer.
 */

const APP = `http://127.0.0.1:${String(APP_PORT)}`;
const SERVER = `http://127.0.0.1:${String(SERVER_PORT)}`;
const dist = fileURLToPath(new URL("../dist", import.meta.url));

let dir = "";
let profile = "";
let context: BrowserContext;
let extensionId = "";

test.beforeAll(async () => {
  // A copy of the built extension with two test-only changes: access to 127.0.0.1 granted up
  // front (Playwright cannot click Chrome's permission prompt) and an empty service worker,
  // whose URL tells the test the extension's ID.
  dir = mkdtempSync(join(tmpdir(), "hardless-ext-"));
  cpSync(dist, dir, { recursive: true });
  const manifest = JSON.parse(readFileSync(join(dir, "manifest.json"), "utf8")) as Record<
    string,
    unknown
  >;
  manifest.host_permissions = ["http://127.0.0.1/*"];
  manifest.background = { service_worker: "test-worker.js" };
  writeFileSync(join(dir, "manifest.json"), JSON.stringify(manifest));
  writeFileSync(join(dir, "test-worker.js"), "");
  profile = mkdtempSync(join(tmpdir(), "hardless-profile-"));
  context = await chromium.launchPersistentContext(profile, {
    channel: "chromium",
    args: [`--disable-extensions-except=${dir}`, `--load-extension=${dir}`],
  });
  const worker = context.serviceWorkers()[0] ?? (await context.waitForEvent("serviceworker"));
  extensionId = new URL(worker.url()).host;
});

test.afterAll(async () => {
  await context.close();
  rmSync(dir, { recursive: true, force: true });
  rmSync(profile, { recursive: true, force: true });
});

test("NFR-03: the extension does nothing on a site until it is turned on", async () => {
  const app = await context.newPage();
  await app.goto(`${APP}/`);
  expect(await app.evaluate(() => "NDEFReader" in globalThis)).toBe(false);
  await app.close();
});

test("FR-17: turned on from the popup, an unmodified app prints through the shim", async () => {
  await context.request.post(`${SERVER}/api/v1/devices/printer-1/reset`);
  const app = await context.newPage();
  await app.goto(`${APP}/`);

  const popup = await context.newPage();
  await popup.goto(
    `chrome-extension://${extensionId}/popup.html?origin=${encodeURIComponent(APP)}`,
  );
  await expect(popup.getByRole("status")).toHaveText(/^Off/);
  await popup
    .getByLabel("Hardless server bridge")
    .fill(`ws://127.0.0.1:${String(SERVER_PORT)}/bridge`);
  const reloaded = app.waitForEvent("load");
  await popup.getByRole("button", { name: "Use Hardless on this site" }).click();
  await expect(popup.getByRole("status")).toHaveText(/^On/);
  await reloaded;

  // The shim was installed before the app's code ran.
  expect(await app.evaluate(() => "NDEFReader" in globalThis)).toBe(true);
  await app.getByRole("button", { name: "Connect printer" }).click();
  await app.locator('hardless-port-picker [data-device-id="printer-1"]').click();
  await app.locator('hardless-port-picker [data-action="connect"]').click();
  await expect(app.getByText("Printer connected", { exact: true })).toBeVisible();
  await app.getByRole("button", { name: "Pay cash" }).click();
  await expect(app.getByText("Receipt printed. Drawer opened.")).toBeVisible();
  const job = (await (
    await context.request.get(`${SERVER}/api/v1/devices/printer-1/jobs/last`)
  ).json()) as {
    text: string;
  };
  expect(job.text).toContain("CORNER CAFE");

  // Turning it off restores the browser's own APIs after the reload.
  const again = app.waitForEvent("load");
  await popup.getByRole("button", { name: "Turn off" }).click();
  await expect(popup.getByRole("status")).toHaveText(/^Off/);
  await again;
  expect(await app.evaluate(() => "NDEFReader" in globalThis)).toBe(false);
});
