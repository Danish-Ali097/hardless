import { expect, test, type Page } from "@playwright/test";
import type { Settled } from "./globals.js";

/**
 * Web NFC shim conformance (PRD section 8.3, FR-14, FR-16, FR-27) in a real Chromium, in
 * standalone mode: taps come from the engine running in the page as WebAssembly.
 *
 * Playwright runs `page.evaluate` with a user gesture, so `scan()` and `write()` called in
 * `evaluate` have transient activation.
 */

const READER = { kind: "nfc-reader", id: "nfc-1" };
const UID = "04a2b3c4d5e6f7";
const SERIAL = "04:a2:b3:c4:d5:e6:f7";

async function setup(page: Page, devices: object[] = [READER]): Promise<void> {
  await page.goto("/e2e/page.html");
  await page.waitForFunction(() => window.ready === true);
  await page.evaluate(async (devices) => {
    const installation = window.shim.install({
      mode: "standalone",
      devices: devices as { kind: string }[],
    });
    await installation.transport.ready;
    window.installation = installation as unknown as Window["installation"];
    window.readings = [];
    window.readingErrors = 0;
  }, devices);
}

/** Starts `window.ndef.scan()` and records what it reads, decoding text payloads. */
async function scan(page: Page): Promise<Settled> {
  return page.evaluate(() => {
    window.ndef = new NDEFReader();
    window.ndef.onreading = (event) => {
      const e = event as InstanceType<typeof NDEFReadingEvent>;
      window.readings.push({
        serialNumber: e.serialNumber,
        records: e.message.records.map((r) => ({
          recordType: r.recordType,
          mediaType: r.mediaType,
          id: r.id,
          encoding: r.encoding,
          lang: r.lang,
          isDataView: r.data instanceof DataView,
          text: r.data ? new TextDecoder().decode(r.data) : null,
        })),
      });
    };
    window.ndef.onreadingerror = () => {
      window.readingErrors++;
    };
    return window.settle(window.ndef.scan());
  });
}

async function tap(page: Page, uid = UID, records?: object[]): Promise<void> {
  await page.evaluate(
    ({ uid, records }) => window.installation.tap("nfc-1", uid, records as never),
    { uid, records },
  );
}

async function setFault(page: Page, fault: string, active = true): Promise<void> {
  await page.evaluate(
    ({ fault, active }) => window.installation.transport.setFault("nfc-1", fault, active),
    { fault, active },
  );
}

async function readerState(page: Page): Promise<Record<string, unknown>> {
  return page.evaluate(
    () =>
      window.installation.transport.devices().find((d) => d.id === "nfc-1")?.state as Record<
        string,
        unknown
      >,
  );
}

test("FR-14: new NDEFReader() is available, with the other Web NFC globals", async ({ page }) => {
  await setup(page);
  expect(
    await page.evaluate(() => ({
      reader: typeof new NDEFReader().scan,
      event: typeof NDEFReadingEvent,
      message: new NDEFMessage({ records: [{ recordType: "url", data: "https://a.example/" }] })
        .records[0]?.recordType,
      record: new NDEFRecord({ recordType: "text", data: "hi" }).lang,
    })),
  ).toEqual({ reader: "function", event: "function", message: "url", record: "en" });
});

test("FR-14: uninstall() restores the page's own Web NFC globals", async ({ page }) => {
  await setup(page);
  expect(
    await page.evaluate(() => {
      window.shim.uninstall();
      return "NDEFReader" in globalThis;
    }),
  ).toBe(false);
});

test.describe("scan()", () => {
  test("8.3: rejects with SecurityError without user activation on the first call", async ({
    page,
  }) => {
    await setup(page);
    // Wait until the activation from page.evaluate (5 seconds in Chrome) has expired.
    await page.evaluate(() => {
      window.result = null;
      const attempt = () => {
        if (navigator.userActivation.isActive) {
          setTimeout(attempt, 250);
          return;
        }
        window.result = window.settle(new NDEFReader().scan());
      };
      setTimeout(attempt, 250);
    });
    await page.waitForFunction(() => window.result !== null, null, { timeout: 15_000 });
    expect(await page.evaluate(() => window.result)).toMatchObject({
      ok: false,
      name: "SecurityError",
    });
  });

  test("fault permission-denied: rejects with NotAllowedError", async ({ page }) => {
    await setup(page);
    await setFault(page, "permission-denied");
    expect(await scan(page)).toMatchObject({ ok: false, name: "NotAllowedError" });
  });

  test("fault picker-cancel: the next scan() rejects with NotAllowedError, once", async ({
    page,
  }) => {
    await setup(page);
    await setFault(page, "picker-cancel");
    expect(await scan(page)).toMatchObject({ ok: false, name: "NotAllowedError" });
    expect(await scan(page)).toMatchObject({ ok: true });
  });

  test("8.3: a second scan() on the same reader rejects with InvalidStateError", async ({
    page,
  }) => {
    await setup(page);
    expect(await scan(page)).toMatchObject({ ok: true });
    expect(await page.evaluate(() => window.settle(window.ndef.scan()))).toMatchObject({
      ok: false,
      name: "InvalidStateError",
    });
  });

  test("8.3: rejects with NotSupportedError when no NFC reader is emulated", async ({ page }) => {
    await setup(page, [{ kind: "scanner-wedge", id: "scanner-1" }]);
    expect(await scan(page)).toMatchObject({ ok: false, name: "NotSupportedError" });
  });

  test("8.3: an aborted signal rejects, and aborting stops reading", async ({ page }) => {
    await setup(page);
    expect(
      await page.evaluate(() =>
        window.settle(new NDEFReader().scan({ signal: AbortSignal.abort() })),
      ),
    ).toMatchObject({ ok: false, name: "AbortError" });

    await page.evaluate(async () => {
      const controller = new AbortController();
      window.ndef = new NDEFReader();
      window.ndef.onreading = () => window.readings.push({ serialNumber: "", records: [] });
      await window.ndef.scan({ signal: controller.signal });
      controller.abort();
    });
    await tap(page);
    expect(await page.evaluate(() => window.readings.length)).toBe(0);
  });
});

