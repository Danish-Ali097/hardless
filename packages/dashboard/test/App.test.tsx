import { readFileSync } from "node:fs";
import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Dashboard } from "../src/App.js";
import { drawer, job, printer, scanner, stateWith } from "./fixtures.js";

/** Server-rendered markup without React's text separators (`<!-- -->`). */
const html = (state: Parameters<typeof Dashboard>[0]["state"]) =>
  renderToString(<Dashboard state={state} />).replaceAll("<!-- -->", "");

describe("dashboard", () => {
  it("FR-23: lists every device with its status and active faults", () => {
    const page = html(
      stateWith([
        printer({ faults: [{ id: "paper-out", options: {} }] }, { paperOut: true }),
        drawer({ state: "open" }),
      ]),
    );
    expect(page).toContain("Receipt printer, paper out");
    expect(page).toContain("1 fault");
    expect(page).toContain("Cash drawer, open");
    expect(html(stateWith([printer({}, { paperNearEnd: true })]))).toContain(
      "Receipt printer, paper low",
    );
    expect(page).toContain("Add device");
  });

  it("FR-24: shows the printer's tray with width switch, clear and an empty state", () => {
    const page = html(stateWith([printer()]));
    expect(page).toContain("Nothing printed yet.");
    expect(page).toMatch(/role="radio" aria-checked="true"[^>]*>80 mm/);
    expect(page).toContain(">Clear<");
    expect(page).toContain("Mark unknown commands");
  });

  it("FR-24: lists receipts newest first with raw bytes and PNG and HTML downloads", () => {
    const page = html(stateWith([printer()], { jobs: { "printer-1": [job(2), job(1)] } }));
    expect(page.indexOf("Receipt 2:")).toBeLessThan(page.indexOf("Receipt 1:"));
    expect(page).toContain("Raw bytes");
    expect(page).toContain("1b 40");
    expect(page).toContain("jobs/2/render?format=html&amp;width=80");
    expect(page).toContain('render?format=png&amp;width=80" download="receipt-2.png"');
    expect(page).toContain("Download PNG");
  });

  it("FR-26: the scanner panel has a scan box, presets, a sequence runner and its faults", () => {
    const page = html(stateWith([scanner({ scans: 3, lastCode: "4006381333931" })]));
    expect(page).toContain(
      "Keyboard-wedge barcode scanner, Enter after each code, 8 ms between keys",
    );
    expect(page).toContain("3 scans, last");
    expect(page).toContain('id="scan-code"');
    expect(page).toMatch(/<button type="submit" disabled=""[^>]*>Scan<\/button>/);
    expect(page).toContain("EAN-13 sample");
    expect(page).toContain("Sequence");
    expect(page).toContain("Double scan");
    expect(page).toContain("Barcode scanner, ready");
  });

  it("FR-25: shows the attached drawer opening, its kick count and a close button", () => {
    const open = html(stateWith([printer(), drawer({ state: "open", kicks: 3 })]));
    expect(open).toContain(">Open<");
    expect(open).toContain("kicked 3 times");
    expect(open).toMatch(/<button type="button"[^>]*>Close drawer<\/button>/);
    expect(open).toContain("translate-y-6");

    const closed = html(stateWith([printer(), drawer()]));
    expect(closed).toMatch(/<button type="button" disabled=""[^>]*>Close drawer<\/button>/);
  });

  it("FR-28: offers a switch for every supported fault, on for active ones", () => {
    const page = html(
      stateWith([
        printer({ faults: [{ id: "slow-write", options: { msPerChunk: 500 } }] }),
        drawer(),
      ]),
    );
    expect(page.match(/role="switch"/g)?.length).toBe(6 + 2);
    expect(page).toMatch(/aria-checked="true" aria-label="Slow writes"/);
    expect(page).toMatch(/aria-checked="false" aria-label="Paper out"/);
    expect(page).toContain('value="500"');
    expect(page).toContain("Jammed");
  });

  it("NFR-13: defines light and dark themes", () => {
    const css = readFileSync(new URL("../src/index.css", import.meta.url), "utf8");
    expect(css).toContain("@media (prefers-color-scheme: dark)");
    expect(css).toContain("prefers-reduced-motion");
  });

  it("explains how to start when there are no devices", () => {
    expect(html(stateWith([]))).toContain("No devices yet.");
  });
});
