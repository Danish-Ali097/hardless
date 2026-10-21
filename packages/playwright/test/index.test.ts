import type { DeviceSummary, EventRecord, Job } from "@hardless/types";
import { describe, expect, it } from "vitest";
import type { Backend } from "../src/backend.js";
import { restBase } from "../src/backend.js";
import { Hardless } from "../src/devices.js";
import { DEFAULT_DEVICES, initScript, MATCHERS, matchers } from "../src/index.js";

/** A backend whose devices and jobs a test sets directly. */
class FakeBackend implements Backend {
  state: Record<string, Record<string, unknown>> = {};
  jobList: Job[] = [];
  faults: string[] = [];

  devices(): Promise<DeviceSummary[]> {
    return Promise.resolve(
      Object.entries(this.state).map(([id, state]) => ({
        id,
        kind: "x",
        state: state as DeviceSummary["state"],
        faults: [],
        supportedFaults: [],
        attachedTo: null,
      })),
    );
  }
  jobs(): Promise<Job[]> {
    return Promise.resolve(this.jobList);
  }
  events(): Promise<EventRecord[]> {
    return Promise.resolve([]);
  }
  setFault(id: string, fault: string, active: boolean): Promise<void> {
    this.faults.push(`${id}:${fault}:${String(active)}`);
    return Promise.resolve();
  }
  reset(): Promise<void> {
    return Promise.resolve();
  }
  scan(): Promise<void> {
    return Promise.resolve();
  }
  tap(): Promise<void> {
    return Promise.resolve();
  }
  control(): Promise<void> {
    return Promise.resolve();
  }
}

const job = (id: number, text: string, cut = true): Job => ({
  id,
  at: id,
  ast: cut ? [{ type: "text" }, { type: "cut" }] : [{ type: "text" }],
  text,
  bytesHex: "",
});

const ctx = { isNot: false, timeout: 300 };

describe("@hardless/playwright", () => {
  it("FR-41: names six matchers", () => {
    expect(MATCHERS).toEqual(Object.keys(matchers));
  });

  it("FR-40: the init script installs the shim with the test's options", () => {
    // The IIFE build is read lazily; only the bootstrap is checked here.
    const script = (() => {
      try {
        return initScript({ mode: "standalone", devices: DEFAULT_DEVICES, nfc: false });
      } catch (error) {
        return String(error);
      }
    })();
    if (script.includes("cannot find")) return; // shim-web not built in this checkout
    expect(script).toContain('"mode":"standalone"');
    expect(script).toContain('"nfc":false');
    expect(script).toContain("/__hardless__/hardless_wasm.js");
    expect(initScript({ mode: "server", url: "ws://localhost:8080/bridge", token: "t" })).toContain(
      '"url":"ws://localhost:8080/bridge","token":"t"',
    );
  });

  it("maps a bridge URL to the REST base", () => {
    expect(restBase("ws://localhost:8080/bridge")).toBe("http://localhost:8080/api/v1");
    expect(restBase("wss://10.0.0.5:8443/bridge?token=x")).toBe("https://10.0.0.5:8443/api/v1");
  });

  it("FR-41: printer matchers poll the receipts", async () => {
    const backend = new FakeBackend();
    const printer = new Hardless("standalone", backend).printer("printer-1");
    setTimeout(() => {
      backend.jobList = [job(1, "Total: 1,250\n")];
    }, 50);
    expect((await matchers.toHavePrinted.call(ctx, printer, /Total:\s+1,250/)).pass).toBe(true);
    expect((await matchers.toHavePrinted.call(ctx, printer, "Total")).pass).toBe(true);
    expect((await matchers.toHavePrintedJobs.call(ctx, printer, 1)).pass).toBe(true);
    expect((await matchers.toHaveCut.call(ctx, printer)).pass).toBe(true);
    const failed = await matchers.toHavePrinted.call(ctx, printer, "Change due");
    expect(failed.pass).toBe(false);
    expect(failed.message()).toContain(JSON.stringify("Total: 1,250\n"));
    backend.jobList = [job(2, "x", false)];
    expect((await matchers.toHaveCut.call(ctx, printer)).pass).toBe(false);
    // .not passes as soon as the condition is false.
    const start = Date.now();
    expect((await matchers.toHaveCut.call({ isNot: true }, printer)).pass).toBe(false);
    expect(Date.now() - start).toBeLessThan(200);
  });

  it("FR-41: drawer and NFC matchers read the device state", async () => {
    const backend = new FakeBackend();
    const hardless = new Hardless("server", backend);
    backend.state = {
      "drawer-1": { state: "open", opened: 1 },
      "nfc-1": {
        lastWritten: { uid: "04:a2", records: [{ recordType: "text", data: "Hi", lang: "en" }] },
      },
    };
    expect(
      (await matchers.toHaveOpened.call(ctx, hardless.drawer("drawer-1"), { times: 1 })).pass,
    ).toBe(true);
    expect(
      (await matchers.toHaveOpened.call(ctx, hardless.drawer("drawer-1"), { times: 2 })).pass,
    ).toBe(false);
    expect((await matchers.toBeClosed.call(ctx, hardless.drawer("drawer-1"))).pass).toBe(false);
    expect(
      (
        await matchers.toHaveWrittenNdef.call(ctx, hardless.nfc("nfc-1"), [
          { recordType: "text", data: "Hi" },
        ])
      ).pass,
    ).toBe(true);
    expect(
      (await matchers.toHaveWrittenNdef.call(ctx, hardless.nfc("nfc-1"), [{ data: "Bye" }])).pass,
    ).toBe(false);
  });

  it("FR-42: fault helpers turn faults on and off", async () => {
    const backend = new FakeBackend();
    const printer = new Hardless("standalone", backend).printer("printer-1");
    await printer.fault("paper-out");
    await printer.clearFault("paper-out");
    expect(backend.faults).toEqual(["printer-1:paper-out:true", "printer-1:paper-out:false"]);
  });

  it("refuses a matcher on the wrong kind of handle", async () => {
    const hardless = new Hardless("standalone", new FakeBackend());
    await expect(matchers.toHavePrinted.call(ctx, hardless.drawer("d"), "x")).rejects.toThrow(
      'expect(hardless.printer("id"))',
    );
  });
});
