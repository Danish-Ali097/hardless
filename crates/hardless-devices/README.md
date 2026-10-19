# hardless-devices

Emulated POS devices for [Hardless](https://github.com/hardlessdev/hardless), built on
`hardless-core`. Each device is a sans-IO state machine: it receives bytes, control commands
and the current time, and returns what it did. Each device is a Cargo feature.

| Kind             | Feature   | What it emulates                                                                                  |
| ---------------- | --------- | ------------------------------------------------------------------------------------------------- |
| `printer-escpos` | `printer` | ESC/POS receipt printer: parses jobs, answers `DLE EOT` status requests, kicks an attached drawer |
| `cash-drawer`    | `drawer`  | Cash drawer attached to a printer's drawer-kick connector (FR-36 to FR-39)                        |
| `scanner-wedge`  | `scanner` | Keyboard-wedge barcode scanner: types each scan into the page through the shim (FR-26)            |

## Usage

```rust
use hardless_core::{FakeClock, Registry};
use hardless_devices::register_all;
use serde_json::json;

let clock = FakeClock::new();
let mut registry = Registry::new(clock.clone());
register_all(&mut registry).unwrap();

let printer = registry.create("printer-escpos", Some("printer-1".parse().unwrap()), json!({})).unwrap();
let drawer = registry
    .create("cash-drawer", Some("drawer-1".parse().unwrap()), json!({ "attachedTo": "printer-1" }))
    .unwrap();

// Print a receipt, kick the drawer (pin 2, 50 ms), cut.
registry.host_bytes(&printer, b"Total: 1,250\n\x1bp\x00\x19\xfa\x1dV\x00").unwrap();
let job = registry.last_job(&printer).unwrap().unwrap();
assert!(job.text.contains("Total: 1,250"));

// The drawer opens once the kick pulse has finished.
clock.advance(50);
registry.tick();
assert_eq!(registry.summary(&drawer).unwrap().state["state"], "open");
```

## Printer (`printer-escpos`)

Config (all optional):

| Field                         | Default    | Meaning                                                                  |
| ----------------------------- | ---------- | ------------------------------------------------------------------------ |
| `transport`                   | `"serial"` | How the app reaches it. Only `serial` in M1; `usb` and `http` follow.    |
| `paperWidth`                  | `80`       | `58` or `80` mm; used for job text and rendering.                        |
| `idleMs`                      | `300`      | A job closes after this much silence, or on a cut.                       |
| `usbVendorId`, `usbProductId` | none       | Reported by `SerialPort.getInfo()` and matched by `requestPort` filters. |
| `autoSelect`                  | `false`    | The shim's port picker picks this device without asking (tests).         |
| `drawerOpenLevel`             | `"high"`   | Drawer sensor level that means open: `"high"` or `"low"` (bit 2 below).  |

Jobs close on a cut or after `idleMs` of silence. A job's `ast` is the list of commands it
contained and its `text` is the plain-text receipt (`hardless_render::to_text`). Commands that
arrive without anything printable (only settings) carry over into the next job, as on a real
printer, until `ESC @` resets them.

Real-time status (`DLE EOT n`) is answered immediately with the documented status byte:

| Request                   | Bits set                                                   |
| ------------------------- | ---------------------------------------------------------- |
| `DLE EOT 1` printer       | bit 2: pin 3 high (drawer open by default); bit 3: offline |
| `DLE EOT 2` offline cause | bit 2: cover open; bit 5: printing stopped by paper end    |
| `DLE EOT 3` error cause   | none                                                       |
| `DLE EOT 4` paper sensor  | bits 2-3: paper near end; bits 5-6: paper end              |

Bits 1 and 4 are always set, as on real printers.

Faults (PRD section 10): `paper-out` and `cover-open` reject prints and stop buffered drawer
kicks (real-time `DLE DC4` kicks still work); `offline` disconnects the printer;
`disconnect-mid-write` disconnects after `afterBytes` (default 64) bytes; `paper-near-end`
reports near end in `DLE EOT 4` while prints still work; `baud-mismatch` scrambles every
received byte (a fixed, repeatable scramble, so status requests go unanswered); `garbled` flips
one bit in a byte with probability `rate` (default 0.05), from a generator seeded when the fault
is turned on, so runs under a fake clock repeat exactly. `slow-write` and `picker-cancel` are
applied by the transport and the browser shim. The event log always shows the bytes the app
sent; a job's `bytesHex` shows what the printer received.

## Cash drawer (`cash-drawer`)

Config: `attachedTo` (printer ID, required), `pin` (`2` or `5`, default `2`), `autoCloseMs`
(close automatically after this long; default off).

States: `closed` → `opening` (for the kick's on-time, at least 50 ms) → `open` → `closed`
(by hand, or after `autoCloseMs`). Faults: `jammed` (kick counted, drawer stays closed) and
`not-connected` (kick ignored; the printer sees no open drawer).

## Barcode scanner (`scanner-wedge`)

A wedge scanner is a keyboard to the computer: for each barcode it types the characters a few
milliseconds apart, then a terminator key. A scan is a `ControlCommand::Scan` (from the
dashboard, `POST /devices/:id/scan`, the CLI or a scenario); the scanner answers with a
`DeviceEvent::Wedge` carrying `{ code, delayMs, terminator }`, which the transport hands to the
shim to type.

Config (all optional):

| Field        | Default   | Meaning                                             |
| ------------ | --------- | --------------------------------------------------- |
| `terminator` | `"enter"` | Key after each code: `"enter"`, `"tab"` or `"none"` |
| `delayMs`    | `8`       | Milliseconds between key presses                    |
| `prefix`     | `""`      | Typed before every code                             |
| `suffix`     | `""`      | Typed after every code, before the terminator       |

A scan can override `terminator` and `delayMs`. Empty codes and codes over 4,096 characters are
ignored with a warning. Faults: `double-scan` repeats each scan `gapMs` (default 30) after its
last key, and `partial-scan` drops the terminator.

## License

Apache-2.0
