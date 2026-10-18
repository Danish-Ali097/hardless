import { expect, test, type Page } from "@playwright/test";
import type { Settled } from "./globals.js";

/**
 * Web Serial shim conformance (PRD section 8.1, FR-14 to FR-17), in standalone mode: the
 * engine runs in the page as WebAssembly, so these tests need no server.
 *
 * Note: Playwright runs `page.evaluate` with a user gesture, so calls made directly in
 * `evaluate` have transient activation. Calls that need a click go through the page's
 * #gesture button, which runs `window.onGesture`.
 */

const DEVICES = [
  {
    kind: "printer-escpos",
    id: "printer-1",
    transport: "serial",
    usbVendorId: 0x1209,
    usbProductId: 1,
  },
  { kind: "printer-escpos", id: "printer-2", transport: "serial" },
  { kind: "cash-drawer", id: "drawer-1", attachedTo: "printer-1" },
];

async function setup(page: Page, devices: object[] = DEVICES): Promise<void> {
  await page.goto("/e2e/page.html");
  await page.waitForFunction(() => window.ready === true);
  await page.evaluate(async (devices) => {
    const installation = window.shim.install({
      mode: "standalone",
      devices: devices as { kind: string }[],
    });
    await installation.transport.ready;
    window.installation = installation as unknown as Window["installation"];
  }, devices);
}

/** Clicks the page's button so `window.onGesture` runs with user activation; settles it. */
async function clickGesture(page: Page): Promise<Settled> {
  await page.click("#gesture");
  return page.evaluate(() => window.result ?? Promise.resolve({ ok: false, name: "NoResult" }));
}

async function choose(page: Page, deviceId: string): Promise<void> {
  await page.locator(`hardless-port-picker [data-device-id="${deviceId}"]`).click();
  await page.locator('hardless-port-picker [data-action="connect"]').click();
}

/** Requests a port through the picker, choosing `deviceId`, as `window.port`. */
async function requestPort(page: Page, deviceId: string): Promise<void> {
  await page.evaluate(() => {
    window.onGesture = () =>
      navigator.serial.requestPort().then((port) => {
        window.port = port;
      });
  });
  const pending = clickGesture(page);
  await choose(page, deviceId);
  expect((await pending).ok).toBe(true);
}

/** Requests printer-1 and opens it at 9600 baud, as `window.port`. */
async function openPrinter(page: Page): Promise<void> {
  await requestPort(page, "printer-1");
  expect(
    await page.evaluate(() => window.settle(window.port.open({ baudRate: 9600 }))),
  ).toMatchObject({ ok: true });
}

