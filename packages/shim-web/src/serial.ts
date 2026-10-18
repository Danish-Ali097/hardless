import type { DeviceSummary } from "@hardless/types";
import { grants } from "./grants.js";
import { showPicker } from "./picker.js";
import { isConnected, stateField, TransportError, type Transport } from "./transport.js";

/**
 * `navigator.serial` and `SerialPort`, following the WICG Web Serial specification
 * (PRD section 8.1, FR-16). Where the spec leaves behaviour open, this follows Chrome.
 */

/** Input signals reported by `getSignals()`: a printer that is ready to receive. */
const INPUT_SIGNALS = {
  dataCarrierDetect: false,
  clearToSend: true,
  ringIndicator: false,
  dataSetReady: true,
} as const;

/** Largest `bufferSize` accepted, as in Chrome. */
const MAX_BUFFER_SIZE = 16 * 1024 * 1024;

function domError(name: string, message: string): DOMException {
  return new DOMException(message, name);
}

/** Maps a transport error to the `DOMException` the spec names. */
function toDomError(error: unknown): DOMException | Error {
  if (error instanceof TransportError) {
    switch (error.code) {
      case "not_found":
        return domError("NotFoundError", error.message);
      case "network":
        return domError("NetworkError", error.message);
      case "invalid_state":
        return domError("InvalidStateError", error.message);
      default:
        return domError("UnknownError", error.message);
    }
  }
  return error instanceof Error ? error : new Error(String(error));
}

/** WebIDL `[EnforceRange] unsigned long` (or `octet` with `max` 255). */
function enforceRange(value: unknown, name: string, max: number): number {
  const n = Number(value);
  if (!Number.isFinite(n)) throw new TypeError(`${name} must be a finite number`);
  const t = Math.trunc(n);
  if (t < 0 || t > max) throw new TypeError(`${name} is outside the range 0 to ${String(max)}`);
  return t;
}

/** A WebIDL enum value as a string; anything but a string is invalid. */
function enumValue(value: unknown): string {
  return typeof value === "string" ? value : typeof value;
}

/** Validates `open()` options like the spec and WebIDL (TypeError on anything invalid). */
export function validateOptions(options: unknown): Record<string, unknown> {
  if (options === undefined || options === null || typeof options !== "object") {
    throw new TypeError(
      "Failed to execute 'open' on 'SerialPort': required member baudRate is undefined.",
    );
  }
  const o = options as Record<string, unknown>;
  if (o.baudRate === undefined) {
    throw new TypeError(
      "Failed to execute 'open' on 'SerialPort': required member baudRate is undefined.",
    );
  }
  const baudRate = enforceRange(o.baudRate, "baudRate", 0xffffffff);
  if (baudRate === 0) throw new TypeError("baudRate must be greater than 0");
  const dataBits = o.dataBits === undefined ? 8 : enforceRange(o.dataBits, "dataBits", 255);
  if (dataBits !== 7 && dataBits !== 8) throw new TypeError("dataBits must be 7 or 8");
  const stopBits = o.stopBits === undefined ? 1 : enforceRange(o.stopBits, "stopBits", 255);
  if (stopBits !== 1 && stopBits !== 2) throw new TypeError("stopBits must be 1 or 2");
  const parity = o.parity === undefined ? "none" : enumValue(o.parity);
  if (!["none", "even", "odd"].includes(parity)) {
    throw new TypeError(
      `The provided value '${parity}' is not a valid enum value of type ParityType.`,
    );
  }
  const bufferSize =
    o.bufferSize === undefined ? 255 : enforceRange(o.bufferSize, "bufferSize", 0xffffffff);
  if (bufferSize === 0) throw new TypeError("bufferSize must be greater than 0");
  if (bufferSize > MAX_BUFFER_SIZE) throw new TypeError("bufferSize is too large");
  const flowControl = o.flowControl === undefined ? "none" : enumValue(o.flowControl);
  if (!["none", "hardware"].includes(flowControl)) {
    throw new TypeError(
      `The provided value '${flowControl}' is not a valid enum value of type FlowControlType.`,
    );
  }
  return { baudRate, dataBits, stopBits, parity, bufferSize, flowControl };
}

/** A `SerialPortFilter`. */
export interface SerialPortFilter {
  usbVendorId?: number;
  usbProductId?: number;
  bluetoothServiceClassId?: number | string;
}

