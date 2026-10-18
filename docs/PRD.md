# Hardless — Product Requirements Document v0.2 (all phases; phase 1 in full detail)

Oct 18, 2026 · @Danish Ali

## 1. Overview

Hardless is an open-source (Apache-2.0) sandbox that emulates POS hardware — receipt printers, cash drawers, barcode scanners and NFC readers — so web and PWA applications can be developed and tested without physical devices. The name stands for hardware-less development. The npm package name was unclaimed on 18 Oct 2026; the npm scope and GitHub org still need to be secured.

**Problem.** Teams building POS, retail and hospitality web apps test hardware by keeping real devices on their desks. That is slow, cannot run in CI, and makes failure paths (paper out, unplugged mid-print, user cancels the device picker) almost impossible to reproduce on demand. Web NFC exists only on Android Chrome, so NFC flows cannot be built on a desktop at all.

**Core principle.** Hardless emulates at the API and transport layer, never at the SDK layer. The app under test calls the same browser APIs (`navigator.serial`, `navigator.usb`, `NDEFReader`) and the same HTTP printer endpoints it uses in production. Hardless plays the device: it parses what the app sends, responds the way real hardware does, and shows the result in a dashboard. The app's production code must not change to use Hardless.

**Phases.** Hardless ships in five phases: browser/PWA (1), desktop and Electron (2), smart cards and HID (3), mobile and Bluetooth (4), and real-USB emulation, label printers and record/replay (5). Phase 1 is specified in full here (sections 4–13); later phases are scoped in section 14 and get their own detailed spec before work starts.

**Three kinds of client.** Hardless is driven by humans (dashboard, CLI), test runners (Playwright) and AI coding agents (an MCP server, section 11.4). An agent writing receipt code can print, look at the rendered receipt, inject a fault and iterate without a person in the loop.

## 2. Goals, non-goals and success metrics

**Goals**

1. A PWA can print a receipt, kick a cash drawer, receive barcode scans and read NFC tags against Hardless with no production code changes.
2. Every emulated device reproduces the real browser API's behaviour, including permission prompts, events and error types.
3. Failure paths are one toggle away: paper out, cover open, disconnect mid-write, slow device, picker cancelled, mixed-content block.
4. Tests can run headless in CI (Playwright) and assert on device output, e.g. "last receipt contains `Total: 1,250`".
5. New devices can be added as plugins by outside contributors without touching the core.
6. AI coding agents can discover devices, read rendered receipts (as text and image), drive scans and taps, and toggle faults through MCP, so they can verify hardware code they write.

**Non-goals (phase 1)**

- Desktop virtual ports, PC/SC, WebHID, Bluetooth, native mobile and USB/IP. These are phases 2–5 (section 14), not part of phase 1.
- Payment card terminals, in any phase. Card-present payment protocols carry certification and legal constraints that an open-source emulator should not take on.
- Pixel-perfect emulation of a specific vendor model. Hardless emulates documented command sets, not branded devices.
- Firefox and Safari support for Web Serial/WebUSB (those browsers do not implement the APIs).

**Success metrics (first 6 months after v0.1)**

| Metric                                                              | Target      |
| ------------------------------------------------------------------- | ----------- |
| Time from `docker run` to first rendered receipt in the example app | < 5 minutes |
| ESC/POS commands in the phase 1 set covered by golden tests         | 100%        |
| Core package test coverage                                          | ≥ 85% lines |
| GitHub stars                                                        | 500         |
| External contributors with a merged PR                              | 10          |
| Real app using it in CI (first: Aamo POS)                           | ≥ 1         |

## 3. Users and use cases

**Primary users:** frontend and full-stack developers building POS, retail, hospitality and kiosk web apps; QA engineers writing end-to-end tests; open-source maintainers of ESC/POS and Web Serial libraries who need a test target.

**User stories**

| ID    | As a…                    | I want to…                                                                | So that…                                                             |
| ----- | ------------------------ | ------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| US-01 | developer                | print from my PWA and see the rendered receipt instantly                  | I can iterate on layout without wasting paper                        |
| US-02 | developer                | see when my app sends a drawer kick                                       | I know the drawer opens only on cash payments                        |
| US-03 | developer                | fire a scripted sequence of barcode scans                                 | I can test fast-scan and duplicate-scan handling                     |
| US-04 | developer                | tap a virtual NFC card on my desktop browser                              | I can build loyalty/staff-card flows without an Android phone        |
| US-05 | developer                | toggle paper-out or unplug mid-print                                      | I can verify my error UI and retry logic                             |
| US-06 | developer                | test printing from an iPad PWA over HTTP/HTTPS                            | I can reproduce mixed-content and CORS failures                      |
| US-07 | QA engineer              | assert on printed output in Playwright                                    | hardware flows are covered in CI                                     |
| US-08 | developer                | load the shim through a browser extension                                 | I can test an app whose code I cannot modify                         |
| US-09 | contributor              | add a new device or command set as a plugin                               | the project grows without core changes                               |
| US-10 | AI coding agent          | read the last receipt as text and image, scan, tap and set faults via MCP | I can check the hardware code I wrote and fix it before handing back |
| US-11 | developer using an agent | tell my agent "make the receipt match this design and handle paper-out"   | the agent iterates against Hardless instead of asking me to print    |

## 4. Scope: devices and browser APIs

Phase 1 ships four device types reachable through four browser routes. Browsers cannot open raw TCP sockets, so a port-9100 network printer is not reachable from a PWA and is out of scope.

| Device                    | Route into the browser                                       | Browser support (real API)    | Hardless delivers                                | Milestone |
| ------------------------- | ------------------------------------------------------------ | ----------------------------- | ------------------------------------------------ | --------- |
| Receipt printer (ESC/POS) | Web Serial                                                   | Chromium desktop only         | `navigator.serial` shim + virtual serial printer | M1        |
| Cash drawer               | ESC/POS drawer kick sent through the printer                 | Same as printer               | Drawer state machine attached to the printer     | M1        |
| Receipt printer (HTTP)    | `fetch` to an ePOS-Print-style XML or WebPRNT-style endpoint | All browsers incl. iOS Safari | HTTP + HTTPS endpoints in the server             | M2        |
| Barcode scanner           | Keyboard wedge (key events)                                  | All browsers                  | Keystroke injector in the shim                   | M2        |
| Receipt printer (USB)     | WebUSB                                                       | Chromium desktop + Android    | `navigator.usb` shim + USB printer-class device  | M3        |
| NFC reader                | Web NFC (`NDEFReader`)                                       | Android Chrome only           | `NDEFReader` shim + "tap card" panel             | M3        |

WebHID devices (HID-mode scanners, customer displays) are deferred to phase 3 but the shim architecture must allow adding `navigator.hid` without refactoring.

## 5. System architecture

The app reaches Hardless through only two doors — the shim and HTTP printer endpoints — and everything after them is shared, so a receipt renders and asserts the same way whichever route printed it.

