# @hardless/shim-web

Browser API shims backed by [Hardless](https://github.com/hardlessdev/hardless) emulated
devices. Your app's own hardware code runs unchanged: it calls `navigator.serial`,
`navigator.usb` or `NDEFReader` as usual and talks to an emulated receipt printer, cash drawer
or NFC reader.

> Status: milestone M3. `navigator.serial` (Web Serial), `navigator.usb` (WebUSB),
> `NDEFReader` (Web NFC) and the keyboard-wedge scanner work.

## Usage

Install the shim before your app code runs, in development builds only:

```ts
import { install } from "@hardless/shim-web";

if (import.meta.env.DEV) {
  // Talk to a running `hardless start` (dashboard at http://localhost:8080).
  install({ mode: "server", url: "ws://localhost:8080/bridge" });
}
```

Or run the devices inside the page, without a server, for unit and Playwright tests. The
engine is the same Rust code as the server, compiled to WebAssembly and loaded only in this
mode:

```ts
install({
  mode: "standalone",
  devices: [
    { kind: "printer-escpos", id: "printer-1", transport: "serial", autoSelect: true },
    { kind: "cash-drawer", id: "drawer-1", attachedTo: "printer-1" },
  ],
});
```

Then use Web Serial as usual:

```ts
const port = await navigator.serial.requestPort(); // shows the Hardless port picker
await port.open({ baudRate: 9600 });
const writer = port.writable.getWriter();
await writer.write(new TextEncoder().encode("Total: 1,250\n\x1dV\x00"));
```

## Options

| Option                          | Mode       | Meaning                                                                                                |
| ------------------------------- | ---------- | ------------------------------------------------------------------------------------------------------ |
| `mode`                          | both       | `"server"` or `"standalone"` (FR-15)                                                                   |
| `url`                           | server     | Bridge URL, default `ws://localhost:8080/bridge`; from an https:// page use `wss://<host>:8443/bridge` |
| `token`                         | server     | Token if the server was started with `--token` (NFR-02)                                                |
| `devices`                       | standalone | Devices to emulate, as `{ kind, id, ...config }` or `{ kind, id, config }`                             |
| `wasmUrl`                       | standalone | Where to load the engine from, default `../wasm/hardless_wasm.js`                                      |
| `serial`, `usb`, `nfc`, `wedge` | both       | Turn individual shims off (FR-14); all default to on                                                   |
| `allowAnyOrigin`                | both       | Silence the warning on non-localhost origins (NFR-03)                                                  |

`install()` is idempotent (FR-17); `uninstall()` restores the browser's own APIs.

## Behaviour (PRD section 8.1)

The shim follows the WICG Web Serial specification and Chrome's behaviour:

- `requestPort()` needs a user gesture (`SecurityError` otherwise), validates filters
  (`TypeError`), and shows a picker of matching emulated ports; Cancel or Escape rejects with
  `NotFoundError`. Devices with `autoSelect: true` skip the picker.
- Grants persist for the tab (`sessionStorage` key `hardless:grants`), so `getPorts()` finds
  them after a reload.
- `open()` validates options like WebIDL and the spec (`TypeError`), and rejects with
  `InvalidStateError` if already open and `NetworkError` if the device is unplugged.
- `readable` is a byte stream and `writable` applies back-pressure. If the device goes away,
  both fail with `NetworkError`, and `disconnect` fires on the port and on `navigator.serial`.
- `close()` rejects with `TypeError` while a stream is locked, as Chrome does.
- `setSignals()` changes are recorded in the device's event log.
- Faults: `slow-write` delays write acknowledgements, `disconnect-mid-write` fails the
  in-flight write, and `picker-cancel` makes the next `requestPort()` behave like Cancel.

## Keyboard wedge (PRD section 8.4)

With the wedge on (the default), scans from an emulated `scanner-wedge` are typed into the
focused element, like a USB barcode scanner, one scan after another:

- Each character fires `keydown`, `keypress`, `beforeinput` and `input` (in text fields, which
  also receive the character, in a way React notices) and `keyup`. `preventDefault()` stops
  the character, as with real typing. Legacy `keyCode`, `which` and `charCode` are set for a US
  layout, because many barcode listeners still read them.
- Enter submits the field's form (HTML implicit submission) or adds a line in a textarea; Tab
  moves focus to the next field.
- With nothing focused, keys go to `document.body`.

Scan from the dashboard, `POST /api/v1/devices/<id>/scan`, or from the page:

```ts
const shim = install({ mode: "standalone", devices: [{ kind: "scanner-wedge", id: "scanner-1" }] });
await shim.scan("scanner-1", "4006381333931"); // standalone: resolves once typed
```

**Limitation:** the events are synthetic, so `event.isTrusted` is `false`. Apps that ignore
untrusted key events cannot be tested with the wedge in phase 1; OS-level key injection comes
in phase 2.

## WebUSB (PRD section 8.2)

Printers with `transport: "usb"` appear to `navigator.usb` as USB printer-class devices: one
configuration, one interface (class 7, subclass 1, protocol 2), bulk OUT endpoint 1 and bulk
IN endpoint 2.

```ts
const device = await navigator.usb.requestDevice({ filters: [{ classCode: 7 }] });
await device.open();
if (device.configuration === null) await device.selectConfiguration(1);
await device.claimInterface(0);
await device.transferOut(1, new TextEncoder().encode("Total: 1,250\n\x1dV\x00"));
```

- `requestDevice()` needs a user gesture (`SecurityError`), requires `filters` (`TypeError`;
  an empty list matches every USB printer), checks filters and `exclusionFilters` like the
  spec, and shows the Hardless picker; Cancel rejects with `NotFoundError`. Grants persist for
  the tab (`sessionStorage` key `hardless:usb-grants`) for `getDevices()`.
- The device is configured when plugged in (`configuration` is set), as a printer is on a real
  computer; `selectConfiguration(1)` also works. Order is enforced: everything needs `open()`
  (`InvalidStateError`) and transfers need a claimed interface (`NotFoundError`).
- `transferOut(1, data)` prints and resolves `{ bytesWritten, status: "ok" }`; `transferIn(2,
length)` waits for the printer's replies (`DLE EOT` status). `controlTransferIn` answers the
  printer-class `GET_DEVICE_ID` and `GET_PORT_STATUS` requests and stalls others.
- `connect` and `disconnect` fire on `navigator.usb` with a `USBConnectionEvent`; after a
  disconnect, calls reject with `NotFoundError`, and a reconnect brings a new `USBDevice`.
- Faults: `stall` makes `transferOut` return `status: "stall"`, `interface-busy` makes
  `claimInterface` reject with `NetworkError`, and `picker-cancel`, `slow-write`, `offline`
  and `disconnect-mid-write` work as for serial printers.

## Web NFC (PRD section 8.3)

With the NFC shim on (the default), `NDEFReader`, `NDEFReadingEvent`, `NDEFMessage` and
`NDEFRecord` exist in every browser, backed by emulated `nfc-reader` devices:

```ts
const reader = new NDEFReader();
await reader.scan(); // needs a user gesture the first time, like the permission prompt
reader.onreading = ({ serialNumber, message }) => {
  const text = new TextDecoder().decode(message.records[0].data); // "Member 1234"
};
await reader.write("Visit 2"); // writes to the card on the reader, or waits for the next tap
```

Tap cards from the dashboard, `POST /api/v1/devices/<id>/tap`, `hardless tap`, or the page:

```ts
await shim.tap("nfc-1", "04a2b3c4", [{ recordType: "text", data: "Member 1234" }]);
```

- `scan()` needs transient user activation until the page has been granted NFC
  (`SecurityError`), rejects with `InvalidStateError` while that reader is already scanning,
  and `NotSupportedError` when no `nfc-reader` is emulated. Its `signal` stops reading.
- `reading` carries `serialNumber` (`04:a2:b3:c4`) and `NDEFRecord`s with `recordType`,
  `mediaType`, `id`, `data` (a `DataView`), `encoding` and `lang`. Every scanning
  `NDEFReader` on the page hears every reader, like a phone's single NFC antenna.
- `write()` accepts a string, bytes or an `NDEFMessageInit`, checks records like the spec
  (`TypeError`), and writes to the card on a reader, or waits for the next tap. A newer
  `write()` aborts a pending one. `overwrite: false` on a card with data, or any write to a
  read-only card, rejects with `NotAllowedError`. `makeReadOnly()` works the same way.
- Faults: `permission-denied` makes `scan()`, `write()` and `makeReadOnly()` reject with
  `NotAllowedError`; `picker-cancel` does so once (a dismissed prompt); `read-error` makes
  the next tap fire `readingerror`.
- Not emulated yet: nested messages (`smart-poster` records, local types). They reject with
  `NotSupportedError`.

## Building

```sh
pnpm --filter @hardless/shim-web build   # dist/ (ESM per file, plus dist/hardless-shim.js)
cargo xtask build-wasm                   # wasm/ (the standalone engine)
pnpm --filter @hardless/shim-web test            # unit tests
pnpm --filter @hardless/shim-web test:browser    # conformance tests in Chromium
```
