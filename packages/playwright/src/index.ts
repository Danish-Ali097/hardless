/**
 * `@hardless/playwright`: a Playwright fixture that installs the Hardless shim in every page
 * before the app runs (FR-40), handles for the emulated devices (FR-42), and matchers such as
 * `toHavePrinted` and `toHaveOpened` (FR-41).
 *
 * @example
 * ```ts
 * import { test, expect } from "@hardless/playwright";
 *
 * test.use({
 *   hardless: {
 *     mode: "standalone",
 *     devices: [
 *       { kind: "printer-escpos", id: "printer-1", transport: "serial", autoSelect: true },
 *       { kind: "cash-drawer", id: "drawer-1", attachedTo: "printer-1" },
 *     ],
 *   },
 * });
 *
 * test("cash sale prints and opens the drawer", async ({ page, hardless }) => {
 *   await page.goto("/pos");
 *   await page.getByRole("button", { name: "Pay cash" }).click();
 *   await expect(hardless.printer("printer-1")).toHavePrinted(/Total:\s+1,250/);
 *   await expect(hardless.drawer("drawer-1")).toHaveOpened({ times: 1 });
 * });
 * ```
 *
 * @packageDocumentation
 */
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, normalize } from "node:path";
import { expect as base, test as baseTest } from "@playwright/test";
import { PageBackend, ServerBackend, type Backend } from "./backend.js";
import { Hardless } from "./devices.js";
import { matchers } from "./matchers.js";

export type { Backend, ScanOptions } from "./backend.js";
export { restBase } from "./backend.js";
export {
  DeviceHandle,
  DrawerHandle,
  Hardless,
  NfcHandle,
  PrinterHandle,
  ScannerHandle,
} from "./devices.js";
export { MATCHERS, matchers, type MatcherName } from "./matchers.js";

/** A device to emulate: `{ kind, id, ...config }` or `{ kind, id, config }`. */
export interface DeviceSpec {
  kind: string;
  id?: string;
  config?: Record<string, unknown>;
  [setting: string]: unknown;
}

/** Which browser APIs the shim replaces; each defaults to on (FR-14). */
export interface ApiToggles {
  serial?: boolean;
  usb?: boolean;
  nfc?: boolean;
  wedge?: boolean;
}

/** Devices run in the page, with the engine compiled to WebAssembly: no server needed. */
export interface StandaloneOptions extends ApiToggles {
  mode: "standalone";
  devices: DeviceSpec[];
}

/** Devices run in a Hardless server (`hardless start`, Docker), shared with its dashboard. */
export interface ServerOptions extends ApiToggles {
  mode: "server";
  /** The bridge URL, e.g. `ws://localhost:8080/bridge`. */
  url: string;
  /** Token if the server was started with `--token`. */
  token?: string;
  /** Devices the test needs; any missing on the server are created. */
  devices?: DeviceSpec[];
  /** Reset these devices (or every device) before each test, so tests start clean (default true). */
  reset?: boolean;
}

/** What `test.use({ hardless })` takes. */
export type HardlessOptions = StandaloneOptions | ServerOptions;

/** The option for `playwright.config`: `defineConfig<HardlessTestOptions>({ use: { hardlessOptions } })`. */
export interface HardlessTestOptions {
  hardlessOptions: HardlessOptions;
}

/** The devices used when a test does not choose any: one of each kind. */
export const DEFAULT_DEVICES: DeviceSpec[] = [
  { kind: "printer-escpos", id: "printer-1", transport: "serial", autoSelect: true },
  { kind: "cash-drawer", id: "drawer-1", attachedTo: "printer-1" },
  { kind: "printer-escpos", id: "usb-printer-1", transport: "usb", autoSelect: true },
  { kind: "scanner-wedge", id: "scanner-1" },
  { kind: "nfc-reader", id: "nfc-1" },
];

/** Where the page loads the WebAssembly engine from; the fixture answers these requests. */
export const ENGINE_PATH = "/__hardless__/";

const require = createRequire(import.meta.url);

/** `@hardless/shim-web`'s folder: the IIFE build and the engine. */
function shimDir(): string {
  return dirname(require.resolve("@hardless/shim-web/package.json"));
}

let shimSource: string | null = null;

function readShim(): string {
  if (shimSource === null) {
    const file = join(shimDir(), "dist", "hardless-shim.iife.js");
    try {
      shimSource = readFileSync(file, "utf8");
    } catch {
      throw new Error(
        `@hardless/playwright cannot find ${file}. In the Hardless repository, build it with ` +
          "`pnpm --filter @hardless/shim-web build` (and `cargo xtask build-wasm` for standalone mode).",
      );
    }
  }
  return shimSource;
}

