import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";

/**
 * The shim in server mode (FR-15, FR-18) against the real `hardless` binary: build it first
 * with `cargo build -p hardless-cli`. Skipped when the binary is missing.
 */

const PORT = 18181;
const API = `http://127.0.0.1:${String(PORT)}/api/v1`;
const binary = fileURLToPath(
  new URL(
    `../../../target/debug/hardless${process.platform === "win32" ? ".exe" : ""}`,
    import.meta.url,
  ),
);

let server: ChildProcess | undefined;

test.describe.configure({ mode: "serial" });

test.beforeAll(async () => {
  test.skip(!existsSync(binary), "build the server first: cargo build -p hardless-cli");
  server = spawn(binary, ["start", "--port", String(PORT)], {
    stdio: "ignore",
    // No HTTPS in tests: it would use port 8443 and the user's certificate folder.
    env: { ...process.env, HARDLESS_HTTPS: "0" },
  });
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(`${API}/health`)).ok) return;
    } catch {
      // Not listening yet.
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error("the hardless server did not start");
});

test.afterAll(() => {
  server?.kill();
});

test.beforeEach(async () => {
  for (const id of ["printer-1", "drawer-1", "scanner-1", "nfc-1"]) {
    await fetch(`${API}/devices/${id}/reset`, { method: "POST" });
  }
});

async function connect(page: Page): Promise<void> {
  await page.goto("/e2e/page.html");
  await page.waitForFunction(() => window.ready === true);
  await page.evaluate(async (port) => {
    const installation = window.shim.install({
      mode: "server",
      url: `ws://127.0.0.1:${String(port)}/bridge`,
    });
    await installation.transport.ready;
    window.onGesture = () =>
      navigator.serial.requestPort().then((p) => {
        window.port = p;
      });
  }, PORT);
  await page.click("#gesture");
  await page.locator('hardless-port-picker [data-device-id="printer-1"]').click();
  await page.locator('hardless-port-picker [data-action="connect"]').click();
  await page.evaluate(() => window.result);
  await page.evaluate(() => window.port.open({ baudRate: 9600 }));
}

async function put(path: string, body: object): Promise<void> {
  await fetch(`${API}${path}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

test("FR-15: a print through the bridge becomes a job on the server", async ({ page }) => {
  await connect(page);
  const status = await page.evaluate(async () => {
    const writer = window.port.writable?.getWriter();
    const reader = window.port.readable?.getReader();
    await writer?.write(new TextEncoder().encode("Total: 1,250\n\x1dV\x00"));
    await writer?.write(new Uint8Array([0x10, 0x04, 0x01]));
    const { value } = (await reader?.read()) ?? {};
    return Array.from(value ?? []);
  });
  expect(status).toEqual([0x12]);
  const job = (await (await fetch(`${API}/devices/printer-1/jobs/last`)).json()) as {
    text: string;
  };
  expect(job.text).toBe("Total: 1,250\n");
});

test("FR-15: signals reach the server's event log", async ({ page }) => {
  await connect(page);
  await page.evaluate(() => window.port.setSignals({ dataTerminalReady: true }));
  const events = (await (await fetch(`${API}/devices/printer-1/events`)).json()) as {
    type: string;
  }[];
  expect(events.some((e) => e.type === "signals")).toBe(true);
});

test("FR-15: faults set on the server reach the page", async ({ page }) => {
  await connect(page);
  await page.evaluate(() => {
    window.result = null;
    navigator.serial.addEventListener("disconnect", () => {
      window.result = Promise.resolve({ ok: true, value: "disconnected" });
    });
  });
  await put("/devices/printer-1/faults/offline", { active: true });
  await page.waitForFunction(() => window.result !== null);
  expect(await page.evaluate(() => window.port.connected)).toBe(false);
  await put("/devices/printer-1/faults/offline", { active: false });
  await expect.poll(() => page.evaluate(() => window.port.connected)).toBe(true);
});

test("FR-15: a dropped bridge is reported as a disconnect", async ({ page }) => {
  await connect(page);
  // Listen first, then stop the server.
  await page.evaluate(() => {
    window.result = null;
    window.port.addEventListener("disconnect", () => {
      window.result = Promise.resolve({ ok: true, value: "disconnect" });
    });
  });
  server?.kill();
  await page.waitForFunction(() => window.result !== null);
  expect(await page.evaluate(() => window.result)).toEqual({ ok: true, value: "disconnect" });
  // Restart for any later tests.
  server = spawn(binary, ["start", "--port", String(PORT)], {
    stdio: "ignore",
    // No HTTPS in tests: it would use port 8443 and the user's certificate folder.
    env: { ...process.env, HARDLESS_HTTPS: "0" },
  });
  await expect
    .poll(async () => {
      try {
        return (await fetch(`${API}/health`)).ok;
      } catch {
        return false;
      }
    })
    .toBe(true);
});

test("FR-26: a scan on the server is typed into the page by the wedge", async ({ page }) => {
  await page.goto("/e2e/page.html");
  await page.waitForFunction(() => window.ready === true);
  await page.evaluate(async (port) => {
    document.body.insertAdjacentHTML("beforeend", '<textarea id="field"></textarea>');
    const installation = window.shim.install({
      mode: "server",
      url: `ws://127.0.0.1:${String(port)}/bridge`,
    });
    await installation.transport.ready;
    window.installation = installation as unknown as Window["installation"];
  }, PORT);
  await page.focus("#field");

  // From outside the page (the dashboard, CLI or a test), through the REST API...
  const response = await fetch(`${API}/devices/scanner-1/scan`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code: "4006381333931" }),
  });
  expect(response.ok).toBe(true);
  await expect(page.locator("#field")).toHaveValue("4006381333931\n");

  // ...and from the page itself, which calls the same API.
  await page.evaluate(() => window.installation.scan("scanner-1", "ABC", { terminator: "none" }));
  await expect(page.locator("#field")).toHaveValue("4006381333931\nABC");
});

test("FR-27: a tap on the server fires reading, and write() changes the server's card", async ({
  page,
}) => {
  await page.goto("/e2e/page.html");
  await page.waitForFunction(() => window.ready === true);
  await page.evaluate(async (port) => {
    const installation = window.shim.install({
      mode: "server",
      url: `ws://127.0.0.1:${String(port)}/bridge`,
    });
    await installation.transport.ready;
    window.readings = [];
    window.ndef = new NDEFReader();
    window.ndef.onreading = (event) => {
      const e = event as InstanceType<typeof NDEFReadingEvent>;
      window.readings.push({ serialNumber: e.serialNumber, records: [] });
    };
    await window.ndef.scan();
    // Waits for a card: the tap below.
    window.result = window.settle(window.ndef.write("Visit 2"));
  }, PORT);
  await fetch(`${API}/devices/nfc-1/tap`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ uid: "04a2b3c4" }),
  });
  await expect
    .poll(() => page.evaluate(() => window.readings))
    .toEqual([{ serialNumber: "04:a2:b3:c4", records: [] }]);
  expect(await page.evaluate(() => window.result)).toEqual({ ok: true });
  const devices = (await (await fetch(`${API}/devices`)).json()) as {
    id: string;
    state: { lastWritten: { records: { data: string }[] } };
  }[];
  expect(devices.find((d) => d.id === "nfc-1")?.state.lastWritten.records[0]?.data).toBe("Visit 2");
});
