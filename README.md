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

## Building from source

You need Rust (the version in `rust-toolchain.toml` installs automatically), Node.js 22+ and
pnpm 12.

```sh
cargo test --workspace
pnpm install
pnpm -r test
```

[AGENTS.md](AGENTS.md) lists every build, test and lint command.

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
