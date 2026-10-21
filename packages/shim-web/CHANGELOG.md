# @hardless/shim-web

## 0.1.0

### Minor Changes

- e0acb09: Milestone M0: package skeletons, shared TypeScript/ESLint/Prettier/Vitest setup, and wire types generated from the Rust core.
- 62f0a2f: A classic-script build (`@hardless/shim-web/iife`, defining `HardlessShim`) for init scripts; the standalone transport gains `jobs()`, `events()` and `reset()`; `WedgeInjector.typed` counts scans typed.
- 183c277: Web NFC (PRD section 8.3): `NDEFReader`, `NDEFReadingEvent`, `NDEFMessage` and `NDEFRecord` backed by emulated `nfc-reader` devices, in server and standalone mode. `scan()`, `reading` / `readingerror`, `write()` (waits for a tap when no card is on the reader) and `makeReadOnly()`, with the `permission-denied`, `picker-cancel` and `read-error` faults. New `installation.tap()`.
- 545ce56: Web Serial shim: `install({ mode: "server" | "standalone" })` replaces `navigator.serial` with an emulated port picker, ports, streams, events and errors that follow the Web Serial spec. Adds the `SerialSignals` type and the bridge `signals` message.
- 27dc800: WebUSB (PRD section 8.2): `navigator.usb` with `requestDevice()` (filters, exclusion filters, picker, `picker-cancel`), `getDevices()`, `connect` / `disconnect` events, and `USBDevice` for emulated USB printers: printer-class descriptors, enforced `open` / `claimInterface` order, `transferOut`, `transferIn`, the `GET_DEVICE_ID` and `GET_PORT_STATUS` control requests, and the `stall` and `interface-busy` faults.
- 962b4f3: Keyboard wedge (PRD section 8.4): scans from an emulated `scanner-wedge` are typed into the focused element with key and input events, Enter submits forms and Tab moves focus. New `installation.scan()` and `WedgeInjector`.

### Patch Changes

- 5f4fb86: Each package ships its LICENSE; `@hardless/shim-web` also ships `THIRD_PARTY_LICENSES.txt` for the Rust crates in its WebAssembly engine.
- Updated dependencies [3d6e084]
- Updated dependencies [a1d8cd1]
- Updated dependencies [e0acb09]
- Updated dependencies [3130549]
- Updated dependencies [5f4fb86]
- Updated dependencies [703cc61]
- Updated dependencies [6736537]
- Updated dependencies [545ce56]
- Updated dependencies [87b1a40]
- Updated dependencies [962b4f3]
  - @hardless/types@0.1.0
