import { describe, expect, it } from "vitest";
import { cardText, visits } from "../src/loyalty.js";

const text = (data: string, encoding: string | null = "utf-8") => ({
  recordType: "text",
  data: new DataView(new TextEncoder().encode(data).buffer),
  encoding,
});

describe("loyalty card", () => {
  it("reads the first text record", () => {
    expect(cardText([{ recordType: "url", data: null, encoding: null }, text("Visits: 4")])).toBe(
      "Visits: 4",
    );
    expect(cardText([])).toBeNull();
  });

  it("counts visits", () => {
    expect(visits("Visits: 12")).toBe(12);
    expect(visits("Member")).toBe(0);
    expect(visits(null)).toBe(0);
  });
});