&#91;embedded content: Hardless phase 1 architecture · browser routes in, Rust engine, three clients out\]

**Data flow (print job via Web Serial)**

1. App calls `navigator.serial.requestPort()`; the shim shows the emulated picker and returns a `SerialPort` bound to device `printer-1`.
2. App writes ESC/POS bytes to `port.writable`; the shim sends `write` frames over `/bridge`.
3. Server routes bytes to the device instance in `hardless-core`; the printer device feeds them to `hardless-escpos`, which emits AST nodes.
4. On cut or idle (configurable, default 300 ms), the device closes a print job, stores it, and emits `device:output`. A drawer-kick node triggers the attached drawer.
5. Dashboard receives the job over WebSocket and renders it with `hardless-render`; Playwright and CLI read it via REST.
6. Device status replies (e.g. `DLE EOT`) flow back as `data` frames to `port.readable`.

HTTP printing follows steps 3–5; the bridge box translates the XML dialects to the same AST before step 3.

**Run modes**

| Mode       | Where devices run                                                                      | Use for                                                                                                                      |
| ---------- | -------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| Server     | The `hardless` binary (`hardless start`, `npx hardless start` or Docker)               | Manual testing with the dashboard, HTTP printers, iPad testing, shared CI server, and every OS-level transport in phases 2–5 |
| Standalone | In the page: `shim-web` runs the Rust engine compiled to WebAssembly (`hardless-wasm`) | Unit tests and Playwright runs with no server; Web Serial/USB/NFC/wedge only                                                 |

## 6. Tech stack, repo layout and conventions

**Stack**

| Concern              | Choice                                                                                                                                                                                                                                                                                                              |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Language             | Rust (stable, edition 2021) for everything that touches devices, the OS or the network; TypeScript 5.x, `strict: true`, ESM only, for code that must run in the browser                                                                                                                                             |
| Why Rust             | Later phases need OS- and hardware-level access (PTYs, uinput/SendInput, USB/IP, USB gadget mode, PC/SC, BlueZ). Rust gives that access safely, ships as one static binary with no runtime, and compiles the same engine to WebAssembly for the browser                                                             |
| Engine               | Crates `hardless-core`, `hardless-escpos`, `hardless-render`, `hardless-devices`: sans-IO state machines, built natively and for `wasm32-unknown-unknown` via `wasm-bindgen`                                                                                                                                        |
| Server               | `tokio` + `axum` (HTTP, WebSocket), `rustls` + `rcgen` for the local HTTPS certificate                                                                                                                                                                                                                              |
| Agent interface      | `rmcp` (official Rust MCP SDK), stdio + Streamable HTTP, inside the `hardless` binary                                                                                                                                                                                                                               |
| OS access (phase 2+) | `serialport` / PTYs via `nix`, platform input APIs, USB/IP and USB gadget (FunctionFS), vpcd socket protocol for PC/SC, `bluer` for BlueZ                                                                                                                                                                           |
| Receipt rendering    | SVG generated in Rust, PNG via `resvg`; QR via `qrcode`; barcodes via `barcoders`                                                                                                                                                                                                                                   |
| Browser packages     | `shim-web`, `dashboard` (React 18 + Vite + Tailwind + Zustand), `extension`, `playwright` in a pnpm workspace                                                                                                                                                                                                       |
| Wire types           | Defined once in Rust with `serde` + `schemars`; TypeScript types generated with `ts-rs`; Zod schemas generated from the JSON Schema for runtime validation in the browser                                                                                                                                           |
| Tests                | `cargo test` + `insta` snapshots, `proptest` and `cargo-fuzz` for parsers; Vitest and Playwright for browser packages                                                                                                                                                                                               |
| Lint/format          | `clippy` (warnings denied), `rustfmt`; ESLint (typescript-eslint) + Prettier                                                                                                                                                                                                                                        |
| Versioning           | `release-plz` for crates, Changesets for npm packages, semver                                                                                                                                                                                                                                                       |
| CI                   | GitHub Actions: fmt, clippy, test, wasm build, `cargo-deny`, browser lint/test, e2e on every PR                                                                                                                                                                                                                     |
| Distribution         | `hardless` binary for Windows, macOS, Linux (x86\_64 + arm64) via `cargo-dist` (GitHub Releases, shell/PowerShell installers, Homebrew); `npx hardless` wrapper that downloads the binary; Docker image `hardless/hardless`; crates on crates.io; browser packages on npm under `@hardless/*`; Chrome MV3 extension |

**Repo layout**

```
hardless/
  AGENTS.md                 how coding agents build, test and use Hardless in this repo
  llms.txt                  index of docs for LLMs (devices, faults, API, MCP tools)
  Cargo.toml                Cargo workspace
  crates/
    hardless-core/          registry, Device trait, faults, event log, scenarios (sans-IO, no_std + alloc where possible)
    hardless-escpos/        byte stream -> command AST
    hardless-render/        AST -> HTML / SVG / PNG at 58mm or 80mm
    hardless-devices/       printer, cash drawer, scanner, NFC reader (feature per device)
    hardless-http-dialects/ ePOS-Print-style and WebPRNT-style XML -> AST
    hardless-server/        axum: WebSocket bridge, HTTP printer endpoints, REST API, dashboard assets
    hardless-mcp/           MCP server (rmcp): tools + resources, stdio and /mcp
    hardless-wasm/          wasm-bindgen build of the engine for shim-web standalone mode
    hardless-cli/           the `hardless` binary: start, scan, tap, fault, receipts, mcp, ports
    hardless-transport-*/   phase 2+: tcp, serial, ipp, input, pcsc, hid, ble, usbip, gadget
  packages/                 pnpm workspace (TypeScript)
    types/                  generated from Rust with ts-rs (never edited by hand)
    shim-web/               navigator.serial, navigator.usb, NDEFReader, keyboard wedge
    dashboard/              React UI, built into hardless-server as static assets
    extension/              Chrome MV3, injects shim-web at document_start in MAIN world
    playwright/             test fixture + matchers
    npm-wrapper/            `npx hardless` -> downloads the platform binary
  examples/
    react-pwa/              prints, kicks drawer, scans, reads NFC
  docs/
  fixtures/escpos/          golden .bin inputs + expected AST/HTML snapshots
```

**Conventions**

- **Sans-IO engine.** `hardless-core`, `hardless-escpos`, `hardless-render`, `hardless-devices` and `hardless-http-dialects` do no I/O and read no clock: devices are pure state machines fed bytes, control commands and a `now` timestamp. Transports and the server own all I/O and time. CI builds these crates for `wasm32-unknown-unknown`.
- **Unsafe code.** `#![forbid(unsafe_code)]` in engine, server and MCP crates. `unsafe` is allowed only in `hardless-transport-*` crates, each block with a `// SAFETY:` comment.
- **No panics on input.** Parsers return errors or `Unknown` nodes; `clippy::unwrap_used` and `clippy::expect_used` are denied outside tests.
- **One source of truth for wire types.** Change a type in Rust, regenerate with `cargo xtask gen-types`; CI fails if `packages/types` is stale.
- **TypeScript.** No `any`; public APIs documented with TSDoc. Rust public items have rustdoc with an example.
- Conventional Commits; release notes from `release-plz` and Changesets.
- All text and byte encodings are explicit (`&[u8]`/`Uint8Array`, named code pages); never rely on platform defaults.

