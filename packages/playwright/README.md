# @hardless/playwright

A [Playwright](https://playwright.dev) fixture and matchers for testing POS hardware flows
against [Hardless](https://github.com/hardlessdev/hardless) emulated devices: the fixture
installs the Hardless shim in every page before your app's code runs (FR-40), gives the test
handles for the devices (FR-42), and adds matchers that poll like Playwright's own (FR-41).
Your app needs no Hardless code: test a production build.

```ts
import { test, expect } from "@hardless/playwright";

test.use({
  hardless: {
    mode: "standalone", // or { mode: "server", url: "ws://localhost:8080/bridge" }
    devices: [
      { kind: "printer-escpos", id: "printer-1", transport: "serial", autoSelect: true },
      { kind: "cash-drawer", id: "drawer-1", attachedTo: "printer-1" },
      { kind: "scanner-wedge", id: "scanner-1" },
      { kind: "nfc-reader", id: "nfc-1" },
    ],
  },
});

test("cash sale prints and opens drawer", async ({ page, hardless }) => {
  await page.goto("/pos");
  await hardless.scanner("scanner-1").scan("8964000123456");
  await page.getByRole("button", { name: "Pay cash" }).click();
  await expect(hardless.printer("printer-1")).toHavePrinted(/Total:\s+1,250/);
  await expect(hardless.drawer("drawer-1")).toHaveOpened({ times: 1 });
});
```

## Modes

- **Standalone** (default): the devices run in the page, with the Hardless engine compiled to
  WebAssembly; no server. The fixture serves the engine to the page from
  `/__hardless__/` on the app's own origin. The devices start fresh on every navigation, so
  print after `page.goto`, not before.
- **Server**: the devices live in a running Hardless server (`hardless start`, Docker), shared
  with its dashboard. `url` is the bridge URL; `token` if the server has one. Devices listed in
  `devices` are created if missing, and they (or, without a list, every device) are reset
  before each test unless `reset: false`.

Without options the fixture runs standalone with `printer-1` (Web Serial), `drawer-1`,
`usb-printer-1` (WebUSB), `scanner-1` and `nfc-1`. `serial`, `usb`, `nfc` and `wedge` turn
individual shims off. In `playwright.config`, set options per project with
`defineConfig<HardlessTestOptions>({ use: { hardlessOptions: { ... } } })`.

## Devices (FR-42)

| Handle                 | Methods                                                                   |
| ---------------------- | ------------------------------------------------------------------------- |
| `hardless.printer(id)` | `jobs()`, `lastJob()`                                                     |
| `hardless.drawer(id)`  | `close()`                                                                 |
| `hardless.scanner(id)` | `scan(code, { terminator?, delayMs? })`: resolves once the keys are typed |
| `hardless.nfc(id)`     | `tap(uid, records?)`, `lastWritten()`                                     |
| every handle           | `state()`, `fault(id, options?)`, `clearFault(id)`, `reset()`, `events()` |

## Matchers (FR-41)

`toHavePrinted(textOrRegex)`, `toHavePrintedJobs(n)`, `toHaveCut()` on printers;
`toHaveOpened({ times? })`, `toBeClosed()` on drawers; `toHaveWrittenNdef(records)` on NFC
readers (each expected record's given fields must match the app's latest write). All of them
poll until they pass or the `expect` timeout ends, and work with `.not`.

## Requirements

`@playwright/test` 1.50 or later, and Chromium for Web Serial and WebUSB flows (Web NFC and
the keyboard wedge work in every browser through the shim).

## License

Apache-2.0
