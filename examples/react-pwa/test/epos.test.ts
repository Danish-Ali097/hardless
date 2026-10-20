import { describe, expect, it } from "vitest";
import { defaultAddress, eposDrawerKick, eposReceipt } from "../src/epos.js";

describe("network printer (ePOS-Print)", () => {
  const lines = [{ name: "Flat white", quantity: 2, price: 450 }];

  it("prints over HTTPS from an https:// page and HTTP otherwise", () => {
    expect(defaultAddress({ protocol: "https:", hostname: "192.168.1.20" })).toBe(
      "https://192.168.1.20:8443/cgi-bin/epos/service.cgi?devid=local_printer&timeout=10000",
    );
    expect(defaultAddress({ protocol: "http:", hostname: "localhost" })).toBe(
      "http://localhost:8080/cgi-bin/epos/service.cgi?devid=local_printer&timeout=10000",
    );
  });

  it("builds the same receipt as over Web Serial, with a drawer kick for cash", () => {
    const cash = eposReceipt({ lines, payment: "cash" });
    expect(cash).toContain(
      '<epos-print xmlns="http://www.epson-pos.com/schemas/2011/03/epos-print">',
    );
    expect(cash).toContain("CORNER CAFE&#10;");
    expect(cash).toContain("2 x Flat white");
    expect(cash).toContain('<pulse drawer="drawer_1" time="pulse_100"/>');
    expect(cash).toContain('<cut type="feed"/>');
    expect(eposReceipt({ lines, payment: "card" })).not.toContain("<pulse");
  });

  it("escapes receipt text", () => {
    const xml = eposReceipt({
      lines: [{ name: "Tea & <cake>", quantity: 1, price: 1 }],
      payment: "card",
    });
    expect(xml).toContain("Tea &amp; &lt;cake&gt;");
  });

  it("opens the drawer with a pulse alone", () => {
    expect(eposDrawerKick()).toMatch(
      /<epos-print[^>]*><pulse drawer="drawer_1" time="pulse_100"\/><\/epos-print>/,
    );
  });
});