## 7. Functional requirements by package

Requirement IDs (FR-xx) are referenced by the milestones in section 13.

### 7.1 `hardless-core` (Rust)

- **FR-01 Device plugin interface.** A device is defined as:

```rust
pub trait DevicePlugin: Send + Sync + 'static {
    fn kind(&self) -> &'static str;                  // e.g. "printer-escpos"
    fn config_schema(&self) -> schemars::Schema;
    fn create(&self, id: DeviceId, config: serde_json::Value)
        -> Result<Box<dyn Device>, ConfigError>;
}

/// Sans-IO: a device is a pure state machine. Transports own all I/O and time.
pub trait Device: Send {
    fn kind(&self) -> &'static str;
    fn state(&self) -> serde_json::Value;
    fn on_host_bytes(&mut self, data: &[u8], now: Millis) -> Vec<DeviceEvent>; // host -> device
    fn on_control(&mut self, cmd: ControlCommand, now: Millis) -> Vec<DeviceEvent>; // scan, tap, close drawer
    fn on_tick(&mut self, now: Millis) -> Vec<DeviceEvent>; // pulses, idle job close
    fn next_deadline(&self) -> Option<Millis>;
    fn set_fault(&mut self, fault: FaultId, active: bool, opts: FaultOptions);
    fn reset(&mut self);
}

pub enum DeviceEvent {
    ToHost(Bytes),            // device -> host bytes (status replies)
    StateChanged,
    JobCompleted(Job),        // a print job with its AST
    Connected, Disconnected,
    Log(LogEntry),
}
```

- **FR-02 Device registry.** Register plugins, create/remove instances by config, look up by `id`, list all. Emits `device:added`, `device:removed`, `device:state`, `device:output` on an event bus.
- **FR-03 Event log.** Every byte in/out and every state change is recorded with a monotonic timestamp, capped at a configurable size (default 10,000 events per device).
- **FR-04 Scenario runner.** Executes a JSON scenario: an ordered list of steps (`scan`, `tap`, `wait`, `setFault`, `clearFault`, `expect`) with millisecond delays.
- **FR-05 Deterministic clock.** Inject a clock so tests can use fake timers.

### 7.2 `hardless-escpos` (Rust)

- **FR-06** Streaming parser: accepts chunks of any size (a command may be split across chunks) and emits a command AST.
- **FR-07** Phase 1 command set: `ESC @` init, `LF`, `ESC d n` feed, `ESC a n` align, `ESC E n` bold, `ESC - n` underline, `GS ! n` size, `ESC t n` code page, `GS V` cut (full/partial), `ESC p m t1 t2` drawer kick, `GS ( k` QR code, `GS k` barcode (CODE128, EAN13, CODE39), `GS v 0` raster image, `DLE EOT n` real-time status.
- **FR-08** Unknown commands produce an `unknown` AST node with the raw bytes and a warning. The parser never panics.
- **FR-09** Code pages: CP437 and CP858 in phase 1, pluggable table for others.

### 7.3 `hardless-render` (Rust)

- **FR-10** Render an AST to HTML at 58 mm (32/42 columns) or 80 mm (48/64 columns, Font A/B).
- **FR-11** Render to SVG and PNG with identical layout to the HTML output.
- **FR-12** Draw cut lines, QR codes, barcodes and raster images. Mark `unknown` commands as a small visible badge, toggleable.
- **FR-13** Expose plain-text extraction (`to_text(&ast)`) for test assertions.

### 7.4 `shim-web` (TypeScript + `hardless-wasm`)

- **FR-14** `install(options)` replaces `navigator.serial`, `navigator.usb` and `globalThis.NDEFReader` (each independently toggleable) and starts the keyboard-wedge injector. `uninstall()` restores the originals.
- **FR-15** Two transports: `{ mode: "server", url: "ws://localhost:8080/bridge" }` and `{ mode: "standalone", devices: [...] }`. In standalone mode the shim lazy-loads `hardless-wasm`, the same Rust engine compiled to WebAssembly, so device behaviour is identical in both modes.
- **FR-16** Shim API fidelity as specified in section 8.
- **FR-17** Install must be idempotent and must run before app code when injected by the extension.

### 7.5 `hardless-server` (Rust)

- **FR-18** WebSocket bridge at `/bridge` using the message protocol in section 11.
- **FR-19** HTTP printer endpoints as specified in section 9.
- **FR-20** REST control API as specified in section 11.
- **FR-21** Serves the dashboard at `/`. Config from `hardless.config.json`, env vars, or CLI flags (in that order of precedence, lowest first).
- **FR-22** Ports: 8080 HTTP (dashboard, API, bridge), 8443 HTTPS with an auto-generated self-signed certificate.

### 7.6 `dashboard`

- **FR-23** Device list with add/remove, live status and a panel per device kind.
- **FR-24** Printer panel: live receipt stack (newest first), paper width switch, download PNG, raw hex view per job, clear.
- **FR-25** Drawer panel: open/closed animation, kick counter, manual close button.
- **FR-26** Scanner panel: input box + "Scan" button, saved barcode presets, sequence runner.
- **FR-27** NFC panel: card list (UID + NDEF records editor), "Tap" button.
- **FR-28** Fault panel per device with toggles for every fault in section 10.
- **FR-29** Event log view with filter by device and direction.

### 7.7 `hardless` CLI (Rust)

- **FR-30** `hardless start [--config] [--port] [--https-port]`, `hardless scan <device> <code>`, `hardless tap <device> --uid <hex>`, `hardless fault <device> <fault> on|off`, `hardless receipts <device> --last`.

## 8. Browser API shim fidelity

The shim must match the WICG/W3C specifications closely enough that code written for real devices runs unchanged. Errors must be real `DOMException` instances with the exact `name` listed below. When this PRD and the spec disagree, follow the spec and open an issue.

### 8.1 Web Serial (`navigator.serial`)

