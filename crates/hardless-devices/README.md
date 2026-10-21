# hardless-devices

Emulated POS devices for [Hardless](https://github.com/hardlessdev/hardless), built on
`hardless-core`. Each device is a sans-IO state machine: it receives bytes, control commands
and the current time, and returns what it did. Each device is a Cargo feature.

| Kind             | Feature   | What it emulates                                                                                  |
| ---------------- | --------- | ------------------------------------------------------------------------------------------------- |
| `printer-escpos` | `printer` | ESC/POS receipt printer: parses jobs, answers `DLE EOT` status requests, kicks an attached drawer |
| `cash-drawer`    | `drawer`  | Cash drawer attached to a printer's drawer-kick connector (FR-36 to FR-39)                        |
| `scanner-wedge`  | `scanner` | Keyboard-wedge barcode scanner: types each scan into the page through the shim (FR-26)            |
| `nfc-reader`     | `nfc`     | NFC reader with virtual cards: taps are read by `NDEFReader`, and the app can write cards (FR-27) |

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

| Field                         | Default    | Meaning                                                                 |
| ----------------------------- | ---------- | ----------------------------------------------------------------------- |
| `transport`                   | `"serial"` | How the app reaches it: `serial`, `usb` (WebUSB) or `http`.             |
| `paperWidth`                  | `80`       | `58` or `80` mm; used for job text and rendering.                       |
| `idleMs`                      | `300`      | A job closes after this much silence, or on a cut.                      |
| `usbVendorId`, `usbProductId` | none       | Reported by `getInfo()` / `USBDevice`, matched by picker filters.       |
| `autoSelect`                  | `false`    | The shim's port picker picks this device without asking (tests).        |
| `drawerOpenLevel`             | `"high"`   | Drawer sensor level that means open: `"high"` or `"low"` (bit 2 below). |

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

A USB printer (`transport: "usb"`) is a USB printer-class device: it reports `usbVendorId`
`0x1209` and `usbProductId` `0x0001` (pid.codes, the open-source vendor ID) unless the config
sets others, and its state carries `usb: { manufacturerName, productName, serialNumber,
ieee1284Id, portStatus }` for the shim. `manufacturerName`, `productName`, `serialNumber` and
`ieee1284Id` can be set in the config; `portStatus` is the printer-class `GET_PORT_STATUS` byte
(bit 3 no error, bit 4 selected, bit 5 paper empty).

Faults (PRD section 10): `paper-out` and `cover-open` reject prints and stop buffered drawer
kicks (real-time `DLE DC4` kicks still work); `offline` disconnects the printer;
`disconnect-mid-write` disconnects after `afterBytes` (default 64) bytes; `paper-near-end`
reports near end in `DLE EOT 4` while prints still work; `baud-mismatch` scrambles every
received byte (a fixed, repeatable scramble, so status requests go unanswered); `garbled` flips
one bit in a byte with probability `rate` (default 0.05), from a generator seeded when the fault
is turned on, so runs under a fake clock repeat exactly. `slow-write`, `picker-cancel`,
`stall` and `interface-busy` are applied by the transport and the browser shim. The event log always shows the bytes the app
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

## NFC reader (`nfc-reader`)

A phone's NFC reader sees a card while it is held against it. A tap (`ControlCommand::Tap`
`{ uid, records? }`, from the dashboard, `POST /devices/:id/tap`, the CLI or a scenario) puts a
card on the reader for `presenceMs` and reads it: the reader emits
`DeviceEvent::Nfc(NfcEvent::Reading { serialNumber, records })`, which the shim fires as
`NDEFReader`'s `reading` event. While the card is on the reader the app can write to it
(`ControlCommand::NfcWrite`, from `NDEFReader.write()`) or lock it (`NfcMakeReadOnly`, from
`makeReadOnly()`). Writes without a card are refused with `NfcRejection::NoCard` (the shim
waits for the next tap, as the spec says); writes to a read-only card, or to a card with data
when `overwrite` is false, are refused with `ReadOnly` / `NotEmpty` (`NotAllowedError`).

The reader remembers every card it has seen (up to 256), so a tap without `records` reads the
card as it was left, including what the app wrote. A tap with `records` is a card holding
exactly those records.

Records use Web NFC's `NDEFRecordInit` names: `recordType` (`empty`, `text`, `url`,
`absolute-url`, `mime`, `unknown` or an external type such as `example.com:loyalty`),
`mediaType`, `id`, `encoding`, `lang`, and the payload as text in `data` or as base64 in
`dataBase64`. UIDs are hex (`04a2b3c4` or `04:a2:b3:c4`); readings carry them as Web NFC's
`serialNumber` (`04:a2:b3:c4`).

Config (all optional): `presenceMs` (how long a tapped card stays on the reader, default 2000) and `cards` (`[{ uid, records, readOnly }]`, known from the start and restored on
reset).

Faults: `read-error` makes the next tap fire `readingerror` instead of `reading` and then
turns itself off; `permission-denied` and `picker-cancel` are applied by the shim
(`scan()` rejects with `NotAllowedError`; `picker-cancel` only once, like a dismissed
permission prompt).

## License

Apache-2.0
