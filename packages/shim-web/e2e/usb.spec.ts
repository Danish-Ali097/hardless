import { expect, test, type Page } from "@playwright/test";
import type { Settled } from "./globals.js";

/**
 * WebUSB shim conformance (PRD section 8.2, FR-14, FR-16) in a real Chromium, in standalone
 * mode: the USB printers run in the page as WebAssembly.
 *
 * Playwright runs `page.evaluate` with a user gesture, so `requestDevice()` called in
 * `evaluate` has transient activation; calls that need the picker go through #gesture.
 */

const DEVICES = [
  { kind: "printer-escpos", id: "usb-1", transport: "usb", productName: "Kitchen printer" },
  {
    kind: "printer-escpos",
    id: "usb-2",
    transport: "usb",
    usbVendorId: 0x0fe6,
    usbProductId: 0x811e,
    serialNumber: "SN-2",
  },
  { kind: "printer-escpos", id: "serial-1", transport: "serial" },
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
    window.usbEvents = [];
    for (const type of ["connect", "disconnect"]) {
      navigator.usb.addEventListener(type, (event) => {
        const e = event as Event & { device: { serialNumber: string | null } };
        window.usbEvents.push({ type: e.type, device: e.device.serialNumber ?? "" });
      });
    }
  }, devices);
}

async function clickGesture(page: Page): Promise<Settled> {
  await page.click("#gesture");
  return page.evaluate(() => window.result ?? Promise.resolve({ ok: false, name: "NoResult" }));
}

/** Requests a device through the picker, choosing `deviceId`, as `window.usbDevice`. */
async function requestDevice(page: Page, deviceId: string, filters: object[] = []): Promise<void> {
  await page.evaluate((filters) => {
    window.onGesture = () =>
      navigator.usb.requestDevice({ filters }).then((d) => {
        window.usbDevice = d;
      });
  }, filters);
  const pending = clickGesture(page);
  await page.locator(`hardless-port-picker [data-device-id="${deviceId}"]`).click();
  await page.locator('hardless-port-picker [data-action="connect"]').click();
  expect((await pending).ok).toBe(true);
}

/** Requests usb-1, opens it and claims interface 0. */
async function claim(page: Page): Promise<void> {
  await requestDevice(page, "usb-1");
  expect(
    await page.evaluate(async () => {
      const d = window.usbDevice;
      await d.open();
      if (d.configuration === null) await d.selectConfiguration(1);
      return window.settle(d.claimInterface(0));
    }),
  ).toMatchObject({ ok: true });
}

function setFault(page: Page, fault: string, active = true): Promise<void> {
  return page.evaluate(
    ({ fault, active }) => window.installation.transport.setFault("usb-1", fault, active),
    { fault, active },
  );
}

