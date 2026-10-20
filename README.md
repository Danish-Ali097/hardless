# Hardless

**Hardware-less development for POS web apps.** Hardless is an open-source emulator for
receipt printers, cash drawers, barcode scanners and NFC readers, so web and PWA apps can be
developed and tested without physical devices, including in CI and by AI coding agents.

Hardless emulates at the browser API and transport layer, never the SDK layer. Your app calls
the same `navigator.serial`, `navigator.usb`, `NDEFReader` and HTTP printer endpoints it uses
in production; Hardless plays the device, renders the receipt and lets you inject faults such
as paper out, cover open or a disconnect mid-print.

> **Status: early development.** Milestones M1 and M2 are done: receipt printers over Web
> Serial and HTTP (with HTTPS for iPad), a cash drawer, a keyboard-wedge barcode scanner, the
> dashboard, a CLI and a Docker image. Nothing is released yet, so you build from source. See
> the [roadmap](#roadmap).

## Features

| Device                                                           | Route into the browser                 | Status         |
| ---------------------------------------------------------------- | -------------------------------------- | -------------- |
| ESC/POS receipt printer                                          | Web Serial                             | Available (M1) |
| Cash drawer (kicked through the printer)                         | Web Serial                             | Available (M1) |
| HTTP receipt printer (ePOS-Print-compatible, WebPRNT-compatible) | `fetch`, all browsers incl. iOS Safari | Available (M2) |
| Barcode scanner                                                  | keyboard wedge                         | Available (M2) |
| USB receipt printer                                              | WebUSB                                 | Planned (M3)   |
| NFC reader                                                       | Web NFC (`NDEFReader`)                 | Planned (M3)   |

Available now: a dashboard with rendered receipts (download as PNG), scanner presets and
sequences and fault switches; a REST API and scripted scenarios; a CLI for scans, faults and
receipts; HTTPS with a local certificate to trust once per device; faults such as paper out,
cover open, offline, disconnect mid-print, garbled bytes, timeouts and blocked CORS; a Docker
image; and a standalone mode that runs the devices inside the page for tests. Planned: a
Playwright fixture with matchers like `toHavePrinted(/Total: 1,250/)`, and an MCP server so
coding agents can print, look at the rendered receipt and toggle faults.

## Roadmap

- **Phase 1, browser and PWA** (in progress): M0 foundation, M1 printer and drawer over Web
  Serial, and M2 HTTP printers, HTTPS, scanner, scenarios, CLI and Docker are done. Next: M3
  WebUSB, Web NFC, extension, Playwright and MCP; then v0.1.0.
- **Later phases**: desktop and Electron (TCP 9100, virtual COM ports, IPP), smart cards and
  WebHID, mobile and Bluetooth, real USB emulation and record/replay.

The full spec is [docs/PRD.md](docs/PRD.md).

## Getting started

### 1. Install the prerequisites

| Tool    | Version                       | How to install                                                                      |
| ------- | ----------------------------- | ----------------------------------------------------------------------------------- |
| Rust    | installed by `rustup` for you | [rustup.rs](https://rustup.rs). The repo pins its version in `rust-toolchain.toml`. |
| Node.js | 22 or later                   | [nodejs.org](https://nodejs.org)                                                    |
| pnpm    | 12                            | `npm install -g pnpm@12.14.0`                                                       |
| Git     | any recent version            | [git-scm.com](https://git-scm.com)                                                  |
| Chrome  | recent                        | Web Serial works in Chrome and Edge on desktop                                      |

### 2. Get the code and build

```sh
git clone https://github.com/hardlessdev/hardless.git
cd hardless
pnpm install
pnpm --filter @hardless/dashboard build   # the dashboard, embedded into the server
cargo build -p hardless-cli               # the `hardless` binary (first build takes a few minutes)
```

### 3. Start Hardless

```sh
cargo run -p hardless-cli -- start
```

The dashboard opens at <http://localhost:8080> (and <https://localhost:8443>). Without a
config file you get `printer-1`, an ESC/POS receipt printer on Web Serial; `drawer-1`, a cash
drawer plugged into it; `local_printer`, an HTTP printer; and `scanner-1`, a barcode scanner.

### 4. Print something

Print a sample receipt without writing any code:

```sh
node scripts/print-sample.mjs
```

Or run the example POS app, which prints over Web Serial like a real app:

```sh
pnpm --filter @hardless/example-react-pwa dev   # http://localhost:5173
```

Click **Connect printer**, choose `printer-1`, then **Pay cash**. The receipt appears in the
dashboard and the drawer opens. Turn on **Paper out** in the dashboard and pay again: the app
reports that the receipt did not print. See [examples/react-pwa](examples/react-pwa).

[AGENTS.md](AGENTS.md) lists every build, test and lint command.

## Using Hardless with your app

**Print from a PWA over Web Serial (available now).**

1. Start Hardless: `hardless start`. The dashboard is at `http://localhost:8080`.
2. In your app's development build, install the shim before your app code runs:

   ```ts
   import { install } from "@hardless/shim-web";

   if (import.meta.env.DEV) {
     install({ mode: "server", url: "ws://localhost:8080/bridge" });
   }
   ```

   Your app's own Web Serial code does not change.

3. In your app, connect to the printer as usual. The port picker lists the emulated printer
   `printer-1`.
4. Print a receipt. It appears, rendered, in the dashboard, and the cash drawer opens there if
   your app sent a drawer kick.
5. Test failures: turn on **Paper out**, **Cover open**, **Offline**, **Disconnect mid-print**
   or **Slow writes** in the dashboard and check that your app handles them.

For unit tests without a server, use standalone mode, which runs the devices in the page; see
[packages/shim-web](packages/shim-web).

**Print over HTTP, from any browser including iPad (available now).** Point your app's
printer address at Hardless instead of a printer on the network:

- ePOS-Print-compatible: `http://<your computer>:8080/cgi-bin/epos/service.cgi?devid=local_printer`
- WebPRNT-compatible: `http://<your computer>:8080/StarWebPRNT/SendMessage`

From an app on https://, use `https://<your computer>:8443/...` and trust the Hardless
certificate once on each device; the dashboard's HTTP printer panel explains why and lists the
steps. Start Hardless with `--host 0.0.0.0` so phones and tablets on the LAN can reach it.

**Scan barcodes (available now).** With the shim installed, scans from the dashboard, the
REST API (`POST /api/v1/devices/scanner-1/scan`) or `hardless scan scanner-1 4006381333931`
are typed into the focused field, like a USB scanner.

**Script and automate (available now).** `POST /api/v1/scenarios/run` runs a list of steps
(scan, wait, setFault, clearFault, expect). The CLI drives a running server:
`hardless fault printer-1 paper-out on`, `hardless receipts printer-1 --last`.

**Docker (built in CI, published at v0.1.0).** See `Dockerfile` and [AGENTS.md](AGENTS.md):
`docker run -p 8080:8080 -p 8443:8443 -v hardless-data:/data hardless/hardless`.

**Planned** (names may change before v0.1.0):

- **Playwright (M3):**

  ```ts
  import { test, expect } from "@hardless/playwright";

  test("cash sale prints and opens drawer", async ({ page, hardless }) => {
    await page.goto("/pos");
    await page.getByRole("button", { name: "Pay cash" }).click();
    await expect(hardless.printer("printer-1")).toHavePrinted(/Total:s+1,250/);
    await expect(hardless.drawer("drawer-1")).toHaveOpened({ times: 1 });
  });
  ```

- **AI coding agents (M3):** add Hardless as an MCP server in your agent's config, so it can
  print, look at the rendered receipt and toggle faults itself:

  ```json
  {
    "mcpServers": {
      "hardless": { "command": "hardless", "args": ["mcp", "--url", "http://localhost:8080"] }
    }
  }
  ```

## Repository layout

- `crates/`: the Rust engine (sans-IO, also compiled to WebAssembly), server, MCP server and
  CLI.
- `packages/`: TypeScript packages: browser API shims, dashboard, Chrome extension,
  Playwright fixture, generated wire types, `npx hardless`.
- `docs/`: the PRD and the decision log.

## Contributing

Contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) and the
[Code of Conduct](CODE_OF_CONDUCT.md). For security issues, see [SECURITY.md](SECURITY.md).

## License

[Apache-2.0](LICENSE).

## Disclaimer

Hardless is an independent project. It is not affiliated with, endorsed by or sponsored by
any printer, scanner or POS hardware vendor. Protocol support is implemented from publicly
available command references. "ePOS-Print-compatible" and "WebPRNT-compatible" describe
compatibility only; any trademarks belong to their owners.
