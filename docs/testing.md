# Testing with Playwright

[`@hardless/playwright`](../packages/playwright) is a Playwright fixture: it puts the
Hardless shim in every page before your app's code runs, gives the test handles for the
devices, and adds matchers that wait like Playwright's own. Test a production build of your
app: it needs no Hardless code.

```sh
npm install --save-dev @hardless/playwright @playwright/test
```

```ts
import { test, expect } from "@hardless/playwright";

test.use({
  hardless: {
    mode: "standalone",
    devices: [
      { kind: "printer-escpos", id: "printer-1", transport: "serial", autoSelect: true },
      { kind: "cash-drawer", id: "drawer-1", attachedTo: "printer-1" },
      { kind: "scanner-wedge", id: "scanner-1" },
      { kind: "nfc-reader", id: "nfc-1" },
    ],
  },
});

test("cash sale prints and opens the drawer", async ({ page, hardless }) => {
  await page.goto("/pos");
  await hardless.scanner("scanner-1").scan("8964000123456");
  await page.getByRole("button", { name: "Pay cash" }).click();
  await expect(hardless.printer("printer-1")).toHavePrinted(/Total:\s+1,250/);
  await expect(hardless.printer("printer-1")).toHaveCut();
  await expect(hardless.drawer("drawer-1")).toHaveOpened({ times: 1 });
});

test("paper out shows an error", async ({ page, hardless }) => {
  await page.goto("/pos");
  await hardless.printer("printer-1").fault("paper-out");
  await page.getByRole("button", { name: "Pay card" }).click();
  await expect(page.getByRole("alert")).toContainText("out of paper");
});
```

## Two modes

- **Standalone** (`mode: "standalone"`): the devices run inside the page, compiled to
  WebAssembly. No server, so it suits CI. The devices start fresh on every page load.
- **Server** (`mode: "server", url: "ws://localhost:8080/bridge"`): the devices live in a
  running Hardless server or Docker container, shared with its dashboard. The fixture creates
  any devices the test lists that the server lacks, and resets them before each test.

`autoSelect: true` on a printer skips the port picker; without it, the test clicks the
picker like a user (`hardless-port-picker [data-device-id="printer-1"]`).

## Handles and matchers

| Handle                 | Methods                                                                   | Matchers                                                     |
| ---------------------- | ------------------------------------------------------------------------- | ------------------------------------------------------------ |
| `hardless.printer(id)` | `jobs()`, `lastJob()`                                                     | `toHavePrinted(text)`, `toHavePrintedJobs(n)`, `toHaveCut()` |
| `hardless.drawer(id)`  | `close()`                                                                 | `toHaveOpened({ times })`, `toBeClosed()`                    |
| `hardless.scanner(id)` | `scan(code)` (resolves once typed)                                        |                                                              |
| `hardless.nfc(id)`     | `tap(uid, records?)`, `lastWritten()`                                     | `toHaveWrittenNdef(records)`                                 |
| every handle           | `fault(id, options?)`, `clearFault(id)`, `state()`, `reset()`, `events()` |                                                              |

The example app in [`examples/react-pwa`](../examples/react-pwa) has a complete suite
(`e2e/m3.spec.ts`) that runs in both modes.
