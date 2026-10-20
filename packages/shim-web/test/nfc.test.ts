import { describe, expect, it } from "vitest";
import { toWireMessage, toWireRecord } from "../src/nfc.js";

describe("NDEF records (PRD section 8.3)", () => {
  it("FR-16: turns NDEFRecordInit into wire records", () => {
    expect(toWireRecord({ recordType: "text", data: "Hi" }, "fr")).toEqual({
      recordType: "text",
      data: "Hi",
      lang: "fr",
    });
    expect(
      toWireRecord({ recordType: "mime", mediaType: "image/png", data: new Uint8Array([137, 80]) }),
    ).toEqual({ recordType: "mime", mediaType: "image/png", dataBase64: "iVA=" });
    expect(toWireRecord({ recordType: "example.com:loyalty", data: new ArrayBuffer(1) })).toEqual({
      recordType: "example.com:loyalty",
      dataBase64: "AA==",
    });
    expect(toWireRecord({ recordType: "empty", id: "e" })).toEqual({
      recordType: "empty",
      id: "e",
    });
  });

  it("FR-16: rejects what Web NFC rejects", () => {
    const bad = [
      undefined,
      {},
      { recordType: "url" },
      { recordType: "url", data: new Uint8Array() },
      { recordType: "mime", data: "text is not allowed" },
      { recordType: "empty", data: "x" },
      { recordType: "text", data: "x", encoding: "utf-16" },
      { recordType: "text", data: new Uint8Array(), encoding: "latin1" },
      { recordType: ":local" },
      { recordType: "nonsense" },
    ];
    for (const init of bad)
      expect(() => toWireRecord(init), JSON.stringify(init)).toThrow(TypeError);
    expect(() => toWireRecord({ recordType: "smart-poster", data: { records: [] } })).toThrow(
      expect.objectContaining({ name: "NotSupportedError" }),
    );
  });

  it("FR-16: a string message is a text record, bytes are a mime record", () => {
    expect(toWireMessage("Hello")).toEqual([{ recordType: "text", data: "Hello", lang: "en" }]);
    expect(toWireMessage(new Uint8Array([1]))).toEqual([
      { recordType: "mime", mediaType: "application/octet-stream", dataBase64: "AQ==" },
    ]);
    expect(() => toWireMessage({ records: [] })).toThrow(TypeError);
    expect(() => toWireMessage(42)).toThrow(TypeError);
  });
});
