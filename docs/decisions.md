# Decisions and deviations

Choices made while implementing the PRD that the PRD does not spell out, and the places where
the implementation differs from it. Newest milestone first. Items marked **open** need a
decision from the maintainer.

## M2 — Cross-browser paths

### Maintainer decisions taken before starting

- **Receipt font: Go Mono** (BSD-3-Clause, Bigelow & Holmes for the Go project). PNG output
  needs a bundled font because containers have none, and most free monospace fonts are SIL
  OFL-1.1, which is not on the NFR-14 list. Go Mono and Go Mono Bold (about 350 KB) live in
  `crates/hardless-render/fonts/` with their licence. Rejected: a hand-drawn pixel font (more
  work, pixelated) and adding OFL-1.1 to the policy.
- **New libraries approved:** `resvg` (Apache-2.0 OR MIT; PRD section 6 already names it) for
  PNG, `roxmltree` (MIT OR Apache-2.0) for the HTTP dialects' XML, and `rustls` with the
  `ring` crypto provider (`ring` is Apache-2.0 AND ISC) for HTTPS. rustls's default provider,
  `aws-lc-rs`, includes OpenSSL-licensed code, which is not on the NFR-14 list.
- **Pushing:** after each M2 piece passes the local checks, commit, push to `main` and watch
  CI, then carry on.
- **Docker image is built and tested in CI but not pushed** until the first release (v0.1.0);
  npm packages are likewise published only at release. The names are already reserved.

### Renderer (FR-11)

- **"Identical layout" means the same layout pass, not identical pixels.** HTML, SVG, PNG and
  `to_text` all come from one `layout()` step, so line breaks, columns, alignment, character
  sizes, codes and cuts match. Fonts differ between a browser and the bundled Go Mono, so
  pixels cannot.
- **SVG is in printer dots**: 576 printable dots across 80 mm and 384 across 58 mm, 18-dot
  side margins, 30-dot line pitch (the ESC/POS default line spacing). Each text run uses
  `textLength` with `lengthAdjust="spacingAndGlyphs"`, so runs fill exactly their columns in
  any monospace font. The SVG names Go Mono first but does not embed it (it would add 350 KB
  to every receipt).
- **PNG is behind the `png` feature** of `hardless-render`, so the WebAssembly engine stays
  small; the server enables it. One pixel per dot, white paper, Go Mono only (no system
  fonts, so PNGs are the same on every OS and in Docker).
- **PNG height limit: 20,000 dots** (about 2.5 m of paper). Longer receipts get a 400
  `receipt_too_long` from `/render?format=png` instead of a multi-hundred-megabyte bitmap.
  SVG and HTML have no limit.
- **`/render?format=png` rasterises on a blocking thread** (`spawn_blocking`) so a large
  receipt does not stall the async workers.
- **Notices:** `hardless_render::FONT_LICENSE` exports the font licence. The v0.1.0 notices
  step (see M0, Licences) must include it, alongside the Rust dependencies' notices.
- **Dashboard (FR-24):** "Download PNG" sits next to "Download HTML" on every receipt.

### Keyboard-wedge scanner (FR-26, section 8.4)

- **New `DeviceEvent::Wedge(WedgeScan)`** (maintainer decision; changes FR-01, additive). A
  real wedge scanner is a USB keyboard: it types each code key by key, then Enter or Tab. The
  scanner emits one `Wedge { code, delayMs, terminator }` per scan and the shim presses the
  keys, playing the computer's keyboard driver. This matches the PRD's existing `wedge-scan`
  bridge message one-to-one. Rejected: reusing `ToHost` bytes, which is how a _serial-mode_
  scanner works and loses the per-scan delay; and one message per key press, whose network
  jitter would break "a 20-barcode sequence arrives in order with the configured delay".
