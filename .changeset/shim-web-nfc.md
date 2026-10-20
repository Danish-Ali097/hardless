---
"@hardless/shim-web": minor
---

Web NFC (PRD section 8.3): `NDEFReader`, `NDEFReadingEvent`, `NDEFMessage` and `NDEFRecord` backed by emulated `nfc-reader` devices, in server and standalone mode. `scan()`, `reading` / `readingerror`, `write()` (waits for a tap when no card is on the reader) and `makeReadOnly()`, with the `permission-denied`, `picker-cancel` and `read-error` faults. New `installation.tap()`.