test.describe("requestPort()", () => {
  test("FR-16: throws SecurityError without user activation", async ({ page }) => {
    await setup(page);
    // Wait until the activation from page.evaluate (5 seconds in Chrome) has expired, then
    // call requestPort() from a timer.
    await page.evaluate(() => {
      window.result = null;
      const attempt = () => {
        if (navigator.userActivation.isActive) {
          setTimeout(attempt, 250);
          return;
        }
        window.result = window.settle(navigator.serial.requestPort());
      };
      setTimeout(attempt, 250);
    });
    await page.waitForFunction(() => window.result !== null, null, { timeout: 15_000 });
    expect(await page.evaluate(() => window.result)).toMatchObject({
      ok: false,
      name: "SecurityError",
    });
  });

  test("FR-16: shows the picker with matching devices and returns the chosen port", async ({
    page,
  }) => {
    await setup(page);
    await page.evaluate(() => {
      window.onGesture = () => navigator.serial.requestPort().then((p) => p.getInfo());
    });
    const pending = clickGesture(page);
    const picker = page.locator("hardless-port-picker");
    await expect(picker.locator("[data-device-id]")).toHaveCount(2);
    await choose(page, "printer-1");
    expect(await pending).toEqual({ ok: true, value: { usbVendorId: 0x1209, usbProductId: 1 } });
    await expect(picker).toHaveCount(0);
  });

  test("FR-16: filters by usbVendorId and usbProductId", async ({ page }) => {
    await setup(page);
    await page.evaluate(() => {
      window.onGesture = () =>
        navigator.serial.requestPort({ filters: [{ usbVendorId: 0x1209, usbProductId: 1 }] });
    });
    const pending = clickGesture(page);
    await expect(page.locator("hardless-port-picker [data-device-id]")).toHaveCount(1);
    await page.locator('hardless-port-picker [data-action="cancel"]').click();
    await pending;
  });

  test("FR-16: rejects invalid filters with TypeError", async ({ page }) => {
    await setup(page);
    await page.evaluate(() => {
      window.onGesture = () => navigator.serial.requestPort({ filters: [{ usbProductId: 1 }] });
    });
    expect(await clickGesture(page)).toMatchObject({ ok: false, name: "TypeError" });
  });

  test("FR-16: Cancel rejects with NotFoundError", async ({ page }) => {
    await setup(page);
    await page.evaluate(() => {
      window.onGesture = () => navigator.serial.requestPort();
    });
    const pending = clickGesture(page);
    await page.locator('hardless-port-picker [data-action="cancel"]').click();
    expect(await pending).toMatchObject({ ok: false, name: "NotFoundError" });
  });

  test("FR-16: Escape cancels the picker", async ({ page }) => {
    await setup(page);
    await page.evaluate(() => {
      window.onGesture = () => navigator.serial.requestPort();
    });
    const pending = clickGesture(page);
    await page.locator("hardless-port-picker [data-device-id]").first().waitFor();
    await page.keyboard.press("Escape");
    expect(await pending).toMatchObject({ ok: false, name: "NotFoundError" });
  });

  test("FR-16: autoSelect skips the picker", async ({ page }) => {
    await setup(page, [
      { kind: "printer-escpos", id: "auto", transport: "serial", autoSelect: true },
    ]);
    await page.evaluate(() => {
      window.onGesture = () => navigator.serial.requestPort().then(() => "chosen");
    });
    expect(await clickGesture(page)).toEqual({ ok: true, value: "chosen" });
  });

  test("fault picker-cancel: the next request behaves as a user cancel, once", async ({ page }) => {
    await setup(page);
    await page.evaluate(() => {
      window.onGesture = () => navigator.serial.requestPort();
      return window.installation.transport.setFault("printer-1", "picker-cancel", true);
    });
    expect(await clickGesture(page)).toMatchObject({ ok: false, name: "NotFoundError" });
    // The fault was one-shot: the picker shows again.
    const pending = clickGesture(page);
    await expect(page.locator("hardless-port-picker")).toHaveCount(1);
    await page.locator('hardless-port-picker [data-action="cancel"]').click();
    await pending;
  });
});

test.describe("getPorts() and getInfo()", () => {
  test("FR-16: getPorts returns granted ports, and grants survive a reload", async ({ page }) => {
    await setup(page);
    expect(await page.evaluate(() => navigator.serial.getPorts().then((p) => p.length))).toBe(0);
    await openPrinter(page);
    expect(
      await page.evaluate(() =>
        navigator.serial.getPorts().then((p) => p.length === 1 && p[0] === window.port),
      ),
    ).toBe(true);
    expect(await page.evaluate(() => sessionStorage.getItem("hardless:grants"))).toBe(
      '["printer-1"]',
    );

    await page.reload();
    await setup(page);
    expect(
      await page.evaluate(() =>
        navigator.serial.getPorts().then((ports) => ports.map((p) => p.getInfo())),
      ),
    ).toEqual([{ usbVendorId: 0x1209, usbProductId: 1 }]);
  });

  test("FR-16: getInfo is empty for a port without USB IDs", async ({ page }) => {
    await setup(page);
    await requestPort(page, "printer-2");
    expect(await page.evaluate(() => window.port.getInfo())).toEqual({});
  });
});