/** The init script: the shim, then `install()` with these options (FR-40). */
export function initScript(options: HardlessOptions): string {
  const toggles: ApiToggles = {};
  for (const api of ["serial", "usb", "nfc", "wedge"] as const) {
    if (options[api] !== undefined) toggles[api] = options[api];
  }
  const install =
    options.mode === "standalone"
      ? { mode: "standalone", devices: options.devices, allowAnyOrigin: true, ...toggles }
      : {
          mode: "server",
          url: options.url,
          ...(options.token === undefined ? {} : { token: options.token }),
          allowAnyOrigin: true,
          ...toggles,
        };
  return `${readShim()}
;(() => {
  // Playwright may run init scripts inside a function, where the build's var stays local.
  if (typeof HardlessShim === "undefined" || !/^https?:$/.test(location.protocol)) return;
  globalThis.HardlessShim = HardlessShim;
  const options = ${JSON.stringify(install)};
  if (options.mode === "standalone") {
    options.wasmUrl = new URL(${JSON.stringify(`${ENGINE_PATH}hardless_wasm.js`)}, location.href).href;
  }
  HardlessShim.install(options);
})();`;
}

const CONTENT_TYPES: Record<string, string> = {
  ".js": "text/javascript; charset=utf-8",
  ".wasm": "application/wasm",
};

/** Normalizes a device spec to the REST `POST /devices` body. */
function createBody(spec: DeviceSpec): {
  kind: string;
  id?: string;
  config: Record<string, unknown>;
} {
  const config =
    spec.config ??
    Object.fromEntries(Object.entries(spec).filter(([key]) => key !== "kind" && key !== "id"));
  return spec.id === undefined
    ? { kind: spec.kind, config }
    : { kind: spec.kind, id: spec.id, config };
}

/** Creates missing devices and resets the test's devices on the server. */
async function prepareServer(backend: ServerBackend, options: ServerOptions): Promise<void> {
  const existing = new Set((await backend.devices()).map((d) => d.id));
  for (const spec of options.devices ?? []) {
    if (spec.id !== undefined && existing.has(spec.id)) continue;
    await backend.request("POST", "/devices", createBody(spec));
  }
  if (options.reset === false) return;
  const ids =
    options.devices?.flatMap((d) => (d.id === undefined ? [] : [d.id])) ??
    (await backend.devices()).map((d) => d.id);
  for (const id of ids) await backend.reset(id);
}

interface Fixtures {
  /** The emulated devices (FR-42). */
  hardless: Hardless;
  /** The options from `test.use({ hardless: ... })`. */
  hardlessOptions: HardlessOptions;
}

const extended = baseTest.extend<Fixtures>({
  hardlessOptions: [{ mode: "standalone", devices: DEFAULT_DEVICES }, { option: true }],
  hardless: async ({ page, context, hardlessOptions }, use) => {
    const options = hardlessOptions;
    if (options.mode === "standalone") {
      // The page loads the engine from its own origin; answer those requests from the shim
      // package, for pages and their service workers alike.
      const wasmDir = join(shimDir(), "wasm");
      await context.route(`**${ENGINE_PATH}**`, async (route) => {
        const name = new URL(route.request().url()).pathname.slice(ENGINE_PATH.length);
        const file = normalize(join(wasmDir, name));
        if (!file.startsWith(wasmDir) || name.includes("/")) {
          await route.fulfill({ status: 404 });
          return;
        }
        try {
          const ext = name.slice(name.lastIndexOf("."));
          await route.fulfill({
            body: readFileSync(file),
            contentType: CONTENT_TYPES[ext] ?? "application/octet-stream",
          });
        } catch {
          await route.fulfill({ status: 404 });
        }
      });
    }
    // FR-40: before any navigation, so the app's code finds the shim already there.
    await page.addInitScript({ content: initScript(options) });
    let backend: Backend;
    if (options.mode === "server") {
      const server = new ServerBackend(options.url, options.token, page);
      await prepareServer(server, options);
      backend = server;
    } else {
      backend = new PageBackend(page);
    }
    await use(new Hardless(options.mode, backend));
  },
});

type UseArg = Parameters<typeof extended.use>[0];
const playwrightUse = extended.use.bind(extended);

/**
 * The test function with the `hardless` fixture. `test.use({ hardless: options })` sets the
 * options, as in PRD section 11.3: a fixture cannot be both an option and the object the test
 * receives, so `use` passes them on as `hardlessOptions` (the name to use in
 * `playwright.config`).
 */
export const test = Object.assign(extended, {
  use(fixtures: UseArg & { hardless?: HardlessOptions }): void {
    const { hardless, ...rest } = fixtures as Record<string, unknown> & {
      hardless?: HardlessOptions;
    };
    playwrightUse(hardless === undefined ? rest : { ...rest, hardlessOptions: hardless });
  },
});

/** Playwright's `expect` with the Hardless matchers (FR-41). */
export const expect = base.extend(matchers);