test.describe("reading event", () => {
  test("8.3: a tap fires reading with serialNumber and NDEFRecords", async ({ page }) => {
    await setup(page);
    expect(await scan(page)).toMatchObject({ ok: true });
    await tap(page, UID, [
      { recordType: "text", data: "Hello", lang: "fr", id: "greeting" },
      { recordType: "url", data: "https://example.com/" },
      { recordType: "mime", mediaType: "application/json", data: '{"points":12}' },
      { recordType: "empty" },
    ]);
    expect(await page.evaluate(() => window.readings)).toEqual([
      {
        serialNumber: SERIAL,
        records: [
          {
            recordType: "text",
            mediaType: null,
            id: "greeting",
            encoding: "utf-8",
            lang: "fr",
            isDataView: true,
            text: "Hello",
          },
          {
            recordType: "url",
            mediaType: null,
            id: null,
            encoding: null,
            lang: null,
            isDataView: true,
            text: "https://example.com/",
          },
          {
            recordType: "mime",
            mediaType: "application/json",
            id: null,
            encoding: null,
            lang: null,
            isDataView: true,
            text: '{"points":12}',
          },
          {
            recordType: "empty",
            mediaType: null,
            id: null,
            encoding: null,
            lang: null,
            isDataView: false,
            text: null,
          },
        ],
      },
    ]);
  });

  test("fault read-error: the next tap fires readingerror instead", async ({ page }) => {
    await setup(page);
    await scan(page);
    await setFault(page, "read-error");
    await tap(page);
    expect(await page.evaluate(() => [window.readingErrors, window.readings.length])).toEqual([
      1, 0,
    ]);
    await tap(page);
    expect(await page.evaluate(() => [window.readingErrors, window.readings.length])).toEqual([
      1, 1,
    ]);
  });
});

test.describe("write()", () => {
  test("8.3: without a card it waits for a tap, then writes to that card", async ({ page }) => {
    await setup(page);
    await page.evaluate(() => {
      window.result = window.settle(
        new NDEFReader().write({ records: [{ recordType: "url", data: "https://example.com/" }] }),
      );
    });
    expect(
      await page.evaluate(() =>
        Promise.race([
          window.result,
          new Promise((resolve) =>
            setTimeout(() => {
              resolve("still waiting");
            }, 200),
          ),
        ]),
      ),
    ).toBe("still waiting");
    await tap(page);
    expect(await page.evaluate(() => window.result)).toEqual({ ok: true });
    const state = await readerState(page);
    expect(state.lastWritten).toEqual({
      uid: SERIAL,
      records: [{ recordType: "url", data: "https://example.com/" }],
    });
  });

  test("8.3: writes a string as a text record to the card on the reader", async ({ page }) => {
    await setup(page);
    await tap(page);
    expect(await page.evaluate(() => window.settle(new NDEFReader().write("Member 1234")))).toEqual(
      { ok: true },
    );
    await scan(page);
    await tap(page);
    expect(await page.evaluate(() => window.readings[0]?.records[0]?.text)).toBe("Member 1234");
  });

  test("8.3: rejects bad records with TypeError before waiting", async ({ page }) => {
    await setup(page);
    expect(
      await page.evaluate(() =>
        window.settle(new NDEFReader().write({ records: [{ recordType: "url" }] })),
      ),
    ).toMatchObject({ ok: false, name: "TypeError" });
  });

  test("8.3: an abort signal cancels a waiting write", async ({ page }) => {
    await setup(page);
    expect(
      await page.evaluate(async () => {
        const controller = new AbortController();
        const pending = window.settle(new NDEFReader().write("x", { signal: controller.signal }));
        controller.abort();
        return pending;
      }),
    ).toMatchObject({ ok: false, name: "AbortError" });
  });

  test("8.3: overwrite: false refuses a card with data (NotAllowedError)", async ({ page }) => {
    await setup(page);
    await tap(page, UID, [{ recordType: "text", data: "keep" }]);
    expect(
      await page.evaluate(() => window.settle(new NDEFReader().write("x", { overwrite: false }))),
    ).toMatchObject({ ok: false, name: "NotAllowedError" });
  });
});

test.describe("makeReadOnly()", () => {
  test("8.3: locks the card; later writes reject with NotAllowedError", async ({ page }) => {
    await setup(page);
    await tap(page);
    expect(await page.evaluate(() => window.settle(new NDEFReader().makeReadOnly()))).toEqual({
      ok: true,
    });
    expect(await page.evaluate(() => window.settle(new NDEFReader().write("x")))).toMatchObject({
      ok: false,
      name: "NotAllowedError",
    });
    const state = await readerState(page);
    expect((state.cards as { readOnly: boolean }[])[0]?.readOnly).toBe(true);
  });
});
