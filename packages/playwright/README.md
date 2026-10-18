# @hardless/playwright

Playwright fixture and matchers for asserting on emulated POS hardware.

> Status: package skeleton (milestone M0). The fixture and matchers land in milestone M3.

## Planned usage (milestone M3)

```ts
import { test, expect } from "@hardless/playwright";

test("cash sale prints and opens drawer", async ({ page, hardless }) => {
  await page.goto("/pos");
  await page.getByRole("button", { name: "Pay cash" }).click();
  await expect(hardless.printer("printer-1")).toHavePrinted(/Total:\s+1,250/);
  await expect(hardless.drawer("drawer-1")).toHaveOpened({ times: 1 });
});
```
