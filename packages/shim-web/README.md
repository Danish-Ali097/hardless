# @hardless/shim-web

Browser API shims backed by [Hardless](https://github.com/hardlessdev/hardless) emulated
devices. Your app's own hardware code runs unchanged: it calls `navigator.serial` as usual and
talks to an emulated receipt printer and cash drawer.

> Status: milestone M1. `navigator.serial` (Web Serial) is shimmed. WebUSB, Web NFC and the
> keyboard-wedge scanner arrive in M2 and M3.

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

| Option                          | Mode       | Meaning                                                                    |
| ------------------------------- | ---------- | -------------------------------------------------------------------------- |
| `mode`                          | both       | `"server"` or `"standalone"` (FR-15)                                       |
| `url`                           | server     | Bridge URL, default `ws://localhost:8080/bridge`                           |
| `token`                         | server     | Token if the server was started with `--token` (NFR-02)                    |
| `devices`                       | standalone | Devices to emulate, as `{ kind, id, ...config }` or `{ kind, id, config }` |
| `wasmUrl`                       | standalone | Where to load the engine from, default `../wasm/hardless_wasm.js`          |
| `serial`, `usb`, `nfc`, `wedge` | both       | Turn individual shims off (FR-14); all default to on                       |
| `allowAnyOrigin`                | both       | Silence the warning on non-localhost origins (NFR-03)                      |

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

## Building

```sh
pnpm --filter @hardless/shim-web build   # dist/ (ESM per file, plus dist/hardless-shim.js)
cargo xtask build-wasm                   # wasm/ (the standalone engine)
pnpm --filter @hardless/shim-web test            # unit tests
pnpm --filter @hardless/shim-web test:browser    # conformance tests in Chromium
```
