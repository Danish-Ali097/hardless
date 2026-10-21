# @hardless/dashboard

## 0.1.0

### Minor Changes

- 6c1b65a: Each receipt in the printer tray has a "Download PNG" link next to "Download HTML" (FR-24).
- 6bea71e: Event log view (FR-29): every device's log, newest first and live, filtered by device and by direction (app to device, device to app), with bytes as hex and text and every record in plain words.
- d3b3d85: HTTP printers (FR-33): the printer panel shows the ePOS-Print-compatible and WebPRNT-compatible addresses, explains mixed content and untrusted certificates, and offers the Hardless certificate with trust steps per platform. "Add device" can add an HTTP receipt printer.
- 822b62e: NFC panel (FR-27): the reader's cards with their records, Tap and Edit, a card editor (UID and text, URL, MIME or empty records), the app's last write, and the NFC reader in Add device.
- 962b4f3: Scanner panel (FR-26): scan a barcode, keep presets, and run a sequence of scans with a delay between them.
- e0acb09: Milestone M0: package skeletons, shared TypeScript/ESLint/Prettier/Vitest setup, and wire types generated from the Rust core.
- e4bc9c7: Milestone M1: the dashboard shows devices, the printer's receipt tray, the cash drawer and fault switches.

### Patch Changes

- b17cce8: Printers with the `paper-near-end` fault show "Paper low" in the device list.
- 27dc800: USB printers: "Receipt printer on WebUSB" in the printer panel, and "Receipt printer (WebUSB)" in Add device.
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
