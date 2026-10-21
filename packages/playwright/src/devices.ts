import type { DeviceSummary, EventRecord, Job, NdefRecord } from "@hardless/types";
import type { Backend, ScanOptions } from "./backend.js";

/** Any emulated device: its state, faults and event log (FR-42). */
export class DeviceHandle {
  constructor(
    readonly id: string,
    protected readonly backend: Backend,
  ) {}

  /** The device as the server or engine lists it. */
  async summary(): Promise<DeviceSummary> {
    const device = (await this.backend.devices()).find((d) => d.id === this.id);
    if (!device) throw new Error(`Hardless has no device \`${this.id}\`.`);
    return device;
  }

  /** The device's state object. */
  async state(): Promise<Record<string, unknown>> {
    const state = (await this.summary()).state;
    return state && typeof state === "object" && !Array.isArray(state) ? state : {};
  }

  /** Turns a fault on, e.g. `fault("paper-out")` (PRD section 10). */
  fault(faultId: string, options?: Record<string, unknown>): Promise<void> {
    return this.backend.setFault(this.id, faultId, true, options);
  }

  /** Turns a fault off. */
  clearFault(faultId: string): Promise<void> {
    return this.backend.setFault(this.id, faultId, false);
  }

  /** Clears state, jobs and faults. */
  reset(): Promise<void> {
    return this.backend.reset(this.id);
  }

  /** The event log, oldest first. */
  events(): Promise<EventRecord[]> {
    return this.backend.events(this.id);
  }
}

/** A receipt printer. Matchers: `toHavePrinted`, `toHavePrintedJobs`, `toHaveCut`. */
export class PrinterHandle extends DeviceHandle {
  readonly kind = "printer";

  /** Print jobs, newest first: `{ id, at, ast, text, bytesHex }`. */
  jobs(): Promise<Job[]> {
    return this.backend.jobs(this.id);
  }

  /** The latest job, if any. */
  async lastJob(): Promise<Job | undefined> {
    return (await this.jobs())[0];
  }
}

/** A cash drawer. Matchers: `toHaveOpened`, `toBeClosed`. */
export class DrawerHandle extends DeviceHandle {
  readonly kind = "drawer";

  /** Closes the drawer by hand, as a cashier does. */
  close(): Promise<void> {
    return this.backend.control(this.id, { type: "close-drawer" });
  }
}

/** A keyboard-wedge barcode scanner. */
export class ScannerHandle extends DeviceHandle {
  readonly kind = "scanner";

  /** Scans `code`; resolves once the keys have been typed into the page. */
  scan(code: string, options?: ScanOptions): Promise<void> {
    return this.backend.scan(this.id, code, options);
  }
}

/** An NFC reader. Matcher: `toHaveWrittenNdef`. */
export class NfcHandle extends DeviceHandle {
  readonly kind = "nfc";

  /**
   * Taps a card: scanning `NDEFReader`s fire `reading`. Without `records` the reader uses
   * what it remembers of the card.
   */
  tap(uid: string, records?: NdefRecord[]): Promise<void> {
    return this.backend.tap(this.id, uid, records);
  }

  /** What the app last wrote, if anything: `{ uid, records }`. */
  async lastWritten(): Promise<{ uid: string; records: NdefRecord[] } | null> {
    const written = (await this.state()).lastWritten as
      { uid: string; records: NdefRecord[] } | null | undefined;
    return written ?? null;
  }
}

/** The `hardless` fixture: handles for the emulated devices (FR-42). */
export class Hardless {
  constructor(
    readonly mode: "standalone" | "server",
    private readonly backend: Backend,
  ) {}

  /** Every device. */
  devices(): Promise<DeviceSummary[]> {
    return this.backend.devices();
  }

  device(id: string): DeviceHandle {
    return new DeviceHandle(id, this.backend);
  }

  printer(id: string): PrinterHandle {
    return new PrinterHandle(id, this.backend);
  }

  drawer(id: string): DrawerHandle {
    return new DrawerHandle(id, this.backend);
  }

  scanner(id: string): ScannerHandle {
    return new ScannerHandle(id, this.backend);
  }

  nfc(id: string): NfcHandle {
    return new NfcHandle(id, this.backend);
  }
}
