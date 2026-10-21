import type {
  ControlCommand,
  DeviceSummary,
  EventRecord,
  Job,
  NdefRecord,
  NfcRejection,
  SerialSignals,
  Terminator,
} from "@hardless/types";
import {
  fromBase64,
  isConnected,
  TransportError,
  type ScanOptions,
  type Transport,
  type TransportListener,
} from "./transport.js";

/**
 * A device for standalone mode. Either the server's config shape, `{ kind, id, config }`,
 * or the Playwright fixture's flat shape, `{ kind, id, autoSelect: true, attachedTo: ... }`.
 */
export interface StandaloneDevice {
  kind: string;
  id?: string;
  config?: Record<string, unknown>;
  [setting: string]: unknown;
}

/** Options for the standalone transport. */
export interface StandaloneTransportOptions {
  devices: StandaloneDevice[];
  /** Where to load the engine's JavaScript glue from (default: next to the shim). */
  wasmUrl?: string;
}

/** The parts of the wasm-bindgen output the shim uses (`crates/hardless-wasm`). */
interface WasmEngine {
  devices(): string;
  hostBytes(now: number, deviceId: string, data: Uint8Array): string;
  hostSignals(now: number, deviceId: string, signals: string): void;
  tick(now: number): string;
  nextDeadline(): number | undefined;
  setFault(now: number, deviceId: string, fault: string, active: boolean, options: string): string;
  control(now: number, deviceId: string, command: string): string;
  reset(now: number, deviceId: string): string;
  jobs(deviceId: string): string;
  events(deviceId: string): string;
}

interface WasmModule {
  default: (input?: unknown) => Promise<unknown>;
  Engine: new (devices: string) => WasmEngine;
}

type EngineEvent =
  | { type: "data"; deviceId: string; data: string }
  | { type: "connect"; deviceId: string }
  | { type: "disconnect"; deviceId: string }
  | { type: "wedge"; deviceId: string; code: string; delayMs: number; terminator: Terminator }
  | {
      type: "nfc-reading" | "nfc-error";
      deviceId: string;
      serialNumber: string;
      records: NdefRecord[];
    }
  | { type: "nfc-done"; deviceId: string }
  | { type: "nfc-rejected"; deviceId: string; reason: NfcRejection; message: string };

/** Turns a fixture-style device into the engine's `{ kind, id, config }`. */
export function normalizeDevice(device: StandaloneDevice): {
  kind: string;
  id?: string;
  config: Record<string, unknown>;
} {
  if (device.config !== undefined) {
    return device.id === undefined
      ? { kind: device.kind, config: device.config }
      : { kind: device.kind, id: device.id, config: device.config };
  }
  const config = Object.fromEntries(
    Object.entries(device).filter(([key]) => key !== "kind" && key !== "id"),
  );
  return device.id === undefined
    ? { kind: device.kind, config }
    : { kind: device.kind, id: device.id, config };
}

/**
 * Runs the Hardless engine, compiled to WebAssembly, inside the page (FR-15). Loaded lazily,
 * only when standalone mode is used (NFR-08). Device behaviour is the same as on the server
 * because it is the same code.
 */
export class StandaloneTransport implements Transport {
  readonly ready: Promise<void>;
  private engine: WasmEngine | null = null;
  private list: DeviceSummary[] = [];
  private timer: ReturnType<typeof setTimeout> | undefined;
  private stopped = false;
  private opened = new Set<string>();

  constructor(
    options: StandaloneTransportOptions,
    private readonly listener: TransportListener,
  ) {
    this.ready = this.load(options);
  }

  private async load(options: StandaloneTransportOptions): Promise<void> {
    // Built from a non-literal so bundlers leave it alone: the engine is loaded only when
    // standalone mode is used (NFR-08).
    const glue = ["..", "wasm", "hardless_wasm.js"].join("/");
    const url = options.wasmUrl ?? new URL(glue, import.meta.url).href;
    const wasm = (await import(/* @vite-ignore */ url)) as WasmModule;
    await wasm.default();
    this.engine = new wasm.Engine(JSON.stringify(options.devices.map(normalizeDevice)));
    this.refresh();
  }

