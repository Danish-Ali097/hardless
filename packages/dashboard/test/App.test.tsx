import { readFileSync } from "node:fs";
import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Dashboard } from "../src/App.js";
import { EventLogView } from "../src/components/EventLog.js";
import { drawer, job, nfcReader, printer, scanner, stateWith } from "./fixtures.js";

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

  it("FR-27: the NFC panel lists cards, shows the last write and has a card editor", () => {
    const page = html(
      stateWith([
        nfcReader({
          card: "04:a2:b3:c4",
          taps: 2,
          cards: [
            {
              uid: "04:a2:b3:c4",
              records: [{ recordType: "text", data: "Visit 2", lang: "en" }],
              readOnly: true,
            },
          ],
          lastWritten: { uid: "04:a2:b3:c4", records: [{ recordType: "text", data: "Visit 2" }] },
        }),
      ]),
    );
    expect(page).toContain("NFC reader, a tapped card stays on it for 2 s");
    expect(page).toContain("2 taps, card");
    expect(page).toContain("on the reader");
    expect(page).toContain("read-only");
    expect(page).toContain("Text &quot;Visit 2&quot; (en)");
    expect(page).toContain("Last written by the app to");
    expect(page).toContain("Tap this card");
    expect(page).toContain("Add record");
    expect(page).toContain("Read error");
    expect(page).toContain("NFC reader, card on the reader");
    expect(html(stateWith([nfcReader()]))).toContain("No cards yet.");
  });

  it("FR-29: the event log view lists records newest first with filters", () => {
    const page = html(stateWith([printer(), scanner()], { view: "log" }));
    expect(page).toContain('aria-pressed="true"');
    expect(page).toContain("Event log");
    expect(page).toContain("Nothing logged yet.");

    const log = renderToString(
      <EventLogView
        devices={[printer(), scanner()]}
        records={[
          { seq: 1, at: 10, deviceId: "printer-1", type: "bytes-in", dataHex: "1b40" },
          { seq: 2, at: 20, deviceId: "printer-1", type: "bytes-out", dataHex: "12" },
        ]}
        filter={{ device: null, direction: null }}
        onFilter={() => undefined}
      />,
    ).replaceAll("<!-- -->", "");
    expect(log.indexOf("Device replied with 1 byte")).toBeLessThan(log.indexOf("App sent 2 bytes"));
    expect(log).toContain('aria-label="app to device"');
    expect(log).toContain("All devices");
    expect(log).toContain("Device to app");
    expect(log).toContain("0.010 s");
  });

  it("FR-33: an HTTP printer shows its addresses, the mixed-content help and the certificate", () => {
    const http = printer({ id: "local_printer" }, { transport: "http" });
    const info = {
      version: "0.0.0",
      httpPort: 8080,
      https: { enabled: true, port: 8443, certificate: "/hardless-ca.crt", hosts: ["localhost"] },
    };
    const page = html(stateWith([http], { info }));
    expect(page).toContain("HTTP receipt printer");
    expect(page).toContain(
      "https://localhost:8443/cgi-bin/epos/service.cgi?devid=local_printer&amp;timeout=10000",
    );
    expect(page).toContain("https://localhost:8443/StarWebPRNT/SendMessage");
    expect(page).toContain("Mixed content.");
    expect(page).toContain('href="http://localhost:8080/hardless-ca.crt"');
    expect(page).toContain("Certificate Trust Settings");

    const off = html(
      stateWith([http], {
        info: {
          ...info,
          https: { enabled: false, hosts: [], error: "cannot listen on port 8443" },
        },
      }),
    );
    expect(off).toContain("http://localhost:8080/cgi-bin/epos/service.cgi");
    expect(off).toContain("HTTPS is off: cannot listen on port 8443.");
    expect(html(stateWith([printer()]))).not.toContain("Printing over HTTP");
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