test.describe("open()", () => {
  test("FR-16: validates options like the spec", async ({ page }) => {
    await setup(page);
    await requestPort(page, "printer-1");
    const outcomes = await page.evaluate(async () => {
      const tries: unknown[] = [
        undefined,
        {},
        { baudRate: 0 },
        { baudRate: 9600, dataBits: 9 },
        { baudRate: 9600 },
        { baudRate: 9600 },
      ];
      const results: string[] = [];
      for (const options of tries) {
        const r = await window.settle(window.port.open(options));
        results.push(r.ok ? "ok" : (r.name ?? "?"));
      }
      return results;
    });
    expect(outcomes).toEqual([
      "TypeError",
      "TypeError",
      "TypeError",
      "TypeError",
      "ok",
      "InvalidStateError",
    ]);
  });

  test("FR-16: readable and writable are real streams that talk to the device", async ({
    page,
  }) => {
    await setup(page);
    await openPrinter(page);
    const reply = await page.evaluate(async () => {
      const readable = window.port.readable;
      const writable = window.port.writable;
      if (!(readable instanceof ReadableStream) || !(writable instanceof WritableStream)) {
        throw new Error("not streams");
      }
      const reader = readable.getReader();
      const writer = writable.getWriter();
      await writer.write(new Uint8Array([0x10, 0x04, 0x01]));
      const { value } = await reader.read();
      reader.releaseLock();
      writer.releaseLock();
      return Array.from(value ?? []);
    });
    expect(reply).toEqual([0x12]);
  });

  test("FR-16: writing something that is not a BufferSource fails with TypeError", async ({
    page,
  }) => {
    await setup(page);
    await openPrinter(page);
    const r = await page.evaluate(() => {
      const writer = (window.port.writable as WritableStream<unknown>).getWriter();
      return window.settle(writer.write("text"));
    });
    expect(r).toMatchObject({ ok: false, name: "TypeError" });
  });

  test("fault slow-write: writes are acknowledged late (back-pressure)", async ({ page }) => {
    await setup(page);
    await openPrinter(page);
    const elapsed = await page.evaluate(async () => {
      await window.installation.transport.setFault("printer-1", "slow-write", true, {
        msPerChunk: 300,
      });
      const writer = window.port.writable?.getWriter();
      const start = performance.now();
      await writer?.write(new Uint8Array([0x41]));
      return performance.now() - start;
    });
    expect(elapsed).toBeGreaterThanOrEqual(290);
  });
});

test.describe("connect and disconnect", () => {
  test("FR-16: disconnect fires on the port and navigator.serial; streams fail with NetworkError", async ({
    page,
  }) => {
    await setup(page);
    await openPrinter(page);
    const r = await page.evaluate(async () => {
      const port = window.port;
      const seen: string[] = [];
      navigator.serial.addEventListener("disconnect", (e) => {
        seen.push(`serial:${String(e.target === port)}`);
      });
      port.addEventListener("disconnect", () => {
        seen.push("port");
      });
      const reader = port.readable?.getReader();
      const read = reader?.read().then(
        () => "read ok",
        (e: unknown) => `read ${(e as DOMException).name}`,
      );
      await window.installation.transport.setFault("printer-1", "offline", true);
      const readResult = await read;
      await new Promise((resolve) => setTimeout(resolve, 50));
      return {
        seen,
        readResult,
        connected: port.connected,
        readable: port.readable,
        writable: port.writable,
      };
    });
    expect(r).toEqual({
      seen: ["port", "serial:true"],
      readResult: "read NetworkError",
      connected: false,
      readable: null,
      writable: null,
    });
  });

  test("fault disconnect-mid-write: the in-flight write rejects with NetworkError", async ({
    page,
  }) => {
    await setup(page);
    await openPrinter(page);
    const r = await page.evaluate(async () => {
      await window.installation.transport.setFault("printer-1", "disconnect-mid-write", true, {
        afterBytes: 4,
      });
      const writer = window.port.writable?.getWriter();
      return window.settle(writer?.write(new TextEncoder().encode("Total: 1,250\n")));
    });
    expect(r).toMatchObject({ ok: false, name: "NetworkError" });
  });

  test("FR-16: reconnect fires connect, and the port can be reopened", async ({ page }) => {
    await setup(page);
    await openPrinter(page);
    const r = await page.evaluate(async () => {
      const port = window.port;
      const transport = window.installation.transport;
      const tick = () => new Promise((resolve) => setTimeout(resolve, 20));
      let connects = 0;
      navigator.serial.onconnect = () => {
        connects++;
      };
      await transport.setFault("printer-1", "offline", true);
      await tick();
      await port.close();
      await transport.setFault("printer-1", "offline", false);
      await tick();
      await port.open({ baudRate: 9600 });
      return { connects, connected: port.connected, readable: port.readable !== null };
    });
    expect(r).toEqual({ connects: 1, connected: true, readable: true });
  });
});

