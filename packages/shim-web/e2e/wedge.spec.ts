import { expect, test, type Page } from "@playwright/test";

/**
 * Keyboard-wedge conformance (PRD section 8.4, FR-14, FR-26) in a real Chromium, in
 * standalone mode: scans come from the engine running in the page as WebAssembly.
 */

const SCANNER = { kind: "scanner-wedge", id: "scanner-1" };

async function setup(page: Page, scanner: object = SCANNER, html = ""): Promise<void> {
  await page.goto("/e2e/page.html");
  await page.waitForFunction(() => window.ready === true);
  await page.evaluate(
    async ({ scanner, html }) => {
      document.body.insertAdjacentHTML("beforeend", html);
      const installation = window.shim.install({
        mode: "standalone",
        devices: [scanner as { kind: string }],
      });
      await installation.transport.ready;
      window.installation = installation as unknown as Window["installation"];
      // Records every key and input event that reaches the document.
      window.log = [];
      for (const type of ["keydown", "keypress", "beforeinput", "input", "keyup"]) {
        document.addEventListener(
          type,
          (e) => {
            const k = e as KeyboardEvent;
            window.log.push({
              type,
              key: k.key,
              // Deprecated, but what many barcode listeners still read; the wedge sets them.
              // eslint-disable-next-line @typescript-eslint/no-deprecated
              keyCode: k.keyCode,
              // eslint-disable-next-line @typescript-eslint/no-deprecated
              which: k.which,
              trusted: e.isTrusted,
              target: (e.target as HTMLElement).id,
            });
          },
          true,
        );
      }
    },
    { scanner, html },
  );
}

const scan = (page: Page, code: string, options: object = {}) =>
  page.evaluate(([code, options]) => window.installation.scan("scanner-1", code, options), [
    code,
    options,
  ] as const);

test("8.4: types each character into the focused field, then Enter", async ({ page }) => {
  await setup(page, SCANNER, '<input id="field">');
  await page.focus("#field");
  await scan(page, "A1");
  await expect(page.locator("#field")).toHaveValue("A1");
  const log = await page.evaluate(() => window.log);
  expect(log.map((e) => `${e.type}:${e.key ?? ""}`)).toEqual([
    "keydown:A",
    "keypress:A",
    "beforeinput:",
    "input:",
    "keyup:A",
    "keydown:1",
    "keypress:1",
    "beforeinput:",
    "input:",
    "keyup:1",
    "keydown:Enter",
    "keypress:Enter",
    "keyup:Enter",
  ]);
  const keydowns = log.filter((e) => e.type === "keydown");
  expect(keydowns.map((e) => e.keyCode)).toEqual([65, 49, 13]);
  expect(keydowns.map((e) => e.which)).toEqual([65, 49, 13]);
  expect(log.every((e) => e.target === "field")).toBe(true);
});

test("8.4: synthetic key events are not trusted (documented limitation)", async ({ page }) => {
  await setup(page, SCANNER, '<input id="field">');
  await page.focus("#field");
  await scan(page, "1");
  const log = await page.evaluate(() => window.log);
  expect(log.some((e) => e.trusted)).toBe(false);
});

test("8.4: keys go to the body when nothing has focus", async ({ page }) => {
  await setup(page);
  await scan(page, "9");
  const log = await page.evaluate(() => window.log);
  expect(log.filter((e) => e.type === "keydown").map((e) => e.key)).toEqual(["9", "Enter"]);
});

test("8.4: preventDefault on keydown stops the character, like real typing", async ({ page }) => {
  await setup(page, SCANNER, '<input id="field">');
  await page.evaluate(() => {
    document.getElementById("field")?.addEventListener("keydown", (e) => {
      if (e.key === "x") e.preventDefault();
    });
  });
  await page.focus("#field");
  await scan(page, "axb", { terminator: "none" });
  await expect(page.locator("#field")).toHaveValue("ab");
});

