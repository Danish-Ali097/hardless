# Hardless

**Hardware-less development for POS web apps.** Hardless is an open-source emulator for
receipt printers, cash drawers, barcode scanners and NFC readers, so web and PWA apps can be
developed and tested without physical devices, including in CI and by AI coding agents.

Hardless emulates at the browser API and transport layer, never the SDK layer. Your app calls
the same `navigator.serial`, `navigator.usb`, `NDEFReader` and HTTP printer endpoints it uses
in production; Hardless plays the device, renders the receipt and lets you inject faults such
as paper out, cover open or a disconnect mid-print.

> **Status: early development.** Milestone M0 (foundation) is done; nothing is released yet.
> See the [roadmap](#roadmap).

## Planned features (phase 1)

| Device                                                           | Route into the browser                 | Milestone |
| ---------------------------------------------------------------- | -------------------------------------- | --------- |
| ESC/POS receipt printer                                          | Web Serial                             | M1        |
| Cash drawer (kicked through the printer)                         | Web Serial                             | M1        |
| HTTP receipt printer (ePOS-Print-compatible, WebPRNT-compatible) | `fetch`, all browsers incl. iOS Safari | M2        |
| Barcode scanner                                                  | keyboard wedge                         | M2        |
| USB receipt printer                                              | WebUSB                                 | M3        |
| NFC reader                                                       | Web NFC (`NDEFReader`)                 | M3        |

Plus a dashboard, a REST API, a CLI, a Playwright fixture with matchers like
`toHavePrinted(/Total: 1,250/)`, and an MCP server so coding agents can print, look at the
rendered receipt and toggle faults.

## Roadmap

- **Phase 1, browser and PWA** (in progress): M0 foundation, then M1 printer and drawer over
  Web Serial, M2 HTTP printers, scanner, CLI and Docker, M3 WebUSB, Web NFC, extension,
  Playwright and MCP, then v0.1.0.
- **Later phases**: desktop and Electron (TCP 9100, virtual COM ports, IPP), smart cards and
  WebHID, mobile and Bluetooth, real USB emulation and record/replay.

The full spec is [docs/PRD.md](docs/PRD.md).

## Getting started

Hardless is not released yet, so for now you build it from source. What works today is the
engine core, which you can use as a Rust library and in tests. Printing from your own app
arrives in milestone M1; the steps for it are under [Using Hardless with your app](#using-hardless-with-your-app).

### 1. Install the prerequisites

| Tool    | Version                       | How to install                                                                      |
| ------- | ----------------------------- | ----------------------------------------------------------------------------------- |
| Rust    | installed by `rustup` for you | [rustup.rs](https://rustup.rs). The repo pins its version in `rust-toolchain.toml`. |
| Node.js | 22 or later                   | [nodejs.org](https://nodejs.org)                                                    |
| pnpm    | 12                            | `npm install -g pnpm@12.14.0`                                                       |
| Git     | any recent version            | [git-scm.com](https://git-scm.com)                                                  |

### 2. Get the code

```sh
git clone https://github.com/Danish-Ali097/hardless.git
cd hardless
```

### 3. Build and test

```sh
cargo test --workspace     # builds the Rust crates and runs their tests
pnpm install               # installs the TypeScript packages
pnpm -r test               # runs their tests
```

The first `cargo` command downloads the pinned Rust toolchain, so it takes a few minutes.

### 4. Try what exists today

Check that the `hardless` binary builds and runs:

```sh
cargo run -p hardless-cli -- --version
```

Open the dashboard shell in your browser (it shows an empty device list until M1):

```sh
pnpm --filter @hardless/dashboard dev
```

Write your own emulated device in Rust with `hardless-core`: add it to a Rust project with

```sh
cargo add hardless-core --git https://github.com/Danish-Ali097/hardless
```

and follow the example in [crates/hardless-core/README.md](crates/hardless-core/README.md). A
device receives the bytes your app sends plus the current time, and returns what it did:
status replies, finished print jobs, connects and disconnects.

[AGENTS.md](AGENTS.md) lists every build, test and lint command.

## Using Hardless with your app

> **Planned.** These steps describe the workflow from the [PRD](docs/PRD.md). They become
> available milestone by milestone, and names may still change before v0.1.0.

**Print from a PWA over Web Serial (M1).**

1. Start Hardless: `hardless start` (later also `npx hardless start` or Docker). The dashboard
   opens at `http://localhost:8080`.
2. In your app's development build, install the shim before your app code runs:

   ```ts
   import { install } from "@hardless/shim-web";

   install({ transport: { mode: "server", url: "ws://localhost:8080/bridge" } });
   ```

   Your app's own Web Serial code does not change.

3. In your app, connect to the printer as usual. The browser's port picker lists the virtual
   printer `printer-1`.
4. Print a receipt. It appears, rendered, in the dashboard, and the cash drawer opens there if
   your app sent a drawer kick.
5. Test failures: turn on **paper out**, **cover open** or **offline** in the dashboard's
   fault panel and check that your app shows the right error.

**Print from iPad or any browser over HTTP (M2):** point your app's ePOS-Print-compatible or
WebPRNT-compatible printer address at `http://<your computer>:8080` (or `https://...:8443`).

**Assert on printed receipts in Playwright (M3):**

```ts
import { test, expect } from "@hardless/playwright";

test("cash sale prints and opens drawer", async ({ page, hardless }) => {
  await page.goto("/pos");
  await page.getByRole("button", { name: "Pay cash" }).click();
  await expect(hardless.printer("printer-1")).toHavePrinted(/Total:\s+1,250/);
  await expect(hardless.drawer("drawer-1")).toHaveOpened({ times: 1 });
});
```

**Let an AI coding agent drive it (M3):** add Hardless as an MCP server in your agent's
config, so it can print, look at the rendered receipt and toggle faults itself:

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