test.describe("close(), signals and forget()", () => {
  test("close() rejects with TypeError while a stream is locked, as Chrome does", async ({
    page,
  }) => {
    // PRD 8.1 says InvalidStateError; the spec cancels the readable, which fails on a locked
    // stream, and Chrome reports TypeError. The shim follows Chrome (see docs/decisions.md).
    await setup(page);
    await openPrinter(page);
    const locked = await page.evaluate(() => {
      const reader = window.port.readable?.getReader();
      if (reader) window.reader = reader;
      return window.settle(window.port.close());
    });
    expect(locked).toMatchObject({ ok: false, name: "TypeError" });
    const closed = await page.evaluate(async () => {
      window.reader.releaseLock();
      await window.port.close();
      return [window.port.readable, window.port.writable];
    });
    expect(closed).toEqual([null, null]);
    expect(await page.evaluate(() => window.settle(window.port.close()))).toMatchObject({
      ok: false,
      name: "InvalidStateError",
    });
  });

  test("FR-16: setSignals and getSignals", async ({ page }) => {
    await setup(page);
    await requestPort(page, "printer-1");
    const r = await page.evaluate(async () => {
      const port = window.port;
      const before = await window.settle(port.setSignals({ dataTerminalReady: true }));
      await port.open({ baudRate: 9600 });
      const empty = await window.settle(port.setSignals({}));
      const set = await window.settle(
        port.setSignals({ dataTerminalReady: true, requestToSend: false }),
      );
      const signals = await port.getSignals();
      return { before: before.name, empty: empty.name, set: set.ok, signals };
    });
    expect(r).toEqual({
      before: "InvalidStateError",
      empty: "TypeError",
      set: true,
      signals: {
        dataCarrierDetect: false,
        clearToSend: true,
        ringIndicator: false,
        dataSetReady: true,
      },
    });
  });

  test("forget() removes the grant", async ({ page }) => {
    await setup(page);
    await openPrinter(page);
    const count = await page.evaluate(async () => {
      await window.port.forget();
      return (await navigator.serial.getPorts()).length;
    });
    expect(count).toBe(0);
  });
});

test.describe("install()", () => {
  test("FR-17: install is idempotent and FR-14: uninstall restores navigator.serial", async ({
    page,
  }) => {
    await page.goto("/e2e/page.html");
    await page.waitForFunction(() => window.ready === true);
    const r = await page.evaluate(() => {
      const original: unknown = Reflect.get(Navigator.prototype, "serial", navigator);
      const a = window.shim.install({ mode: "standalone", devices: [] });
      const b = window.shim.install({ mode: "server", url: "ws://example.invalid/bridge" });
      const shimmed = navigator.serial === a.serial;
      window.shim.uninstall();
      return { same: a === b, shimmed, restored: (navigator.serial as unknown) === original };
    });
    expect(r).toEqual({ same: true, shimmed: true, restored: true });
  });

  test("FR-14: serial can be turned off", async ({ page }) => {
    await page.goto("/e2e/page.html");
    await page.waitForFunction(() => window.ready === true);
    const r = await page.evaluate(() => {
      const before: unknown = navigator.serial;
      const installation = window.shim.install({ mode: "standalone", devices: [], serial: false });
      return { untouched: (navigator.serial as unknown) === before, serial: installation.serial };
    });
    expect(r).toEqual({ untouched: true, serial: null });
  });
});