| Call / event                      | Required behaviour                                                                                                                                                                                                                                             |
| --------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `requestPort({ filters })`        | Throws `SecurityError` without transient user activation. Otherwise shows the emulated picker (a shadow-DOM overlay) listing devices that match `usbVendorId`/`usbProductId` filters. Cancel → `NotFoundError`. Option `autoSelect` skips the picker in tests. |
| `getPorts()`                      | Returns ports granted earlier on this origin; grants persist across reloads (sessionStorage key `hardless:grants`).                                                                                                                                            |
| `port.getInfo()`                  | Returns the device's configured `usbVendorId` and `usbProductId`.                                                                                                                                                                                              |
| `port.open({ baudRate, ... })`    | Validates options like the spec (`TypeError` for a missing or zero `baudRate`). Opening an open port → `InvalidStateError`. Baud rate mismatch with device config → opens, but the device receives garbled bytes (fault `baud-mismatch`).                      |
| `port.readable` / `port.writable` | Real `ReadableStream<Uint8Array>` / `WritableStream<Uint8Array>`. Writes honour back-pressure; `slow-write` fault delays chunk acknowledgement.                                                                                                                |
| Disconnect                        | Fires `disconnect` on `navigator.serial` and the port; in-flight writes reject with `NetworkError`; readable errors with `NetworkError`.                                                                                                                       |
| Reconnect                         | Fires `connect`.                                                                                                                                                                                                                                               |
| `port.close()`                    | Rejects with `InvalidStateError` while streams are locked, as in the spec.                                                                                                                                                                                     |
| `setSignals` / `getSignals`       | Implemented; `DTR`/`RTS` recorded in the event log.                                                                                                                                                                                                            |

### 8.2 WebUSB (`navigator.usb`)

| Call / event                                      | Required behaviour                                                                                                                                                                                                               |
| ------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `requestDevice({ filters })`                      | Same user-activation and picker rules as Web Serial. Empty filters array is allowed; missing `filters` → `TypeError`.                                                                                                            |
| `getDevices()`                                    | Previously granted devices.                                                                                                                                                                                                      |
| Descriptors                                       | `USBDevice` exposes `vendorId`, `productId`, `productName`, `manufacturerName`, `serialNumber` and one configuration with interface class `0x07` (printer), subclass `0x01`, protocol `0x02`, a bulk OUT and a bulk IN endpoint. |
| `open` / `selectConfiguration` / `claimInterface` | Enforce order. `claimInterface` on an unopened device → `InvalidStateError`; on an interface claimed by another tab/instance (fault `interface-busy`) → `NetworkError`.                                                          |
| `transferOut(ep, data)`                           | Resolves `USBOutTransferResult` with `bytesWritten` and `status: "ok"`; fault `stall` → `status: "stall"`; disconnect → `NotFoundError`.                                                                                         |
| `transferIn(ep, len)`                             | Returns status bytes (`DLE EOT` responses) when queued.                                                                                                                                                                          |
| `controlTransferIn`                               | Supports the printer-class `GET_DEVICE_ID` and `GET_PORT_STATUS` requests.                                                                                                                                                       |
| Events                                            | `connect` / `disconnect` on `navigator.usb` with `USBConnectionEvent`.                                                                                                                                                           |

### 8.3 Web NFC (`NDEFReader`)

| Call / event         | Required behaviour                                                                                                                                                                              |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `new NDEFReader()`   | Available on any browser when the shim is installed.                                                                                                                                            |
| `scan({ signal })`   | Requires user activation on first call (`SecurityError` otherwise); fault `permission-denied` → `NotAllowedError`. Abort via `signal` stops reading.                                            |
| `reading` event      | Fired on dashboard/CLI/API "tap" with `serialNumber` (colon-separated hex) and `message.records` (`NDEFRecord` with `recordType`, `mediaType`, `id`, `data` as `DataView`, `encoding`, `lang`). |
| `readingerror` event | Fired by fault `read-error`.                                                                                                                                                                    |
| `write(message)`     | Writes to the card currently "on" the reader; no card present → waits until a tap, as in the spec. Written records are visible in the dashboard.                                                |
| `makeReadOnly()`     | Marks the virtual card read-only; later writes → `NotAllowedError`.                                                                                                                             |

### 8.4 Keyboard-wedge scanner

- Dispatches `keydown`, `keypress`, `input` (where applicable) and `keyup` for each character on `document.activeElement`, then a terminator (`Enter` default; `Tab` or none configurable).
- Configurable inter-key delay (default 8 ms), prefix and suffix strings.
- Synthetic events have `isTrusted === false`. Document this limitation; apps that check `isTrusted` cannot be tested with the wedge in phase 1.

## 9. HTTP printers and cash drawer

### 9.1 HTTP printer endpoints

HTTP printing is the only route that works from iOS Safari PWAs, so its realism matters most. Implement both dialects from the vendors' publicly documented XML schemas; do not copy SDK source code.

| Dialect               | Endpoint (on 8080 and 8443)                              | Request                                                                                                          | Response                                                                                                                 |
| --------------------- | -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| ePOS-Print-compatible | `POST /cgi-bin/epos/service.cgi?devid=<id>&timeout=<ms>` | SOAP envelope containing an `epos-print` XML document (text, feed, align, barcode, symbol/QR, image, cut, pulse) | SOAP `response` element with `success`, `code` and a `status` bitmask (online, cover open, paper end, drawer open, etc.) |
| WebPRNT-compatible    | `POST /StarWebPRNT/SendMessage`                          | XML request document (text, alignment, emphasis, barcode, QR, image, cut, peripheral/drawer)                     | XML response with `TraceID`, success flag and printer status bytes                                                       |

Requirements:

- **FR-31** Each dialect is translated to the same command AST as ESC/POS, so the renderer and assertions work identically regardless of route.
- **FR-32** CORS: respond to preflight with `Access-Control-Allow-Origin` echoing the request origin, allowed methods `POST, OPTIONS`, and `Content-Type` / `SOAPAction` headers. Fault `cors-blocked` omits the headers.
- **FR-33** HTTPS on 8443 with a self-signed certificate, so developers can reproduce what a real printer does to an HTTPS PWA: a call to `http://` is blocked by the browser as mixed content, and a call to `https://` fails until the certificate is trusted. The dashboard explains both cases and links to the cert-trust steps.
- **FR-34** Faults map to the dialect's status codes and error codes (e.g. paper end, cover open, offline, timeout).
- **FR-35** Multiple HTTP printers are addressed by `devid` (ePOS) or by path prefix `/p/<deviceId>/StarWebPRNT/SendMessage` (WebPRNT).

### 9.2 Cash drawer

- **FR-36** A drawer attaches to a printer (`attachedTo: "<printerId>"`, pin 2 or pin 5). It opens when the printer receives `ESC p` on the matching pin, an ePOS `pulse`, or a WebPRNT drawer command.
- **FR-37** State machine: `closed` → `opening` (pulse duration from command `t1`, min 50 ms) → `open` → `closed` (manual close in dashboard/API, or auto-close after configurable N seconds, default off).
- **FR-38** Drawer status is reported through the printer's real-time status (`DLE EOT 1` bit 2) and the HTTP dialects' status fields, so apps that poll "is the drawer closed?" work.
- **FR-39** Faults: `jammed` (kick received, stays `closed`), `not-connected` (kick ignored, status reports no drawer).

## 10. Fault injection catalogue

Every fault can be toggled from the dashboard, REST API, CLI, scenario runner and Playwright fixture. Faults with options accept them as a JSON object.