/** Checks `requestPort()` filters like the spec: TypeError for invalid filters. */
export function validateFilters(filters: unknown): SerialPortFilter[] | undefined {
  if (filters === undefined) return undefined;
  if (!Array.isArray(filters)) throw new TypeError("filters must be an array");
  return filters.map((filter: unknown) => {
    const f = (filter ?? {}) as SerialPortFilter;
    if (f.bluetoothServiceClassId !== undefined) {
      if (f.usbVendorId !== undefined || f.usbProductId !== undefined) {
        throw new TypeError("A filter cannot combine bluetoothServiceClassId with USB IDs.");
      }
      return f;
    }
    if (f.usbVendorId === undefined) {
      throw new TypeError("A filter must contain usbVendorId (or bluetoothServiceClassId).");
    }
    return f;
  });
}

function info(device: DeviceSummary): { usbVendorId?: number; usbProductId?: number } {
  const vendor = stateField(device, "usbVendorId");
  const product = stateField(device, "usbProductId");
  const result: { usbVendorId?: number; usbProductId?: number } = {};
  if (typeof vendor === "number") result.usbVendorId = vendor;
  if (typeof product === "number") result.usbProductId = product;
  return result;
}

/** Whether a device matches one filter (spec "matches the filter"). */
export function matchesFilter(device: DeviceSummary, filter: SerialPortFilter): boolean {
  // Emulated ports are USB-serial: Bluetooth filters never match.
  if (filter.bluetoothServiceClassId !== undefined) return false;
  const i = info(device);
  if (filter.usbVendorId === undefined) return true;
  if (i.usbVendorId !== filter.usbVendorId) return false;
  if (filter.usbProductId === undefined) return true;
  return i.usbProductId === filter.usbProductId;
}

/** Devices that can appear as serial ports. */
function isSerialDevice(device: DeviceSummary): boolean {
  return device.kind === "printer-escpos" && stateField(device, "transport") === "serial";
}

/** Fires `type` at `port` and lets it bubble to `navigator.serial`, as the spec requires. */
function fire(
  port: HardlessSerialPort,
  serial: HardlessSerial,
  type: "connect" | "disconnect",
): void {
  port.dispatchEvent(new Event(type, { bubbles: true }));
  const event = new Event(type, { bubbles: true });
  // EventTargets have no parent chain, so re-dispatch on Serial with the port as target.
  Object.defineProperty(event, "target", { value: port });
  serial.dispatchEvent(event);
}

/** `navigator.serial`. */
export class HardlessSerial extends EventTarget {
  onconnect: ((this: HardlessSerial, ev: Event) => unknown) | null = null;
  ondisconnect: ((this: HardlessSerial, ev: Event) => unknown) | null = null;
  private readonly ports = new Map<string, HardlessSerialPort>();

  constructor(readonly transport: Transport) {
    super();
    this.addEventListener("connect", (e) => this.onconnect?.call(this, e));
    this.addEventListener("disconnect", (e) => this.ondisconnect?.call(this, e));
  }

  /** The one `SerialPort` object for a device (the spec returns the same object each time). */
  port(deviceId: string): HardlessSerialPort {
    let port = this.ports.get(deviceId);
    if (!port) {
      port = new HardlessSerialPort(deviceId, this);
      this.ports.set(deviceId, port);
    }
    return port;
  }

  /** Called by the transport. */
  handleConnect(deviceId: string): void {
    if (!grants.has(deviceId)) return;
    const port = this.port(deviceId);
    port.setConnected(true);
    fire(port, this, "connect");
  }

  /** Called by the transport. */
  handleDisconnect(deviceId: string): void {
    const port = this.ports.get(deviceId);
    if (port) port.lost();
    if (!grants.has(deviceId)) return;
    const p = this.port(deviceId);
    p.setConnected(false);
    fire(p, this, "disconnect");
  }

  /** Called by the transport. */
  handleData(deviceId: string, bytes: Uint8Array): void {
    this.ports.get(deviceId)?.receive(bytes);
  }

  async getPorts(): Promise<HardlessSerialPort[]> {
    await this.transport.ready;
    const devices = this.transport.devices();
    return grants
      .all()
      .filter((id) => devices.some((d) => d.id === id && isSerialDevice(d) && isConnected(d)))
      .map((id) => this.port(id));
  }

