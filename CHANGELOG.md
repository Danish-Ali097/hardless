# Changelog

All notable changes to Hardless. The GitHub Release notes come from this file; each npm
package also has its own `CHANGELOG.md`.

## 0.1.1 - 2026-10-22

Documentation and package pages only; no behaviour changes.

- Fuller npm pages: the `hardless` package explains what Hardless is, the quick start, the
  default devices, every command, testing and AI agent (MCP) setup. Every package has a fuller
  description, keywords and links to the docs and issues.
- `hardless fault --help` points to the fault reference in the docs.
- Publishing fixes found during 0.1.0: crates are published resumably (crates.io limits how
  many new crates can be published at once), and the crates and extension steps can be run by
  hand for a release tag.

## 0.1.0 - 2026-10-22

The first release: emulated POS hardware for web apps and PWAs (phase 1 of the roadmap).

### Install

- `npx hardless start`, the shell or PowerShell installer below, or
  `brew install hardlessdev/tap/hardless`
- `docker run -p 8080:8080 -p 8443:8443 -v hardless-data:/data hardless/hardless:0.1.0`
- `cargo install hardless-cli`
- npm: `@hardless/shim-web`, `@hardless/playwright`, `@hardless/types`
- Chrome extension: unzip `hardless-extension-0.1.0.zip` and load it unpacked

### Devices

- ESC/POS receipt printer over Web Serial, WebUSB or HTTP (ePOS-Print-compatible and
  WebPRNT-compatible), with receipts rendered as HTML, SVG and PNG at 58 or 80 mm, QR codes,
  barcodes, raster images and code pages CP437 and CP858.
- Cash drawer kicked through the printer.
- Keyboard-wedge barcode scanner.
- NFC reader with virtual cards for Web NFC (`NDEFReader`): read, write, make read-only.
- Faults on demand: paper out, paper near end, cover open, offline, disconnect mid-write,
  USB stall, a jammed drawer, double scans, a cancelled picker, a denied permission, a failed
  card read and more (19 in all).

### Ways to use it

- Dashboard: receipts, drawer, scanner and NFC panels, fault switches and a live event log.
- `@hardless/shim-web`: drop-in `navigator.serial`, `navigator.usb` and `NDEFReader`, in
  server mode or standalone in the page (WebAssembly).
- `@hardless/playwright`: a test fixture with device handles and matchers such as
  `toHavePrinted` and `toHaveOpened`.
- Chrome extension: turns Hardless on for the sites you choose, with no code changes.
- REST API, scenarios and the `hardless` CLI for scripts and CI.
- MCP server (`hardless mcp`, or `/mcp` on the server) so AI coding agents can print, look at
  receipts and toggle faults.

### Platforms

`hardless` runs on Windows, macOS and Linux (glibc and static musl), on x86_64 and arm64. The
Docker image is for linux/amd64 and linux/arm64.
