// Globals of the test page (e2e/page.html). The DOM types that ship with TypeScript do not
// include Web Serial yet; in the test page `navigator.serial` is the Hardless shim.
import type * as Shim from "../src/index.js";
import type {
  HardlessNDEFMessage,
  HardlessNDEFReader,
  HardlessNDEFReadingEvent,
  HardlessNDEFRecord,
} from "../src/nfc.js";
import type { HardlessSerial, HardlessSerialPort } from "../src/serial.js";
import type { HardlessUSB, HardlessUSBDevice } from "../src/usb.js";
import type { StandaloneTransport } from "../src/standalone-transport.js";

/** A promise outcome that can cross from the page to the test. */
export interface Settled {
  ok: boolean;
  value?: unknown;
  name?: string;
  message?: string;
}

declare global {
  // Web NFC is not in TypeScript's DOM types; in the test page these are the Hardless shim.
  var NDEFReader: new () => HardlessNDEFReader;
  var NDEFReadingEvent: typeof HardlessNDEFReadingEvent;
  var NDEFMessage: typeof HardlessNDEFMessage;
  var NDEFRecord: typeof HardlessNDEFRecord;

  interface Navigator {
    readonly serial: HardlessSerial;
    readonly usb: HardlessUSB;
  }

  interface Window {
    ready?: boolean;
    shim: typeof Shim;
    settle: (value: unknown) => Promise<Settled>;
    installation: {
      transport: StandaloneTransport;
      scan: Shim.Installation["scan"];
      tap: Shim.Installation["tap"];
      wedge: Shim.WedgeInjector | null;
    };
    /** Key and input events recorded by the wedge tests. */
    log: {
      type: string;
      key?: string;
      keyCode: number;
      which: number;
      trusted: boolean;
      target: string;
    }[];
    submitted: string[];
    /** Run by the page's #gesture click handler, with transient user activation. */
    onGesture?: () => unknown;
    result: Promise<Settled> | null;
    port: HardlessSerialPort;
    reader: ReadableStreamDefaultReader<Uint8Array>;
    /** The USBDevice of the WebUSB tests and the events seen. */
    usbDevice: HardlessUSBDevice;
    usbEvents: { type: string; device: string }[];
    /** The NDEFReader of the NFC tests and what it saw. */
    ndef: HardlessNDEFReader;
    readings: { serialNumber: string; records: Record<string, unknown>[] }[];
    readingErrors: number;
  }
}
