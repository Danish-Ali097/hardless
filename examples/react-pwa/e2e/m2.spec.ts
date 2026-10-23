import { expect, test, type Page } from "@playwright/test";
import { SERVER_PORT } from "../playwright.config.js";

/**
 * PRD M2 acceptance, automated parts (the iOS Safari check is manual):
 * - The example app prints through the HTTP printer endpoint, in Chromium and in WebKit (the
 *   engine of Safari).
 * - A scripted 20-barcode sequence arrives in order with the configured delay.
 */

const SERVER = `http://127.0.0.1:${String(SERVER_PORT)}`;
const EPOS = `${SERVER}/cgi-bin/epos/service.cgi?devid=local_printer&timeout=10000`;

test.describe.configure({ mode: "serial" });

test.beforeEach(async ({ request }) => {
  for (const id of ["local_printer", "scanner-1"]) {
    await request.post(`${SERVER}/api/v1/devices/${id}/reset`);
  }
});

/** Opens the app with the network printer pointed at the test server. */
async function openWithNetworkPrinter(page: Page): Promise<void> {
  await page.goto("http://localhost:5173/");
  await page.getByRole("radio", { name: "Network (ePOS-Print)" }).check();
  await page.getByLabel("Printer address").fill(EPOS);
}

test("M2: the app prints through the ePOS-Print-compatible endpoint", async ({ page, request }) => {
  await openWithNetworkPrinter(page);
  await page.getByRole("button", { name: "Pay card" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Receipt printed." })).toBeVisible();

  const response = await request.get(`${SERVER}/api/v1/devices/local_printer/jobs/last`);
  const job = (await response.json()) as { text: string };
  expect(job.text).toContain("CORNER CAFE");
  expect(job.text).toMatch(/Total:\s+1,250/);
});

test("M2: paper-out on the HTTP printer fails the app's print with a reason", async ({
  page,
  request,
}) => {
  await request.put(`${SERVER}/api/v1/devices/local_printer/faults/paper-out`, {
    data: { active: true },
  });
  await openWithNetworkPrinter(page);
  await page.getByRole("button", { name: "Pay cash" }).click();
  await expect(page.getByRole("alert")).toHaveText(
    "The receipt did not print: the printer is out of paper.",
  );
});

test("M2: a barcode scanned on the server is typed into the app", async ({ page, request }) => {
  await page.goto("http://localhost:5173/");
  await page.getByLabel("Scan or type a barcode").focus();
  // The shim connects to the bridge; scan once it is up.
  await expect(async () => {
    await request.post(`${SERVER}/api/v1/devices/scanner-1/scan`, {
      data: { code: "5012345678900" },
    });
    await expect(page.getByText("1 × Orange juice")).toBeVisible({ timeout: 500 });
  }).toPass({ timeout: 10_000 });
  await expect(page.getByRole("status").filter({ hasText: "Added Orange juice." })).toBeVisible();
});

test("M2: a scripted 20-barcode sequence arrives in order with the configured delay", async ({
  page,
  request,
  browserName,
}) => {
  test.skip(browserName !== "chromium", "timing is measured once, in Chromium");
  const DELAY_MS = 100;
  await page.goto("http://localhost:5173/");
  const field = page.getByLabel("Scan or type a barcode");
  await field.focus();
  // Record each scan when its Enter arrives, before the app clears the field.
  await page.evaluate(() => {
    const scans: { code: string; at: number }[] = [];
    (window as unknown as { scans: typeof scans }).scans = scans;
    document.addEventListener(
      "keydown",
      (event) => {
        const input = event.target as HTMLInputElement;
        if (event.key === "Enter") scans.push({ code: input.value, at: performance.now() });
      },
      true,
    );
  });
  // Wait until the shim's bridge is connected: a first scan gets through.
  await expect(async () => {
    await request.post(`${SERVER}/api/v1/devices/scanner-1/scan`, { data: { code: "READY" } });
    await expect
      .poll(() => page.evaluate(() => (window as unknown as { scans: unknown[] }).scans.length), {
        timeout: 500,
      })
      .toBeGreaterThan(0);
  }).toPass({ timeout: 10_000 });
  await page.evaluate(() => {
    (window as unknown as { scans: unknown[] }).scans.length = 0;
  });

  const codes = Array.from({ length: 20 }, (_, i) => `40063813339${String(i).padStart(2, "0")}`);
  const steps = codes.flatMap((code, i) => [
    ...(i > 0 ? [{ type: "wait", ms: DELAY_MS }] : []),
    { type: "scan", device: "scanner-1", code },
  ]);
  const run = await request.post(`${SERVER}/api/v1/scenarios/run`, { data: { steps } });
  expect(((await run.json()) as { ok: boolean }).ok).toBe(true);

  // The page is still typing the backlog (about 110 ms a scan) after the server has sent the
  // last one; what is checked is order and spacing, not total speed, so allow slow runners.
  await expect
    .poll(() => page.evaluate(() => (window as unknown as { scans: unknown[] }).scans.length), {
      timeout: 20_000,
    })
    .toBe(20);
  const scans = await page.evaluate(
    () => (window as unknown as { scans: { code: string; at: number }[] }).scans,
  );
  expect(scans.map((s) => s.code)).toEqual(codes);
  for (let i = 1; i < scans.length; i++) {
    const gap = (scans[i]?.at ?? 0) - (scans[i - 1]?.at ?? 0);
    // Each scan takes about 110 ms to type (13 digits and Enter, 8 ms apart) plus the delay.
    expect(gap).toBeGreaterThanOrEqual(DELAY_MS - 10);
  }
});