| Fault ID               | Applies to           | Effect                                                   | Options                    |
| ---------------------- | -------------------- | -------------------------------------------------------- | -------------------------- |
| `paper-out`            | printers             | Prints rejected; status reports paper end                | —                          |
| `paper-near-end`       | printers             | Prints succeed; status reports near end                  | —                          |
| `cover-open`           | printers             | Prints rejected; status reports cover open               | —                          |
| `offline`              | printers             | Serial/USB: device disconnects. HTTP: connection refused | —                          |
| `disconnect-mid-write` | serial, USB printers | Disconnects after N bytes of the next job                | `afterBytes` (default 64)  |
| `slow-write`           | serial, USB printers | Delays each chunk ack                                    | `msPerChunk` (default 200) |
| `baud-mismatch`        | serial printers      | Received bytes are scrambled                             | —                          |
| `garbled`              | all printers         | Random byte corruption                                   | `rate` 0–1 (default 0.05)  |
| `timeout`              | HTTP printers        | Response delayed past the request's `timeout`            | —                          |
| `cors-blocked`         | HTTP printers        | CORS headers omitted                                     | —                          |
| `stall`                | USB printers         | `transferOut` returns `status: "stall"`                  | —                          |
| `interface-busy`       | USB printers         | `claimInterface` rejects                                 | —                          |
| `picker-cancel`        | serial, USB, NFC     | Next `request*` call behaves as user cancel              | —                          |
| `permission-denied`    | NFC                  | `scan()` rejects `NotAllowedError`                       | —                          |
| `read-error`           | NFC                  | Next tap fires `readingerror`                            | —                          |
| `jammed`               | drawer               | Kick received, drawer stays closed                       | —                          |
| `not-connected`        | drawer               | Kick ignored; status shows no drawer                     | —                          |
| `double-scan`          | scanner              | Each scan is sent twice                                  | `gapMs` (default 30)       |
| `partial-scan`         | scanner              | Terminator dropped                                       | —                          |

## 11. Control API, bridge protocol, Playwright fixture and MCP server

### 11.1 REST API (base `/api/v1`, JSON, validated with serde + JSON Schema)

| Method | Path                                                                 | Purpose                                                     |
| ------ | -------------------------------------------------------------------- | ----------------------------------------------------------- |
| GET    | `/devices`                                                           | List devices with state                                     |
| POST   | `/devices`                                                           | Create a device from `{ kind, id?, config }`                |
| DELETE | `/devices/:id`                                                       | Remove a device                                             |
| POST   | `/devices/:id/reset`                                                 | Clear state, jobs and faults                                |
| GET    | `/devices/:id/jobs?limit=`                                           | Print jobs, newest first: `{ id, at, ast, text, bytesHex }` |
| GET    | `/devices/:id/jobs/last`                                             | Latest job                                                  |
| GET    | `/devices/:id/jobs/:jobId/render?format=html\|svg\|png&width=58\|80` | Rendered receipt                                            |
| POST   | `/devices/:id/scan`                                                  | `{ code, terminator?, delayMs? }`                           |
| POST   | `/devices/:id/tap`                                                   | `{ uid, records[] }`                                        |
| PUT    | `/devices/:id/faults/:faultId`                                       | `{ active, options? }`                                      |
| GET    | `/devices/:id/events?since=`                                         | Event log                                                   |
| POST   | `/scenarios/run`                                                     | Run a scenario JSON; returns step results                   |
| GET    | `/health`                                                            | Liveness for Docker/CI                                      |

Errors use `{ error: { code, message, details? } }` with 400/404/409/500.

### 11.2 WebSocket bridge (`/bridge`)

JSON messages, one per frame, with binary payloads base64-encoded. Every message has `{ v: 1, type, id?, deviceId? }`.

| Direction       | `type`                      | Payload                                                         |
| --------------- | --------------------------- | --------------------------------------------------------------- |
| client → server | `hello`                     | `{ origin, shimVersion, apis: ["serial","usb","nfc","wedge"] }` |
| server → client | `devices`                   | Full device list for picker/`getPorts`                          |
| client → server | `open` / `close`            | `{ deviceId, options }`                                         |
| client → server | `write`                     | `{ deviceId, data }`; server replies `ack` (or `error`)         |
| server → client | `data`                      | `{ deviceId, data }` (device → host bytes)                      |
| server → client | `connect` / `disconnect`    | `{ deviceId }`                                                  |
| server → client | `nfc-reading` / `nfc-error` | `{ deviceId, serialNumber, records }`                           |
| server → client | `wedge-scan`                | `{ code, delayMs, terminator }`                                 |
| both            | `error`                     | `{ code, message, replyTo? }`                                   |

The shim reconnects with exponential backoff (250 ms → 5 s) and surfaces a dropped bridge as device `disconnect` events.

### 11.3 Playwright fixture (`@hardless/playwright`)

```ts
import { test, expect } from "@hardless/playwright";

test.use({
  hardless: {
    mode: "standalone", // or { mode: 'server', url }
    devices: [
      { kind: "printer-escpos", id: "printer-1", transport: "serial", autoSelect: true },
      { kind: "cash-drawer", id: "drawer-1", attachedTo: "printer-1" },
      { kind: "scanner-wedge", id: "scanner-1" },
    ],
  },
});

test("cash sale prints and opens drawer", async ({ page, hardless }) => {
  await page.goto("/pos");
  await hardless.scanner("scanner-1").scan("8964000123456");
  await page.getByRole("button", { name: "Pay cash" }).click();
  await expect(hardless.printer("printer-1")).toHavePrinted(/Total:\s+1,250/);
  await expect(hardless.drawer("drawer-1")).toHaveOpened({ times: 1 });
});
```

- **FR-40** The fixture injects the shim with `page.addInitScript` before navigation.
- **FR-41** Custom matchers: `toHavePrinted(textOrRegex)`, `toHavePrintedJobs(n)`, `toHaveCut()`, `toHaveOpened({ times })`, `toBeClosed()`, `toHaveWrittenNdef(records)`. All poll with Playwright's default timeout.
- **FR-42** Fault helpers: `hardless.printer(id).fault('paper-out')`, `.clearFault(...)`.

### 11.4 MCP server for coding agents (`hardless mcp`)

The MCP server is a thin, typed layer over the REST API: every agent action is one a human could take in the dashboard, and no device logic lives in the `hardless-mcp` crate, which ships inside the `hardless` binary.

