import { describe, expect, it } from "vitest";
import { loadPresets, SAMPLE_PRESETS, sequenceCodes } from "../src/presets.js";

describe("scanner presets and sequences (FR-26)", () => {
  it("splits a sequence into codes, one per line", () => {
    expect(sequenceCodes(" 1 \r\n\n2\n  \n3")).toEqual(["1", "2", "3"]);
    expect(sequenceCodes("")).toEqual([]);
  });

  it("falls back to the sample presets when storage is unavailable", () => {
    expect(loadPresets()).toEqual(SAMPLE_PRESETS);
  });
});