test.describe("requestDevice()", () => {
  test("8.2: throws SecurityError without user activation", async ({ page }) => {
    await setup(page);
    await page.evaluate(() => {
      window.result = null;
      const attempt = () => {
        if (navigator.userActivation.isActive) {
          setTimeout(attempt, 250);
          return;
        }
        window.result = window.settle(navigator.usb.requestDevice({ filters: [] }));
      };
      setTimeout(attempt, 250);
    });
    await page.waitForFunction(() => window.result !== null, null, { timeout: 15_000 });
    expect(await page.evaluate(() => window.result)).toMatchObject({
      ok: false,
      name: "SecurityError",
    });
  });

  test("8.2: missing filters is a TypeError; bad filters too", async ({ page }) => {
    await setup(page);
    const results = await page.evaluate(() =>
      Promise.all([
        window.settle(navigator.usb.requestDevice(undefined as never)),
        window.settle(navigator.usb.requestDevice({ filters: [{ productId: 1 }] })),
        window.settle(navigator.usb.requestDevice({ filters: [{ subclassCode: 1 }] })),
      ]),
    );
    for (const r of results) expect(r).toMatchObject({ ok: false, name: "TypeError" });
  });

  test("8.2: an empty filter list shows every USB printer; the chosen one is returned", async ({
    page,
  }) => {
    await setup(page);
    await page.evaluate(() => {
      window.onGesture = () =>
        navigator.usb.requestDevice({ filters: [] }).then((d) => d.productName);
    });
    const pending = clickGesture(page);
    const picker = page.locator("hardless-port-picker");
    await expect(picker.locator("[data-device-id]")).toHaveCount(2);
    await expect(picker).toContainText("Kitchen printer (usb-1)");
    await page.locator('hardless-port-picker [data-device-id="usb-1"]').click();
    await page.locator('hardless-port-picker [data-action="connect"]').click();
    expect(await pending).toEqual({ ok: true, value: "Kitchen printer" });
  });

  test("8.2: filters match vendor and product IDs and the printer interface class", async ({
    page,
  }) => {
    await setup(page);
    await page.evaluate(() => {
      window.onGesture = () =>
        navigator.usb.requestDevice({ filters: [{ vendorId: 0x0fe6, productId: 0x811e }] });
    });
    void clickGesture(page);
    await expect(page.locator("hardless-port-picker [data-device-id]")).toHaveCount(1);
    await page.locator('hardless-port-picker [data-action="cancel"]').click();
    expect(await page.evaluate(() => window.result)).toMatchObject({
      ok: false,
      name: "NotFoundError",
    });

    await page.evaluate(() => {
      window.onGesture = () =>
        navigator.usb.requestDevice({
          filters: [{ classCode: 7, subclassCode: 1, protocolCode: 2 }],
        });
    });
    void clickGesture(page);
    await expect(page.locator("hardless-port-picker [data-device-id]")).toHaveCount(2);
    await page.keyboard.press("Escape");
    await page.evaluate(() => {
      window.onGesture = () => navigator.usb.requestDevice({ filters: [{ classCode: 3 }] });
    });
    void clickGesture(page);
    await expect(page.locator("hardless-port-picker")).toContainText("No compatible devices");
    await page.keyboard.press("Escape");
  });

  test("fault picker-cancel: the next request rejects with NotFoundError, once", async ({
    page,
  }) => {
    await setup(page);
    await setFault(page, "picker-cancel");
    expect(
      await page.evaluate(() =>
        window.settle(navigator.usb.requestDevice({ filters: [{ serialNumber: "usb-1" }] })),
      ),
    ).toMatchObject({ ok: false, name: "NotFoundError" });
    await requestDevice(page, "usb-1");
  });
});

test("8.2: getDevices() returns the devices granted earlier, also after a reload", async ({
  page,
}) => {
  await setup(page);
  expect(await page.evaluate(() => navigator.usb.getDevices().then((d) => d.length))).toBe(0);
  await requestDevice(page, "usb-2");
  await setup(page);
  expect(
    await page.evaluate(() => navigator.usb.getDevices().then((d) => d.map((x) => x.serialNumber))),
  ).toEqual(["SN-2"]);
});

test("8.2: descriptors describe a printer-class device with bulk OUT and IN endpoints", async ({
  page,
}) => {
  await setup(page);
  await requestDevice(page, "usb-1");
  expect(
    await page.evaluate(() => {
      const d = window.usbDevice;
      const alt = d.configuration?.interfaces[0]?.alternate;
      return {
        vendorId: d.vendorId,
        productId: d.productId,
        manufacturerName: d.manufacturerName,
        productName: d.productName,
        serialNumber: d.serialNumber,
        opened: d.opened,
        configurations: d.configurations.length,
        class: [alt?.interfaceClass, alt?.interfaceSubclass, alt?.interfaceProtocol],
        endpoints: alt?.endpoints.map((e) => [e.endpointNumber, e.direction, e.type]),
      };
    }),
  ).toEqual({
    vendorId: 0x1209,
    productId: 1,
    manufacturerName: "Hardless",
    productName: "Kitchen printer",
    serialNumber: "usb-1",
    opened: false,
    configurations: 1,
    class: [7, 1, 2],
    endpoints: [
      [1, "out", "bulk"],
      [2, "in", "bulk"],
    ],
  });
});

test.describe("open, selectConfiguration and claimInterface", () => {
  test("8.2: are enforced in order (InvalidStateError, NotFoundError)", async ({ page }) => {
    await setup(page);
    await requestDevice(page, "usb-1");
    const results = await page.evaluate(async () => {
      const d = window.usbDevice;
      const before = await window.settle(d.claimInterface(0));
      const config = await window.settle(d.selectConfiguration(1));
      await d.open();
      const badConfig = await window.settle(d.selectConfiguration(2));
      const badInterface = await window.settle(d.claimInterface(3));
      const unclaimed = await window.settle(d.transferOut(1, new Uint8Array([0x0a])));
      const ok = await window.settle(d.claimInterface(0));
      return { before, config, badConfig, badInterface, unclaimed, ok, opened: d.opened };
    });
    expect(results).toMatchObject({
      before: { ok: false, name: "InvalidStateError" },
      config: { ok: false, name: "InvalidStateError" },
      badConfig: { ok: false, name: "NotFoundError" },
      badInterface: { ok: false, name: "NotFoundError" },
      unclaimed: { ok: false, name: "NotFoundError" },
      ok: { ok: true },
      opened: true,
    });
  });

  test("fault interface-busy: claimInterface rejects with NetworkError", async ({ page }) => {
    await setup(page);
    await setFault(page, "interface-busy");
    await requestDevice(page, "usb-1");
    expect(
      await page.evaluate(async () => {
        await window.usbDevice.open();
        return window.settle(window.usbDevice.claimInterface(0));
      }),
    ).toMatchObject({ ok: false, name: "NetworkError" });
  });
});

