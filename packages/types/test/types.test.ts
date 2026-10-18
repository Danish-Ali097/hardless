import { describe, expectTypeOf, it } from "vitest";
import type { BusEvent, ControlCommand, EventRecord, Job } from "../src/index.js";

describe("@hardless/types", () => {
  it("FR-02: bus events are discriminated by their wire type", () => {
    expectTypeOf<BusEvent["type"]>().toEqualTypeOf<
      "device:added" | "device:removed" | "device:state" | "device:output"
    >();
    expectTypeOf<Extract<BusEvent, { type: "device:output" }>["job"]>().toEqualTypeOf<Job>();
    const event: BusEvent = {
      type: "device:output",
      deviceId: "printer-1",
      job: { id: 1, at: 300, ast: {}, text: "Total: 1,250\n", bytesHex: "0a" },
    };
    expectTypeOf(event).toExtend<BusEvent>();
  });

  it("FR-03: event records carry seq, at, deviceId and a typed kind", () => {
    const record: EventRecord = {
      seq: 2,
      at: 3,
      deviceId: "p1",
      type: "bytes-in",
      dataHex: "1b40",
    };
    expectTypeOf(record.seq).toBeNumber();
    expectTypeOf(record.at).toBeNumber();
  });

  it("FR-01: control commands match the REST payloads", () => {
    const scan: ControlCommand = { type: "scan", code: "8964000123456", delayMs: 8 };
    expectTypeOf(scan).toExtend<ControlCommand>();
  });
});