  private now(): number {
    return performance.now();
  }

  private require(): WasmEngine {
    if (!this.engine) throw new TransportError("internal", "the Hardless engine has not loaded");
    return this.engine;
  }

  /** Reads the device list, reports it, and re-arms the timer for device deadlines. */
  private refresh(): void {
    const engine = this.require();
    this.list = JSON.parse(engine.devices()) as DeviceSummary[];
    this.listener.devices(this.list);
    clearTimeout(this.timer);
    const deadline = engine.nextDeadline();
    if (deadline !== undefined && !this.stopped) {
      this.timer = setTimeout(
        () => {
          this.emit(this.require().tick(this.now()));
          this.refresh();
        },
        Math.max(0, deadline - this.now()),
      );
    }
  }

  /** Delivers engine events asynchronously, as a real device would. */
  private emit(json: string): void {
    const events = JSON.parse(json) as EngineEvent[];
    for (const event of events) {
      queueMicrotask(() => {
        if (event.type === "data") {
          if (this.opened.has(event.deviceId)) {
            this.listener.data(event.deviceId, fromBase64(event.data));
          }
        } else if (event.type === "connect") {
          this.listener.connect(event.deviceId);
        } else if (event.type === "wedge") {
          const { code, delayMs, terminator } = event;
          this.listener.wedge(event.deviceId, { code, delayMs, terminator });
        } else if (event.type === "nfc-reading" || event.type === "nfc-error") {
          const { serialNumber, records } = event;
          this.listener.nfc(event.deviceId, { serialNumber, records }, event.type === "nfc-error");
        } else if (event.type === "disconnect") {
          this.opened.delete(event.deviceId);
          this.listener.disconnect(event.deviceId);
        }
      });
    }
  }

  private call<T>(f: (engine: WasmEngine) => T): T {
    try {
      return f(this.require());
    } catch (error) {
      if (error instanceof TransportError) throw error;
      throw new TransportError("not_found", error instanceof Error ? error.message : String(error));
    }
  }

  devices(): DeviceSummary[] {
    return this.list;
  }

  private device(deviceId: string): DeviceSummary {
    const device = this.list.find((d) => d.id === deviceId);
    if (!device) throw new TransportError("not_found", `device \`${deviceId}\` not found`);
    return device;
  }

  async open(deviceId: string): Promise<void> {
    await this.ready;
    const device = this.device(deviceId);
    if (!isConnected(device))
      throw new TransportError("network", `device \`${deviceId}\` is disconnected`);
    if (this.opened.has(deviceId))
      throw new TransportError("invalid_state", `port \`${deviceId}\` is already open`);
    if (device.faults.some((f) => f.id === "interface-busy")) {
      // A USB printer whose interface another program has claimed (PRD section 10).
      throw new TransportError("network", `the interface of \`${deviceId}\` is claimed elsewhere`);
    }
    this.opened.add(deviceId);
  }

  async close(deviceId: string): Promise<void> {
    await this.ready;
    this.opened.delete(deviceId);
  }

  async write(deviceId: string, data: Uint8Array): Promise<void> {
    await this.ready;
    if (!this.opened.has(deviceId))
      throw new TransportError("invalid_state", `port \`${deviceId}\` is not open`);
    const slow = this.device(deviceId).faults.find((f) => f.id === "slow-write");
    const events = JSON.parse(
      this.call((e) => e.hostBytes(this.now(), deviceId, data)),
    ) as EngineEvent[];
    this.emit(JSON.stringify(events));
    this.refresh();
    if (events.some((e) => e.type === "disconnect" && e.deviceId === deviceId)) {
      throw new TransportError("network", `device \`${deviceId}\` disconnected during the write`);
    }
    if (slow) {
      const ms = typeof slow.options.msPerChunk === "number" ? slow.options.msPerChunk : 200;
      await new Promise((resolve) => setTimeout(resolve, ms));
    }
  }

