# Decisions and deviations

Choices made while implementing the PRD that the PRD does not spell out, and the places where
the implementation differs from it. Newest milestone first. Items marked **open** need a
decision from the maintainer.

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
