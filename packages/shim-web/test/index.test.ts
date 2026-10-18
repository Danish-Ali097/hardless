import type { DeviceSummary } from "@hardless/types";
import { describe, expect, it } from "vitest";
import { SHIM_APIS, SHIM_VERSION } from "../src/index.js";
import { matchesFilter, validateFilters, validateOptions } from "../src/serial.js";
import { normalizeDevice } from "../src/standalone-transport.js";
import { fromBase64, toBase64 } from "../src/transport.js";

const device = (state: object): DeviceSummary => ({
  id: "printer-1",
  kind: "printer-escpos",
  state: { transport: "serial", connected: true, ...state },
  faults: [],
  supportedFaults: [],
  attachedTo: null,
});

describe("@hardless/shim-web", () => {
  it("lists the APIs from the bridge hello message", () => {
    expect(SHIM_APIS).toEqual(["serial", "usb", "nfc", "wedge"]);
    expect(SHIM_VERSION).toMatch(/^\d+\.\d+\.\d+/);
  });

  it("FR-16: open() options are validated like the spec", () => {
    expect(validateOptions({ baudRate: 9600 })).toEqual({
      baudRate: 9600,
      dataBits: 8,
      stopBits: 1,
      parity: "none",
      bufferSize: 255,
      flowControl: "none",
    });
    expect(() => validateOptions(undefined)).toThrow(TypeError);
    expect(() => validateOptions({})).toThrow(/baudRate/);
    expect(() => validateOptions({ baudRate: 0 })).toThrow(TypeError);
    expect(() => validateOptions({ baudRate: Number.NaN })).toThrow(TypeError);
    expect(() => validateOptions({ baudRate: -1 })).toThrow(TypeError);
    expect(() => validateOptions({ baudRate: 9600, dataBits: 6 })).toThrow(TypeError);
    expect(() => validateOptions({ baudRate: 9600, stopBits: 3 })).toThrow(TypeError);
    expect(() => validateOptions({ baudRate: 9600, parity: "mark" })).toThrow(TypeError);
    expect(() => validateOptions({ baudRate: 9600, bufferSize: 0 })).toThrow(TypeError);
    expect(() => validateOptions({ baudRate: 9600, flowControl: "xon" })).toThrow(TypeError);
    expect(validateOptions({ baudRate: 115200.9 }).baudRate).toBe(115200);
  });

  it("FR-16: requestPort() filters are validated like the spec", () => {
    expect(validateFilters(undefined)).toBeUndefined();
    expect(validateFilters([{ usbVendorId: 0x1209 }])).toHaveLength(1);
    expect(() => validateFilters([{}])).toThrow(TypeError);
    expect(() => validateFilters([{ usbProductId: 1 }])).toThrow(TypeError);
    expect(() => validateFilters([{ bluetoothServiceClassId: 1, usbVendorId: 2 }])).toThrow(
      TypeError,
    );
  });

  it("FR-16: filters match on USB vendor and product IDs", () => {
    const printer = device({ usbVendorId: 0x1209, usbProductId: 1 });
    expect(matchesFilter(printer, { usbVendorId: 0x1209 })).toBe(true);
    expect(matchesFilter(printer, { usbVendorId: 0x1209, usbProductId: 1 })).toBe(true);
    expect(matchesFilter(printer, { usbVendorId: 0x1209, usbProductId: 2 })).toBe(false);
    expect(matchesFilter(printer, { usbVendorId: 0x04b8 })).toBe(false);
    expect(matchesFilter(device({}), { usbVendorId: 0x1209 })).toBe(false);
    expect(matchesFilter(printer, { bluetoothServiceClassId: 0x1101 })).toBe(false);
  });

  it("FR-15: standalone devices accept the fixture shape and the config-file shape", () => {
    expect(
      normalizeDevice({
        kind: "printer-escpos",
        id: "printer-1",
        transport: "serial",
        autoSelect: true,
      }),
    ).toEqual({
      kind: "printer-escpos",
      id: "printer-1",
      config: { transport: "serial", autoSelect: true },
    });
    expect(normalizeDevice({ kind: "cash-drawer", config: { attachedTo: "printer-1" } })).toEqual({
      kind: "cash-drawer",
      config: { attachedTo: "printer-1" },
    });
  });

  it("encodes bridge payloads as base64", () => {
    const bytes = new Uint8Array([0x1b, 0x40, 0x00, 0xff]);
    expect(toBase64(bytes)).toBe("G0AA/w==");
    expect(fromBase64("G0AA/w==")).toEqual(bytes);
  });
});
