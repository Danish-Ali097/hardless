---
"@hardless/playwright": minor
---

The Playwright fixture (FR-40 to FR-42): `test.use({ hardless: { mode, devices } })` installs the shim with `page.addInitScript` before the app runs, in standalone mode (engine served from `/__hardless__/`) or server mode; `hardless.printer/drawer/scanner/nfc(id)` handles with fault helpers; and the matchers `toHavePrinted`, `toHavePrintedJobs`, `toHaveCut`, `toHaveOpened`, `toBeClosed` and `toHaveWrittenNdef`.