- **FR-43 Transports.** stdio (`hardless mcp --url http://localhost:8080`) for local agents, and Streamable HTTP at `/mcp` on the server for agents running in containers or remotely. One implementation serves both.
- **FR-44 Embedded mode.** `--embedded` starts an in-process Hardless server when none is reachable, so an agent can use Hardless with a single command.
- **FR-45 Tools.** As listed below, each with input and output JSON Schemas generated by schemars.
- **FR-46 Resources.** As listed below; reference docs are generated from the same source as `docs/`.
- **FR-47 Receipt results.** Receipt tools return a text block (plain-text receipt), structured content (job metadata and AST summary) and, unless `format: "text"`, a PNG image block, so multimodal agents can see layout, alignment, QR codes and cuts.
- **FR-48 Annotations and modes.** Read tools carry `readOnlyHint`; `remove_device` and `reset_device` carry `destructiveHint`. `--read-only` hides every mutating tool.
- **FR-49 Errors.** Tool failures return `isError: true` with the REST error code and a one-line next step, e.g. "Device `printer-1` not found. Call `list_devices`."
- **FR-50 Model-facing descriptions.** Each tool description states when to use it and includes one example input, in under 300 characters.
- **FR-51 Agent docs.** Ship `AGENTS.md` (how to build, test and run Hardless in this repo) and `llms.txt` (index of device, fault, API and MCP docs) at the repo root, kept in sync by a CI check.

**Tools**

| Tool               | Input                                                                     | Returns                                                  | Hint        |
| ------------------ | ------------------------------------------------------------------------- | -------------------------------------------------------- | ----------- |
| `list_devices`     | —                                                                         | Devices with kind, state, active faults                  | read-only   |
| `create_device`    | `kind`, `id?`, `config`                                                   | Created device                                           | —           |
| `remove_device`    | `id`                                                                      | `ok`                                                     | destructive |
| `reset_device`     | `id`                                                                      | `ok`                                                     | destructive |
| `get_last_receipt` | `deviceId`, `format?` (`text`\|`image`\|`both`, default `both`), `width?` | Text + PNG + job metadata                                | read-only   |
| `list_jobs`        | `deviceId`, `limit?`                                                      | Job summaries, newest first                              | read-only   |
| `get_job`          | `deviceId`, `jobId`, `format?`                                            | As `get_last_receipt`                                    | read-only   |
| `wait_for_job`     | `deviceId`, `timeoutMs?` (default 10000)                                  | Next job, as `get_last_receipt`; timeout error otherwise | read-only   |
| `decode_escpos`    | `bytes` (hex or base64), `width?`                                         | AST + text + PNG, without any device                     | read-only   |
| `scan_barcode`     | `deviceId`, `code`, `terminator?`, `delayMs?`                             | `ok`                                                     | —           |
| `tap_nfc`          | `deviceId`, `uid`, `records?`                                             | `ok`                                                     | —           |
| `set_fault`        | `deviceId`, `faultId`, `active`, `options?`                               | Device state                                             | —           |
| `list_faults`      | `kind?`                                                                   | Fault catalogue (section 10)                             | read-only   |
| `get_events`       | `deviceId`, `since?`, `limit?`                                            | Event log with hex and decoded commands                  | read-only   |
| `run_scenario`     | Scenario JSON                                                             | Step results                                             | —           |

**Resources**

| URI                                     | Content                                                       |
| --------------------------------------- | ------------------------------------------------------------- |
| `hardless://devices`                    | JSON device list                                              |
| `hardless://devices/{id}/receipts/last` | Last receipt as plain text                                    |
| `hardless://docs/escpos-commands`       | Supported ESC/POS commands with byte sequences                |
| `hardless://docs/faults`                | Fault catalogue                                               |
| `hardless://docs/http-dialects`         | ePOS-Print-style and WebPRNT-style request/response reference |

**Client configuration (published in the README)**

```json
{
  "mcpServers": {
    "hardless": {
      "command": "hardless",
      "args": ["mcp", "--url", "http://localhost:8080"],
      "env": { "HARDLESS_TOKEN": "" }
    }
  }
}
```

Without a global install, use `"command": "npx"` with `"args": ["-y", "hardless", "mcp", "--url", "http://localhost:8080"]`.

**Reference agent loop.** The agent edits the app's receipt code, triggers a print (Playwright test or `run_scenario`), calls `wait_for_job`, inspects the PNG and text, fixes the code, then calls `set_fault` with `paper-out` and checks that the app shows its error state. The MCP server inherits NFR-01 to NFR-03: localhost only by default, token required beyond it.

## 12. Non-functional requirements

**Security**

- **NFR-01** The server binds to `127.0.0.1` by default. Binding to `0.0.0.0` (needed for iPad testing on the LAN) requires `--host 0.0.0.0` and prints a warning.
- **NFR-02** Optional bearer token (`--token`) for the REST API and bridge when bound beyond localhost.
- **NFR-03** The shim must be inert in production: the npm entry point warns if `install()` runs on a non-localhost origin unless `allowAnyOrigin: true`. The extension only activates on origins the user enables.
- **NFR-04** No telemetry. No outbound network calls from any package.
- **NFR-05** The self-signed certificate is generated locally on first run; it is never committed or shipped in the image.

**Performance**

- **NFR-06** Bridge round trip (shim write → ack) < 10 ms on localhost at p95.
- **NFR-07** A 2 KB ESC/POS job parses and renders to HTML in < 5 ms natively and < 20 ms in WebAssembly.
- **NFR-08** `shim-web` JS < 30 KB gzipped; `hardless-wasm` < 500 KB gzipped and lazy-loaded only in standalone mode.
- **NFR-09** `hardless` binary < 25 MB; Docker image (static binary on distroless) < 40 MB; cold start < 300 ms; idle memory < 50 MB.

**Compatibility**

- **NFR-10** Shim: latest 2 versions of Chrome and Edge for Serial/USB; NDEFReader shim and wedge on Chrome, Edge, Firefox and Safari.
- **NFR-11** `hardless` binary: Windows 10+, macOS 13+, Linux (glibc and static musl builds), each on x86\_64 and arm64. CI tests Windows, macOS and Linux. Minimum supported Rust version = stable minus 2 releases.
- **NFR-12** Docker image: `linux/amd64` and `linux/arm64`.

**Accessibility and UX**

- **NFR-13** Dashboard meets WCAG 2.1 AA for contrast and keyboard navigation, and supports light and dark themes.

**Licensing and legal**

- **NFR-14** Apache-2.0 for all crates and packages. Dependencies must be MIT, Apache-2.0, BSD, ISC, Zlib or Unicode-3.0; CI enforces this with `cargo-deny` (Rust) and `license-checker` (npm), and `cargo-deny` also blocks crates with security advisories.
- **NFR-15** No vendor trademarks in package names, device kinds or UI labels. Dialects are described as "ePOS-Print-compatible" and "WebPRNT-compatible". README carries a non-affiliation disclaimer.
- **NFR-16** Protocol support is implemented from public command references only. No code copied from vendor SDKs.

## 13. Milestones and acceptance criteria

Build in order. A milestone is done only when every acceptance criterion passes in CI.

### M0 — Foundation