  async requestPort(options: { filters?: unknown } = {}): Promise<HardlessSerialPort> {
    // Older browsers lack navigator.userActivation; then the check cannot be made.
    const activation = (navigator as { userActivation?: { isActive: boolean } }).userActivation;
    if (activation !== undefined && !activation.isActive) {
      throw domError(
        "SecurityError",
        "Failed to execute 'requestPort' on 'Serial': Must be handling a user gesture to show a permission request.",
      );
    }
    const filters = validateFilters(options.filters);
    await this.transport.ready;
    const candidates = this.transport
      .devices()
      .filter((d) => isSerialDevice(d) && isConnected(d))
      .filter((d) => !filters || filters.some((f) => matchesFilter(d, f)));

    const cancel = candidates.find((d) => d.faults.some((f) => f.id === "picker-cancel"));
    if (cancel) {
      // `picker-cancel` affects the next request only (PRD section 10).
      await this.transport.clearFault(cancel.id, "picker-cancel").catch(() => undefined);
      throw domError("NotFoundError", "No port selected by the user.");
    }

    const auto = candidates.find((d) => stateField(d, "autoSelect") === true);
    const chosen = auto?.id ?? (await showPicker(candidates, window.location.origin));
    if (chosen === null) throw domError("NotFoundError", "No port selected by the user.");
    grants.add(chosen);
    const port = this.port(chosen);
    port.setConnected(true);
    return port;
  }
}

type PortState = "closed" | "opening" | "opened" | "closing" | "forgotten";

/** `SerialPort`. */
export class HardlessSerialPort extends EventTarget {
  onconnect: ((this: HardlessSerialPort, ev: Event) => unknown) | null = null;
  ondisconnect: ((this: HardlessSerialPort, ev: Event) => unknown) | null = null;
  private state: PortState = "closed";
  private bufferSize = 255;
  private isConnected = false;
  private readableStream: ReadableStream<Uint8Array> | null = null;
  private readableController: ReadableByteStreamController | null = null;
  private writableStream: WritableStream<Uint8Array> | null = null;
  private writableController: WritableStreamDefaultController | null = null;
  private readFatal = false;
  private writeFatal = false;
  private pendingClose: (() => void) | null = null;

  constructor(
    readonly deviceId: string,
    private readonly serial: HardlessSerial,
  ) {
    super();
    this.addEventListener("connect", (e) => this.onconnect?.call(this, e));
    this.addEventListener("disconnect", (e) => this.ondisconnect?.call(this, e));
    const device = serial.transport.devices().find((d) => d.id === deviceId);
    this.isConnected = device ? isConnected(device) : false;
  }

  /** Whether the device is plugged in. */
  get connected(): boolean {
    return this.isConnected;
  }

  /** @internal */
  setConnected(connected: boolean): void {
    this.isConnected = connected;
  }

  getInfo(): { usbVendorId?: number; usbProductId?: number } {
    const device = this.serial.transport.devices().find((d) => d.id === this.deviceId);
    return device ? info(device) : {};
  }

  async open(options?: unknown): Promise<void> {
    if (this.state !== "closed") {
      throw domError(
        "InvalidStateError",
        "Failed to execute 'open' on 'SerialPort': The port is already open.",
      );
    }
    const valid = validateOptions(options);
    this.state = "opening";
    try {
      await this.serial.transport.open(this.deviceId, valid);
    } catch (error) {
      this.state = "closed";
      const e = toDomError(error);
      throw e instanceof DOMException && e.name !== "NetworkError"
        ? domError("NetworkError", `Failed to open serial port: ${e.message}`)
        : e;
    }
    this.state = "opened";
    this.bufferSize = valid.bufferSize as number;
  }

  get readable(): ReadableStream<Uint8Array> | null {
    if (this.readableStream) return this.readableStream;
    if (this.state !== "opened" || this.readFatal) return null;
    const stream = new ReadableStream(
      {
        type: "bytes",
        start: (controller) => {
          this.readableController = controller;
        },
        cancel: () => {
          this.closeReadable();
        },
      },
      { highWaterMark: this.bufferSize },
    ) as ReadableStream<Uint8Array>;
    this.readableStream = stream;
    return stream;
  }

  get writable(): WritableStream<Uint8Array> | null {
    if (this.writableStream) return this.writableStream;
    if (this.state !== "opened" || this.writeFatal) return null;
    const stream = new WritableStream<Uint8Array>(
      {
        start: (controller) => {
          this.writableController = controller;
        },
        write: async (chunk: unknown) => {
          const bytes = toBytes(chunk);
          try {
            await this.serial.transport.write(this.deviceId, bytes);
          } catch (error) {
            const e = toDomError(error);
            if (e instanceof DOMException && e.name === "NetworkError") {
              this.writeFatal = true;
              this.closeWritable();
            }
            throw e;
          }
        },
        close: () => {
          this.closeWritable();
        },
        abort: () => {
          this.closeWritable();
        },
      },
      {
        highWaterMark: this.bufferSize,
        // Count bytes, but let anything that is not a BufferSource reach write(), which
        // rejects it with TypeError as the spec says.
        size: (chunk: unknown) =>
          chunk instanceof ArrayBuffer || ArrayBuffer.isView(chunk) ? chunk.byteLength : 1,
      },
    );
    this.writableStream = stream;
    return stream;
  }

