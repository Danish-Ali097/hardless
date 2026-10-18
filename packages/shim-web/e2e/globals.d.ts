// Globals of the test page (e2e/page.html). The DOM types that ship with TypeScript do not
// include Web Serial yet; in the test page `navigator.serial` is the Hardless shim.
import type * as Shim from "../src/index.js";
import type { HardlessSerial, HardlessSerialPort } from "../src/serial.js";
import type { StandaloneTransport } from "../src/standalone-transport.js";

/** A promise outcome that can cross from the page to the test. */
export interface Settled {
  ok: boolean;
  value?: unknown;
  name?: string;
  message?: string;
}

declare global {
  interface Navigator {
    readonly serial: HardlessSerial;
  }

  interface Window {
    ready?: boolean;
    shim: typeof Shim;
    settle: (value: unknown) => Promise<Settled>;
    installation: { transport: StandaloneTransport };
    /** Run by the page's #gesture click handler, with transient user activation. */
    onGesture?: () => unknown;
    result: Promise<Settled> | null;
    port: HardlessSerialPort;
    reader: ReadableStreamDefaultReader<Uint8Array>;
  }
}
