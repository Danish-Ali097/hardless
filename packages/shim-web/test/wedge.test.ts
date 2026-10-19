import { describe, expect, it } from "vitest";
import { charKey, scanKeys, terminatorKey } from "../src/wedge.js";

describe("keyboard wedge keys (PRD section 8.4)", () => {
  it("maps letters, digits and symbols to US-layout keys with legacy key codes", () => {
    expect(charKey("a")).toMatchObject({ key: "a", code: "KeyA", shift: false, keyCode: 65 });
    expect(charKey("A")).toMatchObject({ key: "A", code: "KeyA", shift: true, keyCode: 65 });
    expect(charKey("7")).toMatchObject({ code: "Digit7", shift: false, keyCode: 55, charCode: 55 });
    expect(charKey("#")).toMatchObject({ code: "Digit3", shift: true, keyCode: 51 });
    expect(charKey("-")).toMatchObject({ code: "Minus", shift: false, keyCode: 189 });
    expect(charKey("_")).toMatchObject({ code: "Minus", shift: true, keyCode: 189 });
    expect(charKey(" ")).toMatchObject({ code: "Space", keyCode: 32 });
    expect(charKey("é")).toMatchObject({ key: "é", code: "", keyCode: 0, text: "é" });
  });

  it("ends a scan with Enter or Tab, or nothing", () => {
    expect(terminatorKey("enter")).toMatchObject({ key: "Enter", keyCode: 13, charCode: 13 });
    expect(terminatorKey("tab")).toMatchObject({ key: "Tab", keyCode: 9, charCode: 0 });
    const keys = (terminator: "enter" | "tab" | "none") =>
      scanKeys({ code: "12", delayMs: 8, terminator }).map((k) => k.key);
    expect(keys("enter")).toEqual(["1", "2", "Enter"]);
    expect(keys("tab")).toEqual(["1", "2", "Tab"]);
    expect(keys("none")).toEqual(["1", "2"]);
  });

  it("presses one key per code point, like the engine counts them", () => {
    expect(scanKeys({ code: "a😀", delayMs: 0, terminator: "none" })).toHaveLength(2);
  });
});