  /** @internal Device -> host bytes. */
  receive(bytes: Uint8Array): void {
    if (this.state !== "opened" || !this.readableController) return;
    // Byte streams take ownership of the buffer, so give them a copy.
    this.readableController.enqueue(bytes.slice());
  }

  /** @internal The device went away: streams error with NetworkError (spec "port was disconnected"). */
  lost(): void {
    if (this.state !== "opened") return;
    const error = domError("NetworkError", "The device has been lost.");
    if (this.readableStream) {
      this.readFatal = true;
      try {
        this.readableController?.error(error);
      } catch {
        // Already closed.
      }
      this.closeReadable();
    } else {
      this.readFatal = true;
    }
    if (this.writableStream) {
      this.writeFatal = true;
      try {
        this.writableController?.error(error);
      } catch {
        // Already closed.
      }
      this.closeWritable();
    } else {
      this.writeFatal = true;
    }
  }

  private closeReadable(): void {
    this.readableStream = null;
    this.readableController = null;
    if (!this.writableStream) this.pendingClose?.();
  }

  private closeWritable(): void {
    this.writableStream = null;
    this.writableController = null;
    if (!this.readableStream) this.pendingClose?.();
  }

  async setSignals(signals: Record<string, unknown> = {}): Promise<void> {
    if (this.state !== "opened") {
      throw domError(
        "InvalidStateError",
        "Failed to execute 'setSignals' on 'SerialPort': The port is closed.",
      );
    }
    const out: { dataTerminalReady?: boolean; requestToSend?: boolean; break?: boolean } = {};
    if (signals.dataTerminalReady !== undefined)
      out.dataTerminalReady = Boolean(signals.dataTerminalReady);
    if (signals.requestToSend !== undefined) out.requestToSend = Boolean(signals.requestToSend);
    if (signals.break !== undefined) out.break = Boolean(signals.break);
    if (Object.keys(out).length === 0) {
      throw new TypeError(
        "Failed to execute 'setSignals' on 'SerialPort': Signals dictionary is empty.",
      );
    }
    try {
      await this.serial.transport.signals(this.deviceId, out);
    } catch (error) {
      throw toDomError(error);
    }
  }

  async getSignals(): Promise<typeof INPUT_SIGNALS> {
    if (this.state !== "opened") {
      throw domError(
        "InvalidStateError",
        "Failed to execute 'getSignals' on 'SerialPort': The port is closed.",
      );
    }
    await Promise.resolve();
    return { ...INPUT_SIGNALS };
  }

  async close(): Promise<void> {
    if (this.state !== "opened") {
      throw domError(
        "InvalidStateError",
        "Failed to execute 'close' on 'SerialPort': The port is already closed.",
      );
    }
    // As in Chrome: a locked stream cannot be cancelled, so close() rejects without side effects.
    if (this.readableStream?.locked || this.writableStream?.locked) {
      throw new TypeError(
        "Failed to execute 'close' on 'SerialPort': Cannot cancel a locked stream",
      );
    }
    const cancel = this.readableStream ? this.readableStream.cancel() : Promise.resolve();
    const abort = this.writableStream ? this.writableStream.abort() : Promise.resolve();
    const pending = new Promise<void>((resolve) => {
      this.pendingClose = resolve;
      if (!this.readableStream && !this.writableStream) resolve();
    });
    this.state = "closing";
    try {
      await Promise.all([cancel, abort, pending]);
    } catch (error) {
      this.pendingClose = null;
      this.state = "opened";
      throw error instanceof Error ? error : new Error(String(error));
    }
    // The server may already have closed the port when the device went away.
    await this.serial.transport.close(this.deviceId).catch(() => undefined);
    this.state = "closed";
    this.readFatal = false;
    this.writeFatal = false;
    this.pendingClose = null;
  }

  async forget(): Promise<void> {
    if (this.state === "opened") await this.close().catch(() => undefined);
    this.state = "forgotten";
    grants.remove(this.deviceId);
  }
}

/** Converts a written chunk to bytes: any BufferSource, else TypeError (spec write algorithm). */
function toBytes(chunk: unknown): Uint8Array {
  if (chunk instanceof ArrayBuffer) return new Uint8Array(chunk.slice(0));
  if (ArrayBuffer.isView(chunk)) {
    return new Uint8Array(
      chunk.buffer.slice(chunk.byteOffset, chunk.byteOffset + chunk.byteLength),
    );
  }
  throw new TypeError(
    "Failed to execute 'write' on 'UnderlyingSinkBase': parameter 1 is not of type 'BufferSource'.",
  );
}
