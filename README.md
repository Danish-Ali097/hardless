# Hardless

**Hardware-less development for POS web apps.** Hardless is an open-source emulator for
receipt printers, cash drawers, barcode scanners and NFC readers, so web and PWA apps can be
developed and tested without physical devices, including in CI and by AI coding agents.

Hardless emulates at the browser API and transport layer, never the SDK layer. Your app calls
the same `navigator.serial`, `navigator.usb`, `NDEFReader` and HTTP printer endpoints it uses
in production; Hardless plays the device, renders the receipt and lets you inject faults such
as paper out, cover open or a disconnect mid-print.

> **Status: early development.** Milestone M1 is done: an ESC/POS receipt printer and a cash
> drawer over Web Serial, with the dashboard. Nothing is released yet, so you build from source.
> See the [roadmap](#roadmap).

## Features

| Device                                                           | Route into the browser                 | Status         |
| ---------------------------------------------------------------- | -------------------------------------- | -------------- |
| ESC/POS receipt printer                                          | Web Serial                             | Available (M1) |
| Cash drawer (kicked through the printer)                         | Web Serial                             | Available (M1) |
| HTTP receipt printer (ePOS-Print-compatible, WebPRNT-compatible) | `fetch`, all browsers incl. iOS Safari | Planned (M2)   |
| Barcode scanner                                                  | keyboard wedge                         | Planned (M2)   |
| USB receipt printer                                              | WebUSB                                 | Planned (M3)   |
| NFC reader                                                       | Web NFC (`NDEFReader`)                 | Planned (M3)   |

Available now: a dashboard with rendered receipts and fault switches, a REST API, faults such
as paper out, cover open, offline, disconnect mid-print and slow writes, and a standalone mode
that runs the devices inside the page for tests. Planned: a CLI for scans and receipts, a
Playwright fixture with matchers like `toHavePrinted(/Total: 1,250/)`, and an MCP server so
coding agents can print, look at the rendered receipt and toggle faults.

## Roadmap

- **Phase 1, browser and PWA** (in progress): M0 foundation and M1 printer and drawer over Web
  Serial are done. Next: M2 HTTP printers, scanner, CLI and Docker; M3 WebUSB, Web NFC,
  extension, Playwright and MCP; then v0.1.0.
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
git clone https://github.com/Danish-Ali097/hardless.git
cd hardless
pnpm install
pnpm --filter @hardless/dashboard build   # the dashboard, embedded into the server
cargo build -p hardless-cli               # the `hardless` binary (first build takes a few minutes)
```

### 3. Start Hardless

```sh
cargo run -p hardless-cli -- start
```

The dashboard opens at <http://localhost:8080>. Without a config file you get `printer-1`, an
ESC/POS receipt printer, and `drawer-1`, a cash drawer plugged into it.

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

**Planned** (names may change before v0.1.0):

- **HTTP printers (M2):** point your app's ePOS-Print-compatible or WebPRNT-compatible printer
  address at `http://<your computer>:8080` (or `https://...:8443`), for iPad and any browser.
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