  async signals(deviceId: string, signals: SerialSignals): Promise<void> {
    await this.ready;
    if (!this.opened.has(deviceId))
      throw new TransportError("invalid_state", `port \`${deviceId}\` is not open`);
    this.call((e) => {
      e.hostSignals(this.now(), deviceId, JSON.stringify(signals));
    });
  }

  async clearFault(deviceId: string, fault: string): Promise<void> {
    await this.ready;
    this.emit(this.call((e) => e.setFault(this.now(), deviceId, fault, false, "")));
    this.refresh();
  }

  /** Turns a fault on or off (used by tests and the Playwright fixture). */
  async setFault(
    deviceId: string,
    fault: string,
    active: boolean,
    options: Record<string, unknown> = {},
  ): Promise<void> {
    await this.ready;
    this.emit(
      this.call((e) => e.setFault(this.now(), deviceId, fault, active, JSON.stringify(options))),
    );
    this.refresh();
  }

  /** Sends a control command, e.g. a scan (used by tests and the Playwright fixture). */
  async control(deviceId: string, command: ControlCommand): Promise<void> {
    await this.ready;
    this.device(deviceId);
    this.emit(this.call((e) => e.control(this.now(), deviceId, JSON.stringify(command))));
    this.refresh();
  }

  /** A device's print jobs, newest first (used by the Playwright fixture). */
  async jobs(deviceId: string): Promise<Job[]> {
    await this.ready;
    return JSON.parse(this.call((e) => e.jobs(deviceId))) as Job[];
  }

  /** A device's event log, oldest first (used by the Playwright fixture). */
  async events(deviceId: string): Promise<EventRecord[]> {
    await this.ready;
    return JSON.parse(this.call((e) => e.events(deviceId))) as EventRecord[];
  }

  /** Resets a device: clears its state, jobs and faults. */
  async reset(deviceId: string): Promise<void> {
    await this.ready;
    this.emit(this.call((e) => e.reset(this.now(), deviceId)));
    this.refresh();
  }

  scan(deviceId: string, code: string, options: ScanOptions = {}): Promise<void> {
    return this.control(deviceId, { type: "scan", code, ...options });
  }

  tap(deviceId: string, uid: string, records?: NdefRecord[]): Promise<void> {
    const command: ControlCommand =
      records === undefined ? { type: "tap", uid } : { type: "tap", uid, records };
    return this.control(deviceId, command);
  }

  nfcWrite(deviceId: string, records: NdefRecord[], overwrite: boolean): Promise<void> {
    return this.nfcCommand(deviceId, { type: "nfc-write", records, overwrite });
  }

  nfcMakeReadOnly(deviceId: string): Promise<void> {
    return this.nfcCommand(deviceId, { type: "nfc-make-read-only" });
  }

  /** An NFC write or lock, answered like the bridge: resolves, or rejects with its code. */
  private async nfcCommand(deviceId: string, command: ControlCommand): Promise<void> {
    await this.ready;
    this.device(deviceId);
    const json = this.call((e) => e.control(this.now(), deviceId, JSON.stringify(command)));
    this.emit(json);
    this.refresh();
    for (const event of JSON.parse(json) as EngineEvent[]) {
      if (event.type === "nfc-done") return;
      if (event.type === "nfc-rejected") {
        const code = event.reason === "no-card" ? "invalid_state" : "not_allowed";
        throw new TransportError(code, event.message);
      }
    }
    throw new TransportError("bad_message", `device ${deviceId} is not an NFC reader`);
  }

  stop(): void {
    this.stopped = true;
    clearTimeout(this.timer);
  }
}
