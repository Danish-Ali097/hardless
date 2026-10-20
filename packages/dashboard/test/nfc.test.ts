import { describe, expect, it } from "vitest";
import {
  blankDraft,
  cardsOf,
  describeRecord,
  draftProblem,
  fromDrafts,
  lastWrittenOf,
  toDrafts,
} from "../src/nfc.js";
import { nfcReader } from "./fixtures.js";

describe("NFC panel helpers (FR-27)", () => {
  it("reads the reader's cards, newest first, and its last write", () => {
    const reader = nfcReader({
      cards: [
        { uid: "04:01", records: [], readOnly: false },
        { uid: "04:02", records: [{ recordType: "text", data: "Hi" }], readOnly: true },
      ],
      lastWritten: { uid: "04:02", records: [{ recordType: "text", data: "Hi" }] },
    });
    expect(cardsOf(reader).map((c) => [c.uid, c.readOnly])).toEqual([
      ["04:02", true],
      ["04:01", false],
    ]);
    expect(lastWrittenOf(reader)?.uid).toBe("04:02");
    expect(cardsOf(nfcReader({ cards: "nonsense" }))).toEqual([]);
  });

  it("describes records in a few words", () => {
    expect(describeRecord({ recordType: "text", data: "Member 1234", lang: "en" })).toBe(
      'Text "Member 1234" (en)',
    );
    expect(describeRecord({ recordType: "url", data: "https://example.com/" })).toBe(
      "URL https://example.com/",
    );
    expect(
      describeRecord({ recordType: "mime", mediaType: "image/png", dataBase64: "iVBORw==" }),
    ).toBe("image/png: 4 bytes");
    expect(describeRecord({ recordType: "empty" })).toBe("Empty record");
  });

  it("round-trips records through the editor, keeping binary ones", () => {
    const records = [
      { recordType: "text", data: "Hi", lang: "fr" },
      { recordType: "url", data: "https://example.com/" },
      { recordType: "mime", mediaType: "application/json", data: "{}" },
      { recordType: "mime", mediaType: "image/png", dataBase64: "iVBORw==" },
      { recordType: "empty" },
    ];
    const drafts = toDrafts(records);
    expect(drafts[3]?.kept).toEqual(records[3]);
    expect(fromDrafts(drafts)).toEqual(records);
  });

  it("checks the UID and URL records before a tap", () => {
    expect(draftProblem("04a2b3c4", [blankDraft()])).toBeNull();
    expect(draftProblem("04:a2:b3:c4", [])).toBeNull();
    expect(draftProblem("xyz", [])).toMatch(/hex/);
    expect(draftProblem("04a", [])).toMatch(/hex/);
    expect(draftProblem("04a2", [{ ...blankDraft(), type: "url", data: " " }])).toMatch(/URL/);
  });
});
