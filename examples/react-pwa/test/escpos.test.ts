import { describe, expect, it } from "vitest";
import { decodeStatus, money, receipt, total } from "../src/escpos.js";

describe("receipt builder", () => {
  const lines = [
    { name: "Flat white", quantity: 2, price: 450 },
    { name: "Croissant", quantity: 1, price: 350 },
  ];

  it("totals the order and formats money", () => {
    expect(total(lines)).toBe(1250);
    expect(money(1250)).toBe("1,250");
  });

  it("kicks the drawer only for cash sales, and ends with a cut", () => {
    const cash = Array.from(receipt({ lines, payment: "cash" }));
    const card = Array.from(receipt({ lines, payment: "card" }));
    const kick = [0x1b, 0x70, 0, 25, 250];
    const has = (bytes: number[], seq: number[]) =>
      bytes.some((_, i) => seq.every((b, j) => bytes[i + j] === b));
    expect(has(cash, kick)).toBe(true);
    expect(has(card, kick)).toBe(false);
    expect(cash.slice(-4)).toEqual([0x1d, 0x56, 0x42, 0]);
    expect(new TextDecoder().decode(Uint8Array.from(cash))).toContain("Total:");
  });

  it("decodes DLE EOT status bytes", () => {
    expect(decodeStatus(0x12, 0x12, 0x12)).toEqual({
      online: true,
      drawerOpen: false,
      coverOpen: false,
      paperOut: false,
    });
    expect(decodeStatus(0x1e, 0x32, 0x7e)).toEqual({
      online: false,
      drawerOpen: true,
      coverOpen: false,
      paperOut: true,
    });
  });
});