- [x] Cargo workspace with every crate from section 6 and pnpm workspace with every browser package, each compiling
- [x] `rustfmt`, `clippy`, `cargo-deny` config; shared `tsconfig`, ESLint, Prettier, Vitest; `release-plz` and Changesets initialised
- [x] `cargo xtask gen-types` generating `packages/types` with `ts-rs`, plus a CI staleness check
- [x] GitHub Actions on Ubuntu, Windows and macOS: fmt, clippy, test, `wasm32-unknown-unknown` build of engine crates, `cargo-deny`, browser lint/test
- [x] `LICENSE` (Apache-2.0), `README.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `AGENTS.md` skeleton with build/test commands, issue and PR templates
- [x] `hardless-core` Device trait, registry, event log, faults (FR-01–FR-03, FR-05)

**Accept:** `cargo test --workspace`, `cargo build -p hardless-wasm --target wasm32-unknown-unknown` and `pnpm -r test` pass on all three OSes; a sample device in `hardless-core`'s tests receives bytes and emits `DeviceEvent`s driven by a fake clock.

### M1 — Printer and drawer over Web Serial (first end-to-end path)

- [ ] `hardless-escpos` streaming parser with the full phase 1 command set (FR-06–FR-09)
- [ ] Golden fixtures in `fixtures/escpos/` (at least 30 cases, including commands split across chunks)
- [ ] `hardless-render` HTML + `to_text` (FR-10, FR-12, FR-13); SVG/PNG may follow in M2
- [ ] `hardless-devices::printer` and `hardless-devices::drawer` (FR-36–FR-39)
- [ ] `shim-web` Web Serial with picker, grants, streams, events and errors (FR-14–FR-17, section 8.1)
- [ ] `server` bridge + REST endpoints for devices, jobs, faults (FR-18, FR-20, FR-21)
- [ ] Dashboard: device list, printer panel, drawer panel, fault panel (FR-23–FR-25, FR-28)
- [ ] Faults: `paper-out`, `cover-open`, `offline`, `disconnect-mid-write`, `slow-write`, `picker-cancel`, `jammed`
- [ ] `examples/react-pwa` prints a sample receipt and kicks the drawer via Web Serial

**Accept:** In Chrome, the example app selects the virtual port, prints, and the dashboard shows the rendered receipt within 500 ms and the drawer opening. Toggling `paper-out` makes the app's next print fail with a status the app can read. Shim spec-conformance tests for section 8.1 pass.

### M2 — Cross-browser paths

- [ ] HTTP printer endpoints, both dialects, CORS, HTTPS on 8443 (FR-31–FR-35)
- [ ] Keyboard-wedge scanner device, dashboard panel, presets, sequences (FR-26, section 8.4)
- [ ] Renderer SVG/PNG output (FR-11) and `/render` endpoint
- [ ] Scenario runner (FR-04) and `/scenarios/run`
- [ ] CLI (FR-30)
- [ ] Docker image `hardless/hardless` multi-arch (NFR-12)
- [ ] Remaining printer/HTTP/scanner faults from section 10

**Accept:** The example app prints from desktop Safari and iOS Safari (manual check, recorded in the PR) via the HTTPS endpoint after trusting the cert; the mixed-content case is reproduced and explained in the dashboard. `docker run -p 8080:8080 -p 8443:8443 hardless/hardless` serves a working dashboard. A scripted 20-barcode sequence arrives in order with the configured delay.

### M3 — WebUSB, Web NFC, agent interface and delivery

- [ ] `shim-web` WebUSB with printer-class device (section 8.2)
- [ ] `hardless-devices::nfc` + `NDEFReader` shim (section 8.3) + NFC dashboard panel (FR-27)
- [ ] Chrome MV3 extension with per-origin enable (FR-17, NFR-03)
- [ ] `@hardless/playwright` fixture and matchers (FR-40–FR-42)
- [ ] `hardless-mcp` with stdio and `/mcp` transports, all tools and resources in section 11.4 (FR-43–FR-50)
- [ ] `AGENTS.md` and `llms.txt` at repo root with a CI sync check (FR-51)
- [ ] Event log view (FR-29)
- [ ] Docs site in `docs/`: quickstart, device reference, fault reference, writing a device plugin, using Hardless from an AI agent

**Accept:** The example app's Playwright suite covers print, drawer, scan, NFC read/write and three fault cases, and runs green headless in CI with no server (standalone mode) and with the Docker server. The extension lets an unmodified copy of the example app print via the shim. Shim conformance tests for sections 8.2 and 8.3 pass. An MCP integration test (using the SDK's client) calls every tool against a live server, checks output schemas, and confirms `get_last_receipt` returns a PNG image block; `--read-only` exposes only read tools.

### Release v0.1.0

- [ ] `hardless` binaries on GitHub Releases with `cargo-dist` installers and Homebrew, `npx hardless` wrapper on npm, crates on crates.io, browser packages on npm under `@hardless/*`, Docker image pushed, extension zip attached to the release
- [ ] At least 10 issues labelled `good first issue` opened for new ESC/POS commands and code pages

## 14. Roadmap: phases 2–5

Each later phase reuses phase 1's core, plugin API, renderer, dashboard, REST API and MCP server; what changes is the transport that reaches the app. Each phase gets its own detailed spec (like sections 7–13) before work starts. A coding agent must not build anything in this section until that spec exists.

| Phase                | Target apps                                          | New transports                                              | New devices                                             | Gate to start             |
| -------------------- | ---------------------------------------------------- | ----------------------------------------------------------- | ------------------------------------------------------- | ------------------------- |
| 1 Browser/PWA        | Web apps, PWAs                                       | Web Serial, WebUSB, Web NFC, HTTP, keyboard wedge           | Receipt printer, drawer, scanner, NFC                   | —                         |
| 2 Desktop & Electron | Electron, .NET, Java, Node desktop POS               | Raw TCP 9100, virtual COM ports, IPP                        | Network printer, label printer, customer display, scale | v0.1.0 released           |
| 3 Smart cards & HID  | Desktop apps and web apps using PC/SC or WebHID      | PC/SC virtual reader, WebHID                                | NFC/smart-card reader, HID scanner, HID display         | Phase 2 transports stable |
| 4 Mobile & Bluetooth | Android/iOS native, React Native, Flutter, Capacitor | Web Bluetooth, BLE GATT, Bluetooth SPP, mobile SDK adapters | BLE/BT printers, mobile NFC                             | Phase 3 released          |
| 5 Real USB & replay  | Apps using vendor drivers or raw USB                 | USB/IP, record/replay proxy                                 | Any device as genuine USB; captured real devices        | Phase 4 released          |

### 14.1 Phase 2 — Desktop and Electron

- **Raw TCP 9100 printers.** ESC/POS over TCP with the `DLE EOT` status back-channel, multiple printers on different ports. This is the most common setup in real cafes and restaurants.
- **Virtual serial ports.** `hardless ports create` sets up paired ports: com0com on Windows (signed driver, one-time install), `socat` PTYs on Linux and macOS. The app opens `COM7` or `/dev/ttyHARDLESS0` as if a device were attached.
- **OS-level keyboard wedge.** Trusted key events from the OS input layer — `uinput` on Linux, `SendInput` on Windows, `CGEvent` on macOS — so scans reach native apps and web apps that check `isTrusted`. This removes the phase 1 limitation in section 8.4.
- **Virtual IPP printer.** An IPP Everywhere endpoint so OS printing, `window.print()` in kiosk mode and Electron's `webContents.print()` land in Hardless as rendered pages.
- **Label printers.** ZPL and TSPL over TCP 9100 with a label renderer (`renderer-label`) for shelf labels and barcodes.
- **Customer pole display** (VFD, 2×20 characters, common command sets) and **weighing scale** (continuous and request-response serial protocols) with dashboard panels to set the weight.
- **New MCP tools:** `set_weight`, `get_display_text`, `get_last_label`.
- **Exit:** an Electron example prints over TCP and over a virtual COM port, reads a scale and drives a pole display, in CI on Windows, macOS and Linux.

### 14.2 Phase 3 — Smart cards, desktop NFC and WebHID

- **PC/SC virtual reader** through vsmartcard's `vpcd`, so PC/SC libraries see a real reader. Support the common reader pseudo-APDUs (e.g. get UID).
- **Card models:** NTAG213/215/216 with NDEF, MIFARE Ultralight, MIFARE Classic 1K (sector reads with keys), and a scriptable APDU card for custom applets.
- **WebHID shim** (`navigator.hid`) for HID-mode barcode scanners (POS barcode usage page) and HID customer displays.
- **New MCP tools:** `insert_card`, `remove_card`, `send_apdu_script`.
- **Exit:** a Node app using a PC/SC library reads a virtual NTAG's UID and NDEF records; a web app reads a HID-mode scan through the shim.

### 14.3 Phase 4 — Mobile and Bluetooth

- **Web Bluetooth shim** (`navigator.bluetooth`) for BLE receipt printers, with configurable GATT service and characteristic UUIDs since these vary by vendor.
- **Real Bluetooth peripherals** from a Linux machine or Raspberry Pi using BlueZ: BLE GATT and classic SPP printers that a physical phone can pair with.
- **Android emulator:** apps reach Hardless's TCP and HTTP printers at `10.0.2.2`; documented setup only.
- **SDK adapters** for React Native, Flutter and Capacitor that replace NFC and Bluetooth plugin modules with Hardless-backed fakes in debug builds. This is the one place Hardless works at the SDK layer, because Android emulators and iOS do not allow NFC or Bluetooth emulation at the OS level; document that trade-off clearly.
- **Exit:** a React Native example reads a virtual NFC tag through the adapter, and a physical Android phone prints to a BLE printer emulated on Linux.

### 14.4 Phase 5 — Real USB, record/replay and ecosystem

- **USB/IP.** Export virtual devices (USB printer class, HID scanner, CDC-ACM serial) over USB/IP so the OS sees genuine USB hardware (Linux `vhci-hcd`, Windows usbip-win2). This allows testing apps that rely on vendor drivers.
- **USB gadget mode ("Hardless Box").** Run `hardless` on a Linux board with a USB device controller (for example a Raspberry Pi Zero 2 W or Pi 4) using configfs and FunctionFS. Plugged into a real POS terminal by cable, it enumerates as a genuine USB printer, HID scanner or serial device. With BlueZ (phase 4) on the same board, one device stands in for a whole counter's hardware.
- **Record and replay.** A proxy between an app and a real device (TCP or serial) captures traffic and device responses, saves them as a fixture, and replays them as a virtual device in CI.
- **More command sets:** Star Line Mode and StarPRNT, extra ESC/POS dialects and code pages (including Arabic/Urdu), kitchen printers.
- **Community devices:** a `hardless-device-*` naming convention for device plugins compiled to WebAssembly, loaded by the binary (via `wasmtime`) and by the browser shim, so community devices work without recompiling Hardless.
- **Exit:** a recorded session from a physical printer replays in CI with byte-identical responses, and a Hardless Box enumerates as a USB printer on Windows and prints a receipt from an unmodified POS app.

**Agent support across phases.** Generic MCP tools (`list_devices`, `set_fault`, `get_events`) work for every new device without changes. Each phase adds only device-specific tools, following FR-43 to FR-51.

## 15. Instructions for the coding agent

1. Work one milestone at a time, in order M0 → M3. Within a milestone, build bottom-up: `hardless-core` → `hardless-escpos` → `hardless-render` → `hardless-devices` → `hardless-server` + `dashboard` → `hardless-wasm` + `shim-web` → example app → `hardless-mcp`.
2. Before writing a crate or package, write its public types and a README usage example, then the tests, then the implementation.
3. Reference requirement IDs (FR-xx, NFR-xx) in commit messages and PR descriptions.
4. Do not change any public API, message format, endpoint path or fault ID defined in this PRD without stopping and asking. If the spec of a browser API contradicts this PRD, follow the spec and flag it.
5. Do not add dependencies outside section 6 without justification in the PR (maintenance status, licence against NFR-14).
6. Never copy code from vendor SDKs. Implement protocols from public command references and cite the reference in a code comment.
7. Keep engine crates sans-IO and building for `wasm32-unknown-unknown` (section 6 conventions); the CI wasm build enforces it.
8. Put all OS- and hardware-specific code in `hardless-transport-*` crates behind a common transport trait, with `cfg(target_os)` gates and a clear error on unsupported platforms.

**Testing rules**

- Every FR has at least one test that names its ID, e.g. Rust `fn fr_08_unknown_command_yields_unknown_node()` or TS `it('FR-08: …')`.
- Parsers: golden byte fixtures with `insta` snapshot ASTs, `proptest` properties, and a `cargo-fuzz` target per parser run nightly in CI; a parser must never panic on any input. Renderer: HTML/SVG snapshots plus `to_text` assertions.
- Shims: a conformance suite per API that runs in a real Chromium via Playwright, covering every row of the tables in section 8.
- Faults: one test per fault ID proving the observable effect in section 10.
- The device test suite runs natively and in WebAssembly (`wasm-bindgen-test`) to prove server and standalone modes behave identically.
- No test may depend on wall-clock time; devices receive `now` from the caller.

**Definition of done (per task)**

- [ ] `cargo fmt`, `clippy`, tests, and TS lint/typecheck pass locally and in CI on all three OSes
- [ ] New public APIs documented (rustdoc or TSDoc) and shown in the crate or package README
- [ ] Release note added (`release-plz` for crates, a changeset for npm packages)
- [ ] Dashboard changes work in light and dark themes
- [ ] Milestone checklist in section 13 updated

## 16. Open questions

- [ ] Secure the hardless npm scope and GitHub org, and run a trademark search for the name.
- [ ] Should the WebSerial shim emulate USB-serial adapters only, or also Bluetooth-serial ports (`bluetoothServiceClassId` in `getInfo()`)?
- [ ] Which additional code pages are needed first (e.g. CP864 for Arabic, Windows-1256 for Urdu receipts)?
- [ ] Should Web Bluetooth printers move from phase 4 into phase 2? BLE printers are common with tablet POS setups.
- [ ] Publish the extension on the Chrome Web Store at v0.1, or ship as an unpacked zip until v0.2?
