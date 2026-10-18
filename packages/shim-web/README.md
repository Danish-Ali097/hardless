# @hardless/shim-web

Browser API shims backed by Hardless emulated devices: `navigator.serial` (Web Serial),
`navigator.usb` (WebUSB), `NDEFReader` (Web NFC) and a keyboard-wedge barcode scanner.

> Status: package skeleton (milestone M0). The Web Serial shim lands in milestone M1.

## Planned usage (milestone M1)

```ts
import { install } from "@hardless/shim-web";

install({ transport: { mode: "server", url: "ws://localhost:8080/bridge" } });

// The app's normal Web Serial code now talks to the emulated printer.
const port = await navigator.serial.requestPort();
```
