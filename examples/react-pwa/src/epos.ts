import { money, total, type Sale } from "./escpos.js";

/**
 * A network receipt printer that speaks ePOS-Print XML over HTTP: the production code for
 * printers on the LAN, and the only way to print from Safari on iPhone and iPad. It knows
 * nothing about Hardless; in development its address points at the Hardless server.
 */

const NS = "http://www.epson-pos.com/schemas/2011/03/epos-print";

/** The ePOS-Print address of `devid` on the host the app was loaded from: HTTPS on 8443 when
 * the app is on HTTPS (a page on https:// may not call http://), HTTP on 8080 otherwise. */
export function defaultAddress(location: { protocol: string; hostname: string }): string {
  const secure = location.protocol === "https:";
  const base = secure ? `https://${location.hostname}:8443` : `http://${location.hostname}:8080`;
  return `${base}/cgi-bin/epos/service.cgi?devid=local_printer&timeout=10000`;
}

function escape(text: string): string {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll("\n", "&#10;");
}

function columns(left: string, right: string, width: number): string {
  const space = Math.max(1, width - left.length - right.length);
  return left + " ".repeat(space) + right;
}

/** The receipt for a sale as an ePOS-Print document, the same receipt as over Web Serial. */
export function eposReceipt(sale: Sale, width = 32): string {
  const lines = sale.lines
    .map((line) =>
      columns(`${String(line.quantity)} x ${line.name}`, money(line.quantity * line.price), width),
    )
    .join("\n");
  const body = [
    '<text align="center"/>',
    '<text dw="true" dh="true">CORNER CAFE&#10;</text>',
    '<text dw="false" dh="false">12 Market Street&#10;&#10;</text>',
    '<text align="left"/>',
    `<text>${escape(`${lines}\n${"-".repeat(width)}\n`)}</text>`,
    `<text em="true">${escape(`${columns("Total:", money(total(sale.lines)), width)}\n`)}</text>`,
    `<text em="false">${escape(`Paid by ${sale.payment}\n\n`)}</text>`,
    '<text align="center">Thank you!&#10;</text>',
    '<feed line="3"/>',
    sale.payment === "cash" ? '<pulse drawer="drawer_1" time="pulse_100"/>' : "",
    '<cut type="feed"/>',
  ].join("");
  return envelope(body);
}

/** Opens the cash drawer: a drawer-kick pulse on its own. */
export function eposDrawerKick(): string {
  return envelope('<pulse drawer="drawer_1" time="pulse_100"/>');
}

/** A SOAP envelope around an `epos-print` document. */
function envelope(body: string): string {
  return (
    '<?xml version="1.0" encoding="utf-8"?>' +
    '<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body>' +
    `<epos-print xmlns="${NS}">${body}</epos-print>` +
    "</s:Body></s:Envelope>"
  );
}

/** What an ePOS-Print error code means for the person at the till. */
function explain(code: string): string {
  switch (code) {
    case "EPTR_REC_EMPTY":
      return "The receipt did not print: the printer is out of paper.";
    case "EPTR_COVER_OPEN":
      return "The receipt did not print: the printer cover is open.";
    case "EX_TIMEOUT":
      return "The printer did not answer in time.";
    case "DeviceNotFound":
      return "No printer at that address.";
    default:
      return `The receipt did not print (${code || "unknown error"}).`;
  }
}

/** Sends a document; resolves when it printed, rejects with a message the app can show. */
export async function printOverNetwork(address: string, document: string): Promise<void> {
  let response: Response;
  try {
    response = await fetch(address, {
      method: "POST",
      headers: {
        "Content-Type": "text/xml; charset=utf-8",
        "If-Modified-Since": "Thu, 01 Jan 1970 00:00:00 GMT",
        SOAPAction: '""',
      },
      body: document,
    });
  } catch {
    // The browser hides why: offline printer, mixed content (http:// from an https:// page),
    // an untrusted certificate or blocked CORS all look the same here.
    throw new Error(
      "Could not reach the printer. Check its address; from an https:// page use its https:// address and trust its certificate.",
    );
  }
  const xml = new DOMParser().parseFromString(await response.text(), "text/xml");
  const result = xml.getElementsByTagNameNS(NS, "response")[0];
  if (!result) throw new Error(`The printer answered ${String(response.status)} without a result.`);
  if (result.getAttribute("success") !== "true") {
    throw new Error(explain(result.getAttribute("code") ?? ""));
  }
}
