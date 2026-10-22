# hardless

**POS hardware for web apps, without the hardware.** Hardless emulates receipt printers, cash
drawers, barcode scanners and NFC readers, so you can develop and test a web app or PWA's
hardware code without devices on your desk, in CI, or from an AI coding agent.

Your app's code stays the same. Hardless answers the browser APIs it already uses (Web Serial,
WebUSB, Web NFC, keyboard input) and the HTTP calls it makes to network printers
(ePOS-Print-compatible and WebPRNT-compatible). It renders every receipt so you can see it, and
lets you break things on purpose: paper out, cover open, a printer unplugged mid-print, a
failed card read.

This package runs the `hardless` program with `npx`, with nothing else to install.

## Quick start

```sh
npx hardless start
```

Open the dashboard at <http://localhost:8080>. Without a config file you get one of each
device:

| Device          | ID              | How your app reaches it                 |
| --------------- | --------------- | --------------------------------------- |
| Receipt printer | `printer-1`     | Web Serial                              |
| Cash drawer     | `drawer-1`      | kicked through `printer-1`              |
| Receipt printer | `usb-printer-1` | WebUSB                                  |
| Receipt printer | `local_printer` | HTTP, at `http://localhost:8080`        |
| Barcode scanner | `scanner-1`     | keyboard (types into the focused field) |
| NFC reader      | `nfc-1`         | Web NFC (`NDEFReader`)                  |

An app that prints over HTTP can print straight away: point it at `localhost:8080` instead of
the printer's address. For Web Serial, WebUSB, Web NFC and the scanner, add
[`@hardless/shim-web`](https://www.npmjs.com/package/@hardless/shim-web) to your development
build:

```ts
import { install } from "@hardless/shim-web";

if (import.meta.env.DEV) install({ mode: "server", url: "ws://localhost:8080/bridge" });
```

Or, for an app you can't change, use the Hardless Chrome extension (on the
[release page](https://github.com/hardlessdev/hardless/releases/latest)).

## Commands

```sh
npx hardless start                                 # dashboard, REST API, printers, on 8080 and 8443
npx hardless scan scanner-1 4006381333931          # scan a barcode
npx hardless tap nfc-1 --uid 04a2b3c4d5e6f7         # tap an NFC card
npx hardless fault printer-1 paper-out on          # turn a fault on (or off)
npx hardless receipts printer-1 --last             # show the last receipt
npx hardless mcp --url http://localhost:8080       # MCP server for AI coding agents
npx hardless --help                                # everything else
```

## Testing

[`@hardless/playwright`](https://www.npmjs.com/package/@hardless/playwright) is a Playwright
fixture: it puts the emulated devices in every page and adds matchers like
`toHavePrinted(/Total/)` and `toHaveOpened()`. It needs no server, so it runs anywhere
Playwright does.

## AI coding agents

Add Hardless to your agent's MCP configuration, so it can print, look at the rendered receipt
and toggle faults itself:

```json
{
  "mcpServers": {
    "hardless": {
      "command": "npx",
      "args": ["-y", "hardless", "mcp", "--embedded"]
    }
  }
}
```

`--embedded` starts a Hardless server along with the MCP server. To share the dashboard with
the agent, run `npx hardless start` yourself and use `"--url", "http://localhost:8080"`
instead.

## How this package works

On first run it downloads the `hardless` binary for your platform from
[GitHub Releases](https://github.com/hardlessdev/hardless/releases), checks its SHA-256
against the release's checksum file and caches it, so later runs start straight away. The
package has no dependencies. It needs Node.js 22 or later.

Supported: Windows, macOS and Linux, on x86_64 and arm64. Linux uses a static build, so it
runs on any distribution.

| Environment variable | Effect                                                                                                             |
| -------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `HARDLESS_CACHE_DIR` | Where binaries are cached (default: `%LOCALAPPDATA%\hardless`, `~/Library/Caches/hardless` or `~/.cache/hardless`) |
| `HARDLESS_BINARY`    | Run this binary instead of downloading one                                                                         |

Other ways to install `hardless`: the shell and PowerShell installers on the
[release page](https://github.com/hardlessdev/hardless/releases/latest),
`brew install hardlessdev/tap/hardless`, `cargo install hardless-cli`, or Docker:
`docker run -p 8080:8080 -p 8443:8443 hardless/hardless`.

## Links

- Documentation: <https://hardless-docs.netlify.app>
- Source and issues: <https://github.com/hardlessdev/hardless>

Apache-2.0. Hardless is not affiliated with any printer, scanner or POS hardware maker.
