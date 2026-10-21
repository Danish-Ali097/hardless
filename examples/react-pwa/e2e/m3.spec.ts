import { expect, test, type Hardless } from "@hardless/playwright";
import type { Page } from "@playwright/test";
import { APP_PORT } from "../playwright.config.js";

/**
 * PRD M3 acceptance: the example app's Playwright suite covers print, drawer, scan, NFC read
 * and write, and three fault cases, with `@hardless/playwright`. The app is a production
 * build, so the only shim is the one the fixture injects. The suite runs twice (see
 * playwright.config.ts): with the devices in the page (standalone) and against a server.
 */

const APP = `http://127.0.0.1:${String(APP_PORT)}/`;

/** Opens the app and connects the Web Serial printer (choosing it if a picker shows). */
async function openWithPrinter(page: Page): Promise<void> {
  await page.goto(APP);
  await page.getByRole("button", { name: "Connect printer" }).click();
  const choice = page.locator('hardless-port-picker [data-device-id="printer-1"]');
  const connected = page.getByText("Printer connected", { exact: true });
  await expect(choice.or(connected).first()).toBeVisible();
  if (await choice.isVisible()) {
    await choice.click();
    await page.locator('hardless-port-picker [data-action="connect"]').click();
  }
  await expect(connected).toBeVisible();
}

test("M3: a cash sale prints a cut receipt and opens the drawer once", async ({
  page,
  hardless,
}) => {
  await openWithPrinter(page);
  await page.getByRole("button", { name: "Pay cash" }).click();
  await expect(hardless.printer("printer-1")).toHavePrinted(/Total:\s+1,250/);
  await expect(hardless.printer("printer-1")).toHavePrintedJobs(1);
  await expect(hardless.printer("printer-1")).toHaveCut();
  await expect(hardless.drawer("drawer-1")).toHaveOpened({ times: 1 });
  await expect(page.getByText("Receipt printed. Drawer opened.")).toBeVisible();
});

test("M3: a scanned barcode is added to the order and printed", async ({ page, hardless }) => {
  await openWithPrinter(page);
  await page.getByLabel("Scan or type a barcode").focus();
  await hardless.scanner("scanner-1").scan("5012345678900");
  await expect(page.getByText("1 × Orange juice")).toBeVisible();
  await page.getByRole("button", { name: "Pay card" }).click();
  await expect(hardless.printer("printer-1")).toHavePrinted("Orange juice");
  await expect(hardless.drawer("drawer-1")).toBeClosed();
});

async function scanCard(page: Page, hardless: Hardless, records?: object[]): Promise<void> {
  await page.getByRole("button", { name: "Scan loyalty card" }).click();
  await expect(page.getByText("Tap the loyalty card on the reader.")).toBeVisible();
  await hardless.nfc("nfc-1").tap("04a2b3c4", records as never);
}

test("M3: a loyalty card is read over NFC and stamped by writing to it", async ({
  page,
  hardless,
}) => {
  await page.goto(APP);
  await scanCard(page, hardless, [{ recordType: "text", data: "Visits: 2", lang: "en" }]);
  await expect(page.getByText("Card 04:a2:b3:c4: Visits: 2", { exact: true })).toBeVisible();
  // The card is still on the reader (for 2 s), so the write goes straight to it.
  await page.getByRole("button", { name: "Stamp card" }).click();
  await expect(hardless.nfc("nfc-1")).toHaveWrittenNdef([
    { recordType: "text", data: "Visits: 3" },
  ]);
  await expect(page.getByText("Card stamped: Visits: 3.")).toBeVisible();
});

test("M3 fault paper-out: the app reports that the receipt did not print", async ({
  page,
  hardless,
}) => {
  await openWithPrinter(page);
  await hardless.printer("printer-1").fault("paper-out");
  await page.getByRole("button", { name: "Pay card" }).click();
  await expect(page.getByRole("alert")).toHaveText(
    "The receipt did not print: the printer is out of paper.",
  );
  await expect(hardless.printer("printer-1")).toHavePrintedJobs(0);
  await hardless.printer("printer-1").clearFault("paper-out");
});

test("M3 fault offline: the app sees the printer disconnect", async ({ page, hardless }) => {
  await openWithPrinter(page);
  await hardless.printer("printer-1").fault("offline");
  await expect(page.getByRole("alert")).toHaveText("The printer was disconnected.");
  await hardless.printer("printer-1").clearFault("offline");
});

test("M3 fault read-error: the app asks for the card again", async ({ page, hardless }) => {
  await page.goto(APP);
  await hardless.nfc("nfc-1").fault("read-error");
  await scanCard(page, hardless);
  await expect(page.getByRole("alert")).toHaveText("The card could not be read. Tap it again.");
});
