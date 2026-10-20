import type { EventRecord } from "@hardless/types";
import { describe as group, expect, it } from "vitest";
import {
  bytesPreview,
  describe,
  directionOf,
  formatAt,
  matches,
  MAX_LOG_RECORDS,
  mergeRecords,
} from "../src/events.js";

const at = { at: 1500, deviceId: "printer-1" };

const records: EventRecord[] = [
  { seq: 1, ...at, type: "bytes-in", dataHex: "1b400a" },
  { seq: 2, ...at, type: "bytes-out", dataHex: "12" },
  { seq: 3, ...at, type: "state-changed", state: { online: true } },
  {
    seq: 4,
    ...at,
    deviceId: "scanner-1",
    type: "wedge",
    scan: { code: "123", delayMs: 8, terminator: "enter" },
  },
  {
    seq: 5,
    ...at,
    deviceId: "nfc-1",
    type: "nfc",
    event: { type: "reading", serialNumber: "04:a2", records: [] },
  },
  {
    seq: 6,
    ...at,
    deviceId: "nfc-1",
    type: "control",
    command: { type: "nfc-write", records: [{ recordType: "text", data: "x" }], overwrite: true },
  },
  {
    seq: 7,
    ...at,
    deviceId: "nfc-1",
    type: "nfc",
    event: { type: "written", serialNumber: "04:a2", records: [] },
  },
];

group("event log (FR-29)", () => {
  it("gives bytes, keystrokes and NFC traffic a direction, like the server", () => {
    expect(records.map(directionOf)).toEqual([
      "host-to-device",
      "device-to-host",
      null,
      "device-to-host",
      "device-to-host",
      "host-to-device",
      null,
    ]);
  });

  it("filters by device and by direction", () => {
    const seqs = (device: string | null, direction: "host-to-device" | "device-to-host" | null) =>
      records.filter((r) => matches(r, { device, direction })).map((r) => r.seq);
    expect(seqs(null, null)).toEqual([1, 2, 3, 4, 5, 6, 7]);
    expect(seqs("printer-1", null)).toEqual([1, 2, 3]);
    expect(seqs(null, "device-to-host")).toEqual([2, 4, 5]);
    expect(seqs("nfc-1", "host-to-device")).toEqual([6]);
  });

  it("merges polled records in seq order without duplicates, keeping the newest", () => {
    const merged = mergeRecords(records.slice(0, 3), [
      records[2],
      records[4],
      records[3],
    ] as EventRecord[]);
    expect(merged.map((r) => r.seq)).toEqual([1, 2, 3, 4, 5]);
    const many = Array.from(
      { length: MAX_LOG_RECORDS + 5 },
      (_, i) => ({ ...records[0], seq: i + 1 }) as EventRecord,
    );
    const capped = mergeRecords([], many);
    expect(capped).toHaveLength(MAX_LOG_RECORDS);
    expect(capped[0]?.seq).toBe(6);
  });

  it("describes records in plain words", () => {
    expect(records.map((r) => describe(r).title)).toEqual([
      "App sent 3 bytes",
      "Device replied with 1 byte",
      "State changed",
      "Typed 123 + Enter",
      "Card 04:a2 read, 0 records",
      'App writes 1 record: Text "x"',
      "Card 04:a2 written",
    ]);
    expect(describe(records[0] as EventRecord).detail).toBe("1b 40 0a  ·@·");
    expect(bytesPreview("41".repeat(30), 4)).toEqual({
      count: 30,
      hex: "41 41 41 41 …",
      text: "AAAA …",
    });
    expect(formatAt(12345)).toBe("12.345 s");
  });
});
