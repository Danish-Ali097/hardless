import { describe, expect, it } from "vitest";
import { MATCHERS } from "../src/index.js";

describe("@hardless/playwright skeleton", () => {
  it("names every matcher from FR-41", () => {
    expect(MATCHERS).toHaveLength(6);
    expect(new Set(MATCHERS).size).toBe(MATCHERS.length);
  });
});
