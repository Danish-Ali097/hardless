# @hardless/dashboard

The Hardless dashboard: device list, rendered receipts, cash drawer state, fault toggles and
the event log. React 18 + Vite + Tailwind. `hardless-server` serves the built files at `/`.

| Panel                   | What it does                                                                                      |
| ----------------------- | ------------------------------------------------------------------------------------------------- |
| Devices (FR-23)         | Every device with its live status and active faults; add and remove devices                       |
| Receipt printer (FR-24) | Receipts newest first at 58 or 80 mm, raw bytes, PNG and HTML downloads, clear; HTTP printer help |
| Cash drawer (FR-25)     | Open/closed animation, kick counter, manual close                                                 |
| Barcode scanner (FR-26) | Scan box, saved presets, sequence runner                                                          |
| NFC reader (FR-27)      | Remembered cards with Tap and Edit, a card editor (UID and records), the app's last write         |
| Faults (FR-28)          | A toggle for every fault the device supports, with its options                                    |
| Event log (FR-29)       | Every device's log, newest first and live, filtered by device and direction                       |

```sh
pnpm --filter @hardless/dashboard dev     # dev server
pnpm --filter @hardless/dashboard build   # writes dist/ (and dist/THIRD_PARTY_LICENSES.txt)
```

## License

Apache-2.0
