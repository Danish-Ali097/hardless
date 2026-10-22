/**
 * `@hardless/shim-web` replaces browser hardware APIs with shims backed by Hardless emulated
 * devices, so an app's production code runs unchanged (FR-14 to FR-17).
 *
 * It shims `navigator.serial` (Web Serial, PRD section 8.1), `navigator.usb` (WebUSB,
 * section 8.2) and `NDEFReader` (Web NFC, section 8.3), and runs the keyboard-wedge
 * injector (section 8.4).
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
import type { NdefRecord } from "@hardless/types";
import {
  HardlessNDEFMessage,
  HardlessNDEFReader,
  HardlessNDEFReadingEvent,
  HardlessNDEFRecord,
  NfcHub,
} from "./nfc.js";
import { HardlessSerial } from "./serial.js";
import { HardlessUSB } from "./usb.js";
import { ServerTransport } from "./server-transport.js";
import { StandaloneTransport, type StandaloneDevice } from "./standalone-transport.js";
import type { ScanOptions, Transport, TransportListener } from "./transport.js";
import { WedgeInjector } from "./wedge.js";

export type { StandaloneDevice } from "./standalone-transport.js";
export {
  HardlessNDEFMessage,
  HardlessNDEFReader,
  HardlessNDEFReadingEvent,
  HardlessNDEFRecord,
  NfcHub,
} from "./nfc.js";
export { HardlessSerial, HardlessSerialPort } from "./serial.js";
export {
  HardlessUSB,
  HardlessUSBConnectionEvent,
  HardlessUSBDevice,
  HardlessUSBInTransferResult,
  HardlessUSBOutTransferResult,
} from "./usb.js";
export type { ScanOptions } from "./transport.js";
export { WedgeInjector } from "./wedge.js";

/** The browser APIs the shim can replace. */
export type ShimApi = "serial" | "usb" | "nfc" | "wedge";

/** Every API the shim knows about, in the order the bridge `hello` message lists them. */
export const SHIM_APIS: readonly ShimApi[] = ["serial", "usb", "nfc", "wedge"];

/** The APIs this version implements; the others arrive in M3. */
const IMPLEMENTED: ReadonlySet<ShimApi> = new Set(["serial", "usb", "nfc", "wedge"]);

/** The version of this package, sent as `shimVersion` in the bridge `hello` message. */
export const SHIM_VERSION = "0.1.1";

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
  /** The `navigator.usb` replacement, if installed. */
  readonly usb: HardlessUSB | null;
  /** The keyboard-wedge injector, if installed. */
  readonly wedge: WedgeInjector | null;
  /** What the `NDEFReader` replacement shares, if installed. */
  readonly nfc: NfcHub | null;
  /**
   * Scans `code` on a keyboard-wedge scanner (FR-26). In standalone mode it resolves once
   * the keys have been typed; in server mode, once the server has taken the scan (the keys
   * follow over the bridge).
   */
  scan(deviceId: string, code: string, options?: ScanOptions): Promise<void>;
  /**
   * Taps a card on an NFC reader (FR-27): scanning `NDEFReader`s fire `reading`. Without
   * `records` the reader uses what it remembers of the card. In standalone mode it resolves
   * once the events have fired.
   */
  tap(deviceId: string, uid: string, records?: NdefRecord[]): Promise<void>;
}

/** The Web NFC globals the shim replaces. */
const NFC_GLOBALS = ["NDEFReader", "NDEFReadingEvent", "NDEFMessage", "NDEFRecord"] as const;

/** The page's own Web NFC globals, restored by {@link uninstall}. */
const originalNfc = new Map<string, PropertyDescriptor | undefined>();

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
  let usb: HardlessUSB | null = null;
  let nfc: NfcHub | null = null;
  const wedge = options.wedge === false ? null : new WedgeInjector(document);
  const listener: TransportListener = {
    devices: () => undefined,
    // Each API ignores devices it does not own (serial ports, USB printers).
    data: (id, bytes) => {
      serial?.handleData(id, bytes);
      usb?.handleData(id, bytes);
    },
    connect: (id) => {
      serial?.handleConnect(id);
      usb?.handleConnect(id);
    },
    disconnect: (id) => {
      serial?.handleDisconnect(id);
      usb?.handleDisconnect(id);
    },
    wedge: (_id, scan) => void wedge?.type(scan),
    nfc: (id, reading, error) => {
      nfc?.handleReading(id, reading, error);
    },
  };
  const apis = SHIM_APIS.filter((api) => options[api] !== false && IMPLEMENTED.has(api));
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

  if (options.usb !== false) {
    usb = new HardlessUSB(transport);
    const shim = usb;
    Object.defineProperty(navigator, "usb", {
      configurable: true,
      enumerable: true,
      get: () => shim,
    });
  }

  if (options.nfc !== false) {
    const hub = new NfcHub(transport);
    nfc = hub;
    // NDEFReader takes no arguments; each one shares the page's hub.
    const NDEFReader = class NDEFReader extends HardlessNDEFReader {
      constructor() {
        super(hub);
      }
    };
    const replacements: Record<(typeof NFC_GLOBALS)[number], unknown> = {
      NDEFReader,
      NDEFReadingEvent: HardlessNDEFReadingEvent,
      NDEFMessage: HardlessNDEFMessage,
      NDEFRecord: HardlessNDEFRecord,
    };
    for (const name of NFC_GLOBALS) {
      originalNfc.set(name, Object.getOwnPropertyDescriptor(globalThis, name));
      Object.defineProperty(globalThis, name, {
        configurable: true,
        writable: true,
        value: replacements[name],
      });
    }
  }

  const installation: Installation = {
    options,
    transport,
    serial,
    usb,
    wedge,
    nfc,
    async scan(deviceId, code, scanOptions) {
      await transport.scan(deviceId, code, scanOptions);
      // Standalone: the engine's events are delivered in microtasks that have run by now.
      if (options.mode === "standalone") await wedge?.idle();
    },
    async tap(deviceId, uid, records) {
      await transport.tap(deviceId, uid, records);
      // Standalone: let the engine's events, delivered in microtasks, fire first.
      if (options.mode === "standalone") await new Promise((resolve) => setTimeout(resolve, 0));
    },
  };
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
  if (installation.usb) {
    delete (navigator as unknown as Record<string, unknown>).usb;
  }
  if (installation.nfc) {
    for (const name of NFC_GLOBALS) {
      const original = originalNfc.get(name);
      if (original) Object.defineProperty(globalThis, name, original);
      else Reflect.deleteProperty(globalThis, name);
    }
    originalNfc.clear();
  }
  installation.wedge?.stop();
  installation.transport.stop();
  Reflect.deleteProperty(global, KEY);
}

/** The current installation, if any. */
export function installation(): Installation | null {
  return (globalThis as Global)[KEY] ?? null;
}
