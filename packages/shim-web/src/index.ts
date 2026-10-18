/**
 * `@hardless/shim-web` replaces browser hardware APIs with shims backed by Hardless emulated
 * devices, so an app's production code runs unchanged (FR-14 to FR-17).
 *
 * Milestone M1 shims `navigator.serial` (Web Serial, PRD section 8.1). `navigator.usb`,
 * `NDEFReader` and the keyboard wedge arrive in M2 and M3.
 *
 * @example
 * ```ts
 * import { install } from "@hardless/shim-web";
 * install({ mode: "server", url: "ws://localhost:8080/bridge" });
 * const port = await navigator.serial.requestPort(); // the Hardless port picker
 * ```
 *
 * @packageDocumentation
 */
import { HardlessSerial } from "./serial.js";
import { ServerTransport } from "./server-transport.js";
import { StandaloneTransport, type StandaloneDevice } from "./standalone-transport.js";
import type { Transport, TransportListener } from "./transport.js";

export type { StandaloneDevice } from "./standalone-transport.js";
export { HardlessSerial, HardlessSerialPort } from "./serial.js";

/** The browser APIs the shim can replace. */
export type ShimApi = "serial" | "usb" | "nfc" | "wedge";

/** Every API the shim knows about, in the order the bridge `hello` message lists them. */
export const SHIM_APIS: readonly ShimApi[] = ["serial", "usb", "nfc", "wedge"];

/** The version of this package, sent as `shimVersion` in the bridge `hello` message. */
export const SHIM_VERSION = "0.0.0";

/** Which APIs to replace (FR-14); each defaults to on. */
export interface ApiToggles {
  serial?: boolean;
  usb?: boolean;
  nfc?: boolean;
  wedge?: boolean;
}

/** Connect to a running Hardless server through its bridge. */
export interface ServerMode extends ApiToggles {
  mode: "server";
  /** The bridge URL (default `ws://localhost:8080/bridge`). */
  url?: string;
  /** Bearer token if the server requires one (NFR-02). */
  token?: string;
  /** Install on origins other than localhost without a warning (NFR-03). */
  allowAnyOrigin?: boolean;
}

/** Run the devices in the page, with the engine compiled to WebAssembly. */
export interface StandaloneMode extends ApiToggles {
  mode: "standalone";
  /** The devices to emulate. */
  devices: StandaloneDevice[];
  /** Where to load the engine's JavaScript glue from (default: next to the shim). */
  wasmUrl?: string;
  /** Install on origins other than localhost without a warning (NFR-03). */
  allowAnyOrigin?: boolean;
}

/** Options for {@link install} (FR-15). */
export type InstallOptions = ServerMode | StandaloneMode;

/** A running shim. */
export interface Installation {
  readonly options: InstallOptions;
  /** The transport to the emulated devices. */
  readonly transport: Transport;
  /** The `navigator.serial` replacement, if installed. */
  readonly serial: HardlessSerial | null;
}

const KEY = Symbol.for("hardless.shim");

interface Global {
  [KEY]?: Installation;
}

function isLocalOrigin(): boolean {
  const host = window.location.hostname;
  return (
    host === "localhost" ||
    host === "127.0.0.1" ||
    host === "[::1]" ||
    host === "::1" ||
    host.endsWith(".localhost")
  );
}

/**
 * Replaces the enabled browser APIs with Hardless shims (FR-14). Calling it again returns
 * the existing installation unchanged (FR-17), so it is safe in code that runs twice.
 */
export function install(options: InstallOptions): Installation {
  if (typeof window === "undefined" || typeof navigator === "undefined") {
    throw new Error("@hardless/shim-web runs in a browser");
  }
  const global = globalThis as Global;
  const existing = global[KEY];
  if (existing) return existing;

  if (!options.allowAnyOrigin && !isLocalOrigin()) {
    // NFR-03: the shim is a development tool; warn if it ends up in production.
    console.warn(
      `[hardless] The Hardless shim is installed on ${window.location.origin}, which is not localhost. ` +
        "Remove it from production builds, or pass allowAnyOrigin: true if this is intended.",
    );
  }

  let serial: HardlessSerial | null = null;
  const listener: TransportListener = {
    devices: () => undefined,
    data: (id, bytes) => serial?.handleData(id, bytes),
    connect: (id) => serial?.handleConnect(id),
    disconnect: (id) => serial?.handleDisconnect(id),
  };
  const apis = SHIM_APIS.filter((api) => options[api] !== false && api === "serial");
  const transport: Transport =
    options.mode === "server"
      ? new ServerTransport(
          {
            url: options.url ?? "ws://localhost:8080/bridge",
            ...(options.token === undefined ? {} : { token: options.token }),
            apis,
            shimVersion: SHIM_VERSION,
          },
          listener,
        )
      : new StandaloneTransport(
          {
            devices: options.devices,
            ...(options.wasmUrl === undefined ? {} : { wasmUrl: options.wasmUrl }),
          },
          listener,
        );

  if (options.serial !== false) {
    serial = new HardlessSerial(transport);
    const shim = serial;
    Object.defineProperty(navigator, "serial", {
      configurable: true,
      enumerable: true,
      get: () => shim,
    });
  }

  const installation: Installation = { options, transport, serial };
  global[KEY] = installation;
  return installation;
}

/** Restores the original browser APIs and disconnects (FR-14). */
export function uninstall(): void {
  const global = globalThis as Global;
  const installation = global[KEY];
  if (!installation) return;
  if (installation.serial) {
    // Removing the own property brings back the browser's getter on Navigator.prototype.
    delete (navigator as unknown as Record<string, unknown>).serial;
  }
  installation.transport.stop();
  Reflect.deleteProperty(global, KEY);
}

/** The current installation, if any. */
export function installation(): Installation | null {
  return (globalThis as Global)[KEY] ?? null;
}