test.describe("transfers", () => {
  test("8.2: transferOut prints and resolves with bytesWritten and status ok", async ({ page }) => {
    await setup(page);
    await claim(page);
    const result = await page.evaluate(async () => {
      const r = await window.usbDevice.transferOut(
        1,
        new TextEncoder().encode("Total: 1,250\n\x1dV\x00"),
      );
      return { status: r.status, bytesWritten: r.bytesWritten };
    });
    expect(result).toEqual({ status: "ok", bytesWritten: 16 });
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (
              window.installation.transport.devices().find((d) => d.id === "usb-1")?.state as {
                jobs: number;
              }
            ).jobs,
        ),
      )
      .toBe(1);
  });

  test("8.2: transferIn returns DLE EOT status replies", async ({ page }) => {
    await setup(page);
    await claim(page);
    expect(
      await page.evaluate(async () => {
        const d = window.usbDevice;
        const reply = d.transferIn(2, 64);
        await d.transferOut(1, new Uint8Array([0x10, 0x04, 0x01]));
        const r = await reply;
        return {
          status: r.status,
          bytes: Array.from(new Uint8Array(r.data?.buffer ?? new ArrayBuffer(0))),
        };
      }),
    ).toEqual({ status: "ok", bytes: [0x12] });
  });

  test("fault stall: transferOut returns status stall", async ({ page }) => {
    await setup(page);
    await claim(page);
    await setFault(page, "stall");
    expect(
      await page.evaluate(async () => {
        const r = await window.usbDevice.transferOut(1, new Uint8Array([0x0a]));
        await window.usbDevice.clearHalt("out", 1);
        return [r.status, r.bytesWritten];
      }),
    ).toEqual(["stall", 0]);
  });

  test("8.2: a disconnected device rejects transfers with NotFoundError", async ({ page }) => {
    await setup(page);
    await claim(page);
    await setFault(page, "offline");
    expect(
      await page.evaluate(() =>
        window.settle(window.usbDevice.transferOut(1, new Uint8Array([1]))),
      ),
    ).toMatchObject({ ok: false, name: "NotFoundError" });
  });
});

test("8.2: controlTransferIn answers GET_DEVICE_ID and GET_PORT_STATUS", async ({ page }) => {
  await setup(page);
  await claim(page);
  const replies = await page.evaluate(async () => {
    const d = window.usbDevice;
    const setup = { requestType: "class", recipient: "interface", value: 0, index: 0 } as const;
    const id = await d.controlTransferIn({ ...setup, request: 0 }, 1024);
    const status = await d.controlTransferIn({ ...setup, request: 1 }, 1);
    const vendor = await d.controlTransferIn({ ...setup, requestType: "vendor", request: 9 }, 8);
    const idBytes = new Uint8Array(id.data?.buffer ?? new ArrayBuffer(0));
    return {
      length: (idBytes[0] ?? 0) * 256 + (idBytes[1] ?? 0),
      id: new TextDecoder().decode(idBytes.slice(2)),
      status: new Uint8Array(status.data?.buffer ?? new ArrayBuffer(0))[0],
      vendor: vendor.status,
    };
  });
  expect(replies).toEqual({
    length: 2 + "MFG:Hardless;MDL:Kitchen printer;CMD:ESC/POS;CLS:PRINTER;".length,
    id: "MFG:Hardless;MDL:Kitchen printer;CMD:ESC/POS;CLS:PRINTER;",
    status: 0x18,
    vendor: "stall",
  });
});

test("8.2: connect and disconnect fire USBConnectionEvent on navigator.usb", async ({ page }) => {
  await setup(page);
  await requestDevice(page, "usb-1");
  await setFault(page, "offline");
  await setFault(page, "offline", false);
  await expect
    .poll(() => page.evaluate(() => window.usbEvents))
    .toEqual([
      { type: "disconnect", device: "usb-1" },
      { type: "connect", device: "usb-1" },
    ]);
});