- **Event log:** scans are recorded as `wedge` records (`{ scan: { code, delayMs, terminator }
}`) with direction device-to-host, so the log filter (FR-29) treats them like status bytes.
- **`scanner-wedge` config:** `terminator` (default `enter`), `delayMs` (default 8),
  `prefix`, `suffix`. A scan may override `terminator` and `delayMs`. Empty codes and codes
  over 4,096 characters are ignored with a warning rather than typed.
- **`double-scan` timing:** the repeat starts `gapMs` after the first scan's _last_ key, i.e.
  at `(keys - 1) × delayMs + gapMs`. "Each scan is sent twice" with a gap only makes sense
  measured from the end of the first one. The repeat is a fault, so it does not count as a
  new scan in `scans`. Turning the fault off drops repeats not yet sent.
- **Who receives scans:** every bridge connection whose `hello` listed `wedge`, the way a
  keyboard types into whichever window has focus. Connections without `wedge` never get
  `wedge-scan` frames.
- **Default devices** now include `scanner-1`, so the dashboard has a scanner without config
  (the PRD's own example config in section 11.3 uses that ID).
- **Shim injector, where the spec leaves room:** legacy `keyCode`/`which`/`charCode` follow a
  US keyboard layout (barcode listeners commonly read them); characters with no US key get
  `keyCode` 0. Enter follows HTML implicit submission (click the default button; with none,
  submit only a single-field form). Tab moves to the next focusable element in tab order
  (positive `tabindex` first, then document order); it does not cross into iframes or shadow
  roots. Text is inserted with `setRangeText` so React's controlled inputs see the change.
  `isTrusted` is `false`, as section 8.4 says, documented in the shim README.
- **`installation.scan(deviceId, code, options?)`** in the shim (new public API) triggers a
  scan from the page in either mode: standalone resolves once the keys are typed; server mode
  calls `POST /devices/:id/scan` and resolves once the server has taken it. It exists so
  standalone-mode tests (and the M3 Playwright fixture) can scan without a server.
- **Dashboard sequence runner** runs in the page: it sends each scan, waits for the server to
  accept it, then waits the configured delay. Presets are kept in the browser's local storage
  (sample presets until the user saves their own); they are a convenience, not shared state.

## Between M1 and M2

- **Names secured (PRD section 16).** On 19 Oct 2026 the maintainer registered the npm
  organization `hardless` (scope `@hardless`), the Docker Hub account `hardless` (image
  `hardless/hardless`) and the GitHub organization `hardlessdev`, then moved the repository to
  <https://github.com/hardlessdev/hardless>. The GitHub name `hardless` belongs to a dormant
  personal account from 2013, and GitHub no longer releases dormant names on request, so the
  GitHub org differs from the npm scope. `hardlessdev` was picked because it is free on GitHub,
  npm and Docker Hub alike. Docker Hub uses a free personal account rather than an organization,
  which needs a paid plan; the image name is the same. The PRD's names for the npm packages and
  the Docker image are unchanged. Every repository URL now points at `hardlessdev/hardless`.
  The trademark search is still **open**.
- **Drawer sensor polarity is a printer setting** (maintainer decision, 19 Oct 2026; extends
  FR-38). `DLE EOT 1` bit 2 reports the level of connector pin 3, and real drawers are wired
  either way, so the meaning of the bit cannot be fixed. New optional printer config field
  `drawerOpenLevel`: `"high"` (default, unchanged M1 behaviour: bit 2 set means open) or
  `"low"` (bit 2 set means closed). The printer's `drawerOpen` state stays the logical value.
  An unplugged drawer (`not-connected`) still reads as closed, so with `"low"` it sets bit 2.
  The HTTP dialects' drawer status in M2 should apply the same setting.
- **Formatting carries over between receipts, as on real printers** (maintainer decision,
  19 Oct 2026). Settings such as bold or alignment sent without any printable text stay in
  effect for the next job, until `ESC @` resets them. This keeps the emulator honest: an app
  that forgets to reset formatting misbehaves here just as it would on a real printer. No code
  change; `printer_settings_without_printing_carry_into_the_next_job` pins the behaviour.

## M1 — Printer and drawer over Web Serial

### Maintainer decisions taken before starting

- **Printer and drawer connect through a peripheral link in `hardless-core`** (changes FR-01
  and FR-02). A device can name the device it is attached to; a device can emit a
  `PeripheralSignal` (the printer's drawer-kick pulse, the drawer's open sensor), and the
  registry delivers it between attached devices. This mirrors the real drawer-kick connector
  and keeps `cash-drawer` a separate device kind as in the PRD.
- **No PNG download in M1.** FR-24's "download PNG" button arrives with PNG rendering (FR-11)
  in M2. M1 offers the raw hex view and an HTML download.
- **`hardless start` ships in M1** (`--config`, `--port`, `--host`, `--token`), because FR-21
  needs CLI flags and the M1 acceptance test needs a running server. The other FR-30
  subcommands stay in M2.
- **New endpoint `GET /api/v1/stream`** (WebSocket): pushes event bus events
  (`device:added`, `device:removed`, `device:state`, `device:output`) to the dashboard.
  Section 5 says the dashboard receives jobs over a WebSocket, but the PRD defines none for it.
- **New endpoint `POST /api/v1/devices/:id/close`**: closes an open cash drawer (FR-25, FR-37),
  in the style of `/scan` and `/tap`.
- **New bridge message `signals`** (client to server): `{ deviceId, signals }` from
  `SerialPort.setSignals()`, recorded in the event log and acknowledged. Section 8.1 asks for
  DTR/RTS in the event log, but section 11.2 had no message to carry them.

### Where the Web Serial shim follows the spec instead of the PRD (section 8.1)

The PRD says to follow the WICG specification where they disagree. Checked against the spec
source (`WICG/serial`, `index.bs`) and Chrome:

- **`close()` with a locked stream rejects with `TypeError`, not `InvalidStateError`.** The
  spec's close() cancels the readable and aborts the writable; on a locked stream that fails,
  and Chrome reports `TypeError: Cannot cancel a locked stream`. Apps are written against
  Chrome, so the shim matches it, and rejects before changing anything.
- **`requestPort()` validates filters**: an empty filter, a `usbProductId` without
  `usbVendorId`, or Bluetooth and USB IDs together reject with `TypeError` (spec steps; not in
  the PRD).
- **`open()` validates every option**, not only `baudRate`: `dataBits` 7 or 8, `stopBits` 1 or
  2, `parity` and `flowControl` enum values, `bufferSize` above 0 and at most 16 MiB (Chrome's
  limit), all with WebIDL `[EnforceRange]` semantics.
- **`connect` and `disconnect` fire on the port and bubble to `navigator.serial`** (spec), with
  `event.target` being the port. `port.connected` reflects the state.
- **`getPorts()` returns only granted ports whose device is connected** ("available" in the
  spec). `forget()` removes the grant.
- **`getSignals()`** reports the inputs of a ready printer: CTS and DSR set, DCD and RI clear.
- After a disconnect, `readable` and `writable` become `null` but the port stays open until the
  app calls `close()`, as in the spec; it can then reopen once the device is back.

### Other M1 choices

- **`picker-cancel` is one-shot**, as PRD section 10 says ("next request"): the shim turns it
  off after using it, through `PUT /api/v1/devices/:id/faults/picker-cancel` in server mode.
  The REST API therefore allows CORS from local origins (any origin when a token is set).
- **Default devices.** Without a config file the server creates `printer-1` and `drawer-1`, so
  `hardless start` is enough for a first receipt.
- **Job closing.** Jobs close on a cut or after `idleMs` (default 300 ms). Status requests do
  not start a job. Settings sent without anything printable carry over into the next job.
- **Paper out and cover open** stop printing like a real printer: prints are dropped, buffered
  `ESC p` kicks are not executed, and real-time `DLE DC4` kicks still are. The app learns the
  cause through `DLE EOT` status, which is how ESC/POS apps detect it.
- **Drawer sensor**: `DLE EOT 1` bit 2 set means the drawer is open. Real drawers differ in
  polarity; this can become a printer setting if needed.
- **Code pages** come from the Windows definitions of CP437 and CP858.
- **Parser extras** beyond the phase 1 list (`CR`, `HT`, `ESC !`, `ESC M`, `GS h/w/H`,
  `ESC i/m`, `DLE DC4 1`), because apps send them constantly. Other known commands are
  skipped by length so their parameters are not printed.
- **Render endpoint** takes `unknown=0` to hide unknown-command badges (FR-12's toggle).
- **New dependencies**: `tokio`, `axum`, `qrcode`, `barcoders` (all named in section 6),
  `clap` for the CLI and `futures-util` for WebSocket handling (both MIT/Apache-2.0, widely
  used); `zustand` for the dashboard (section 6).
- **Sizes (NFR-08)**: `dist/hardless-shim.js` is 7.6 KiB gzipped (limit 30 KB); the
  WebAssembly engine is 169 KiB gzipped (limit 500 KB) and loads only in standalone mode.

## M0 — Foundation

### Toolchain

- **Rust 1.99 pinned** in `rust-toolchain.toml`; MSRV 1.97 (stable minus 2, NFR-11) in
  `rust-version` and checked by a separate CI job. Pinning keeps local and CI builds
  identical; bump both together.
- **Edition 2021** as the PRD says, although edition 2024 is available.
- **TypeScript 5.9** (`~5.9.3`): the PRD says TypeScript 5.x, so newer majors are not used.
- **pnpm 12.14.0** via the `packageManager` field.

### `hardless-core`

- **`std`, not `no_std + alloc`.** The PRD says "no_std + alloc where possible". The crate
  depends on `serde_json` and `schemars` in its public API (FR-01), and nothing in phase 1
  needs `no_std` (WebAssembly has `std`). Revisit for the phase 5 USB gadget if needed.
- **`bytes` dependency.** FR-01 uses `Bytes` in `DeviceEvent::ToHost`; the type comes from the
  `bytes` crate (MIT, tokio-rs, widely used, and already a dependency of axum). Re-exported as
  `hardless_core::Bytes`.
- **Job AST is JSON.** `Job.ast` is `serde_json::Value` so the core does not depend on any
  command set (ESC/POS now, ZPL/TSPL in phase 2). Devices fill it from their own typed AST.
- **Jobs are stored by the registry**, newest first, capped at 1,000 per device
  (`RegistryConfig::job_capacity`), so `GET /devices/:id/jobs` works the same for every
  printer route. Devices number their own jobs from 1.
- **Event bus is a drained queue.** Being sans-IO, the registry cannot push to subscribers;
  callers call `drain_bus()` after each operation. Undrained events are capped at 10,000.
- **Bus events are exactly the four in FR-02.** Device -> host bytes and connection changes
  are returned to the caller from every registry method instead of adding bus event types.
  Connection changes also publish `device:state`.
- **FR-01 changed (maintainer decision, M0): `set_fault` and `reset` take `now` and return
  `Vec<DeviceEvent>`**, like every other `Device` method. The original signatures gave a
  device no way to report what a fault caused (e.g. `offline` must disconnect immediately),
  which forced a hidden workaround: set a deadline of "now" and wait for the registry to tick.
  Device authors who forgot it would get faults that silently did nothing. The PRD's FR-01
  code block was updated. The registry still publishes `device:state` after every fault
  toggle and reset, because the active faults are part of the device summary.
- **Faults are open strings.** `FaultId` is a validated kebab-case string so plugins can add
  faults (goal 5); the section 10 IDs are constants and `catalogue()`.
- **FR-01/FR-02 changed (maintainer decision, M0): devices declare their faults.**
  `DevicePlugin::faults()` lists every fault a kind can support (for listing faults by kind,
  e.g. the MCP `list_faults` tool). `Device::faults()` lists the faults that apply to one
  instance, because applicability can depend on config: the same `printer-escpos` kind gets
  `baud-mismatch` over serial but `stall` over USB. `Registry::set_fault` rejects any other
  fault with `RegistryError::UnsupportedFault` (code `unsupported_fault`, HTTP 400 in M1),
  whether turning it on or off, and changes and records nothing. `DeviceSummary` gains
  `supportedFaults` so the dashboard fault panel (FR-28) shows only toggles that apply. Both
  return `Vec<FaultId>` so plugins can return their own fault IDs.
- **Device IDs** are 1–64 ASCII letters, digits, `-`, `_` and `.`, because they appear in URL
  paths. Generated IDs are `<kind>-<n>`.
- **Event log `seq`** is unique across all devices, so `?since=` works for merged views.
- **Event log records carry `deviceId`** and are JSON `{ seq, at, deviceId, type, ... }` with
  byte payloads as lowercase hex (`dataHex`), matching `bytesHex` in the REST job shape.
- **Monotonic time.** If the injected clock goes backwards, the registry keeps the last
  timestamp it used (FR-03 "monotonic timestamp").

### Wire types

- `cargo xtask gen-types` flattens ts-rs output into `packages/types/src`, rewrites imports
  to ESM (`./X.js`) and writes an `index.ts`. `u64` maps to `number` (all timestamps and
  sequence numbers fit in 2^53).
- Zod schemas (PRD section 6) are not generated yet; they are needed when the shim validates
  bridge messages in M1.

### Licences (NFR-14)

**Decided (maintainer, M0):** NFR-14 applies its strict list to what ships; build tools get a
slightly wider list; strong copyleft is banned everywhere. NFR-14 in the PRD was reworded to
say so. Why:

- `lightningcss` (MPL-2.0) is required by Vite 8 and Tailwind 4 (both PRD section 6). MPL-2.0
  is file-level weak copyleft: it only obliges sharing changes to its own files when they are
  distributed. It runs at build time and none of its code is in any output.
- `minimatch` (BlueOak-1.0.0) is required by ESLint. BlueOak-1.0.0 is an OSI-approved
  permissive licence (MIT-like, with a patent grant), so it joins the shipped list.
- The alternative, older Vite/Tailwind/ESLint versions, would trade a non-risk for an
  outdated toolchain.

How it is enforced:

- **One policy file**, `license-policy.json`: `shipped`, `buildToolsAlsoAllow` (only
  `MPL-2.0`) and `neverAllow` (GPL family, AGPL, SSPL, BUSL, non-commercial CC). Any licence
  not listed fails, so each new licence needs a decision here.
- **Rust**: `cargo-deny` applies the shipped list to every dependency, dev ones included
  (stricter than needed). Unpublished workspace crates are exempt (`licenses.private.ignore`).
- **npm declared dependencies**: `scripts/check-npm-licenses.mjs` (`pnpm run
check:licenses`) checks runtime `dependencies` against `shipped` and everything else against
  `shipped` + `buildToolsAlsoAllow`. It uses `pnpm licenses list` instead of the unmaintained
  `license-checker` package.
- **What is actually bundled**: Vite bundles whatever is imported, whichever dependency list
  it is in. The dashboard build (`packages/dashboard/build/licenses.ts`) checks every bundled
  package against `shipped` and fails the build otherwise.
- **Notices**: MIT, BSD, ISC and Apache-2.0 require their notices to accompany distributed
  copies. The dashboard build writes `THIRD_PARTY_LICENSES.txt` (full licence texts) and
  `third-party-licenses.json` into `dist/`. Today that is React, react-dom, scheduler,
  Tailwind's generated base CSS, and small Vite and rolldown runtime helpers, all MIT.
- **Follow-up for v0.1.0**: the `hardless` binary must also ship notices for its Rust
  dependencies (e.g. generated with `cargo-about` in the release workflow), and npm packages
  that bundle code must include theirs.

### Not in M0

- `cargo-fuzz` targets and the nightly fuzz job arrive with the first parser (M1).
- The device test suite under `wasm-bindgen-test` arrives with the first real device (M1).
- `llms.txt` and its CI sync check are FR-51 (M3).