test("8.4: Enter submits the form (implicit submission)", async ({ page }) => {
  await setup(
    page,
    SCANNER,
    '<form id="f"><input id="code" name="code"><button>Add</button></form>',
  );
  await page.evaluate(() => {
    window.submitted = [];
    document.getElementById("f")?.addEventListener("submit", (e) => {
      e.preventDefault();
      const input = document.getElementById("code") as HTMLInputElement;
      window.submitted.push(input.value);
      input.value = "";
    });
  });
  await page.focus("#code");
  await scan(page, "4006381333931");
  await scan(page, "8964000123456");
  expect(await page.evaluate(() => window.submitted)).toEqual(["4006381333931", "8964000123456"]);
});

test("8.4: Tab terminator moves focus to the next field", async ({ page }) => {
  await setup(
    page,
    { ...SCANNER, terminator: "tab" },
    '<input id="one"><input id="hidden" type="hidden"><input id="two">',
  );
  await page.focus("#one");
  await scan(page, "12");
  await expect(page.locator("#one")).toHaveValue("12");
  await expect(page.locator("#two")).toBeFocused();
});

test("8.4: Enter in a textarea adds a line", async ({ page }) => {
  await setup(page, SCANNER, '<textarea id="notes"></textarea>');
  await page.focus("#notes");
  await scan(page, "a");
  await scan(page, "b", { terminator: "none" });
  await expect(page.locator("#notes")).toHaveValue("a\nb");
});

test("8.4: prefix, suffix and inter-key delay come from the scanner", async ({ page }) => {
  await setup(page, { ...SCANNER, prefix: "<", suffix: ">", delayMs: 40 }, '<input id="field">');
  await page.focus("#field");
  const ms = await page.evaluate(async () => {
    const start = performance.now();
    await window.installation.scan("scanner-1", "12", { terminator: "none" });
    return performance.now() - start;
  });
  await expect(page.locator("#field")).toHaveValue("<12>");
  // 4 keys: 3 gaps of 40 ms.
  expect(ms).toBeGreaterThanOrEqual(115);
});

test("FR-26: scans sent back to back are typed in order", async ({ page }) => {
  await setup(page, SCANNER, '<textarea id="log"></textarea>');
  await page.focus("#log");
  await page.evaluate(async () => {
    const all = Array.from({ length: 20 }, (_, i) =>
      window.installation.scan("scanner-1", `C${String(i).padStart(2, "0")}`),
    );
    await Promise.all(all);
  });
  const lines = (await page.locator("#log").inputValue()).trim().split("\n");
  expect(lines).toEqual(Array.from({ length: 20 }, (_, i) => `C${String(i).padStart(2, "0")}`));
});

test("fault double-scan types each scan twice; partial-scan drops Enter", async ({ page }) => {
  await setup(page, SCANNER, '<textarea id="log"></textarea>');
  await page.focus("#log");
  await page.evaluate(() =>
    window.installation.transport.setFault("scanner-1", "double-scan", true, { gapMs: 10 }),
  );
  await scan(page, "1");
  await expect(page.locator("#log")).toHaveValue("1\n1\n");

  await page.evaluate(async () => {
    await window.installation.transport.setFault("scanner-1", "double-scan", false);
    await window.installation.transport.setFault("scanner-1", "partial-scan", true);
  });
  await scan(page, "2");
  await expect(page.locator("#log")).toHaveValue("1\n1\n2");
});

test("FR-14: wedge: false leaves the page's keyboard alone", async ({ page }) => {
  await page.goto("/e2e/page.html");
  await page.waitForFunction(() => window.ready === true);
  const wedge = await page.evaluate(async () => {
    const installation = window.shim.install({
      mode: "standalone",
      devices: [{ kind: "scanner-wedge", id: "scanner-1" }],
      wedge: false,
    });
    await installation.transport.ready;
    return installation.wedge;
  });
  expect(wedge).toBeNull();
});
