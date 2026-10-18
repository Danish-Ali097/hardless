import { expect, test } from "@playwright/test";
import { SERVER_PORT } from "../playwright.config.js";

/**
 * PRD M1 acceptance criteria:
 * - In Chrome, the example app selects the virtual port, prints, and the dashboard shows the
 *   rendered receipt within 500 ms and the drawer opening.
 * - Toggling paper-out makes the app's next print fail with a status the app can read.
 */

const SERVER = `http://127.0.0.1:${String(SERVER_PORT)}`;

test.beforeEach(async ({ request }) => {
  for (const id of ["printer-1", "drawer-1"]) {
    await request.post(`${SERVER}/api/v1/devices/${id}/reset`);
  }
});

test("M1: the app prints a cash sale; the dashboard shows the receipt and the open drawer", async ({
  context,
}) => {
  const dashboard = await context.newPage();
  await dashboard.goto(`${SERVER}/`);
  await expect(dashboard.getByRole("status").filter({ hasText: "Live" })).toBeVisible();
  await expect(dashboard.getByText("Nothing printed yet.")).toBeVisible();

  const app = await context.newPage();
  await app.goto("http://localhost:5173/");
  await app.getByRole("button", { name: "Connect printer" }).click();
  // The shim's port picker: choose the emulated printer.
  await app.locator('hardless-port-picker [data-device-id="printer-1"]').click();
  await app.locator('hardless-port-picker [data-action="connect"]').click();
  await expect(app.getByText("Printer connected", { exact: true })).toBeVisible();

  const receipt = dashboard.getByRole("img", { name: /^Receipt 1:/ });
  const start = Date.now();
  await app.getByRole("button", { name: "Pay cash" }).click();
  await expect(receipt).toBeVisible();
  await expect(receipt).toContainText("CORNER CAFE");
  const elapsed = Date.now() - start;
  console.log(`receipt on the dashboard ${String(elapsed)} ms after "Pay cash"`);
  expect(elapsed).toBeLessThan(500);
  await expect(receipt).toContainText(/Total:\s+1,250/);

  await expect(app.getByText("Receipt printed. Drawer opened.")).toBeVisible();
  await expect(dashboard.getByRole("status").filter({ hasText: /^Open/ })).toBeVisible();

  const response = await context.request.get(`${SERVER}/api/v1/devices/drawer-1`);
  const drawer = (await response.json()) as { state: unknown };
  expect(drawer.state).toMatchObject({ state: "open", kicks: 1 });
});

test("M1: with paper out, the app's next print fails with a status it can read", async ({
  context,
}) => {
  const dashboard = await context.newPage();
  await dashboard.goto(`${SERVER}/`);
  const app = await context.newPage();
  await app.goto("http://localhost:5173/");
  await app.getByRole("button", { name: "Connect printer" }).click();
  await app.locator('hardless-port-picker [data-device-id="printer-1"]').click();
  await app.locator('hardless-port-picker [data-action="connect"]').click();
  await expect(app.getByText("Printer connected", { exact: true })).toBeVisible();

  // Toggle paper-out in the dashboard's fault panel.
  await dashboard.getByRole("switch", { name: "Paper out" }).click();
  await expect(dashboard.getByRole("switch", { name: "Paper out" })).toHaveAttribute(
    "aria-checked",
    "true",
  );

  await app.getByRole("button", { name: "Pay card" }).click();
  await expect(app.getByRole("alert")).toHaveText(
    "The receipt did not print: the printer is out of paper.",
  );
  await expect(dashboard.getByText("Nothing printed yet.")).toBeVisible();
});
