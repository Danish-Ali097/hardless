# Quickstart

Hardless emulates POS hardware, so a web app or PWA can print receipts, open a cash drawer,
read barcodes and NFC cards without the devices on your desk. Your app's hardware code stays
the same: Hardless replaces the browser APIs it uses (Web Serial, WebUSB, Web NFC, the
keyboard) or answers its HTTP printer calls.

## 1. Start Hardless

Until v0.1.0 is published, build it from the repository (Rust 1.99, Node.js 22, pnpm):

```sh
git clone https://github.com/hardlessdev/hardless
cd hardless
pnpm install
pnpm --filter @hardless/dashboard build
cargo run -p hardless-cli -- start
```

Or with Docker, once you have built the image (`Dockerfile`):

```sh
docker run -p 8080:8080 -p 8443:8443 -v hardless-data:/data hardless/hardless
```

Open the dashboard at <http://localhost:8080>. Without a config file you get one of each
device:

| Device          | ID              | How an app reaches it                               |
| --------------- | --------------- | --------------------------------------------------- |
| Receipt printer | `printer-1`     | Web Serial                                          |
| Cash drawer     | `drawer-1`      | kicked through `printer-1`                          |
| Receipt printer | `usb-printer-1` | WebUSB                                              |
| Receipt printer | `local_printer` | HTTP (ePOS-Print-compatible and WebPRNT-compatible) |
| Barcode scanner | `scanner-1`     | keyboard (types into the focused field)             |
| NFC reader      | `nfc-1`         | Web NFC (`NDEFReader`)                              |

## 2. Connect your app

Pick one:

- **In development builds**, install the shim before your app's code runs:

  ```ts
  import { install } from "@hardless/shim-web";
  if (import.meta.env.DEV) install({ mode: "server", url: "ws://localhost:8080/bridge" });
  ```

- **Without changing the app**, load the Chrome extension (`packages/extension`) and turn it
  on for your app's site.

- **HTTP printers** need nothing: point the app at
  `http://localhost:8080/cgi-bin/epos/service.cgi?devid=local_printer` (ePOS-Print) or
  `http://localhost:8080/StarWebPRNT/SendMessage` (WebPRNT). From an HTTPS page, use port 8443
  and trust the Hardless certificate once (the dashboard shows how).

## 3. Print

Use the app as usual. Choose `printer-1` in the port picker (it looks like the browser's),
print, and the receipt appears in the dashboard, rendered at 58 or 80 mm, with its raw bytes.
A drawer kick in the receipt opens `drawer-1`.

## 4. Try the failure paths

In the dashboard, turn on **Paper out** for the printer and print again: the printer rejects
the print and reports paper end in its status, as a real one does. Every fault is listed in
the [fault reference](reference/faults.md).

## 5. Automate

- **Tests**: [`@hardless/playwright`](testing.md) runs your app with the devices in the page
  (no server) or against a server, with matchers such as `toHavePrinted`.
- **Scripts and CI**: the [REST API](rest-api.md), `hardless scan`, `hardless fault`,
  `hardless receipts`, and scenarios (`POST /api/v1/scenarios/run`).
- **AI coding agents**: the [MCP server](agents.md) lets an agent print, look at the receipt
  and toggle faults.

Next: the [device reference](reference/devices.md).
