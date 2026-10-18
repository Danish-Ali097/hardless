import { describe, expect, it } from "vitest";
import { SHIM_APIS, SHIM_VERSION } from "../src/index.js";

describe("@hardless/shim-web skeleton", () => {
  it("lists the APIs from the bridge hello message", () => {
    expect(SHIM_APIS).toEqual(["serial", "usb", "nfc", "wedge"]);
  });

  it("has a semver version", () => {
    expect(SHIM_VERSION).toMatch(/^\d+\.\d+\.\d+/);
  });
});
