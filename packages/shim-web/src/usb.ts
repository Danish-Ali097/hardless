import type { DeviceSummary } from "@hardless/types";
import { usbGrants } from "./grants.js";
import { showPicker } from "./picker.js";
import { isConnected, stateField, TransportError, type Transport } from "./transport.js";

/**
 * `navigator.usb` and `USBDevice`, following the WICG WebUSB specification (PRD section 8.2,
 * FR-16), for emulated USB receipt printers. A printer has one configuration with one
 * interface of the USB printer class (class 7, subclass 1, protocol 2, bidirectional), a bulk
 * OUT endpoint 1 and a bulk IN endpoint 2, as in the USB printer class definition 1.1. Where
 * the spec leaves behaviour open, this follows Chrome.
 */

/** USB printer class (USB Device Class Definition for Printing Devices 1.1). */
const PRINTER_CLASS = 0x07;
const PRINTER_SUBCLASS = 0x01;
/** Bidirectional interface. */
const PRINTER_PROTOCOL = 0x02;
const OUT_ENDPOINT = 1;
const IN_ENDPOINT = 2;
const PACKET_SIZE = 64;

/** Printer-class requests (bRequest). */
const GET_DEVICE_ID = 0;
const GET_PORT_STATUS = 1;
const SOFT_RESET = 2;

function domError(name: string, message: string): DOMException {
  return new DOMException(message, name);
}

const NOT_OPEN = "The device must be opened first.";
const NOT_CLAIMED =
  "The specified endpoint is not part of a claimed and selected alternate interface.";
const GONE = "The device was disconnected.";

/** `USBEndpoint`. */
export class HardlessUSBEndpoint {
  readonly type = "bulk";
  readonly packetSize = PACKET_SIZE;
  constructor(
    readonly endpointNumber: number,
    readonly direction: "in" | "out",
  ) {}
}

/** `USBAlternateInterface`: the printer-class interface. */
export class HardlessUSBAlternateInterface {
  readonly alternateSetting = 0;
  readonly interfaceClass = PRINTER_CLASS;
  readonly interfaceSubclass = PRINTER_SUBCLASS;
  readonly interfaceProtocol = PRINTER_PROTOCOL;
  readonly interfaceName = null;
  readonly endpoints: readonly HardlessUSBEndpoint[] = Object.freeze([
    new HardlessUSBEndpoint(OUT_ENDPOINT, "out"),
    new HardlessUSBEndpoint(IN_ENDPOINT, "in"),
  ]);
}

/** `USBInterface`. */
export class HardlessUSBInterface {
  readonly interfaceNumber = 0;
  readonly alternate = new HardlessUSBAlternateInterface();
  readonly alternates: readonly HardlessUSBAlternateInterface[] = Object.freeze([this.alternate]);

  constructor(private readonly device: HardlessUSBDevice) {}

  get claimed(): boolean {
    return this.device.isClaimed();
  }
}

/** `USBConfiguration`. */
export class HardlessUSBConfiguration {
  readonly configurationValue = 1;
  readonly configurationName = null;
  readonly interfaces: readonly HardlessUSBInterface[];

  constructor(device: HardlessUSBDevice) {
    this.interfaces = Object.freeze([new HardlessUSBInterface(device)]);
  }
}

/** `USBInTransferResult`. */
export class HardlessUSBInTransferResult {
  constructor(
    readonly status: "ok" | "stall" | "babble",
    readonly data?: DataView,
  ) {}
}

/** `USBOutTransferResult`. */
export class HardlessUSBOutTransferResult {
  constructor(
    readonly status: "ok" | "stall",
    readonly bytesWritten = 0,
  ) {}
}

/** `USBConnectionEvent`: `connect` and `disconnect` on `navigator.usb`. */
export class HardlessUSBConnectionEvent extends Event {
  readonly device: HardlessUSBDevice;
  constructor(type: string, init: { device: HardlessUSBDevice } & EventInit) {
    super(type, init);
    this.device = init.device;
  }
}

/** A `USBDeviceFilter`. */
export interface USBDeviceFilter {
  vendorId?: number;
  productId?: number;
  classCode?: number;
  subclassCode?: number;
  protocolCode?: number;
  serialNumber?: string;
}

/** Checks filters like the spec's "is a valid filter" (TypeError otherwise). */
export function validateUsbFilters(filters: unknown, name = "filters"): USBDeviceFilter[] {
  if (!Array.isArray(filters)) {
    throw new TypeError(
      `Failed to execute 'requestDevice' on 'USB': required member ${name} is undefined.`,
    );
  }
  return filters.map((filter: unknown) => {
    const f = (filter ?? {}) as USBDeviceFilter;
    if (f.productId !== undefined && f.vendorId === undefined) {
      throw new TypeError("A filter containing a productId must also contain a vendorId.");
    }
    if (f.subclassCode !== undefined && f.classCode === undefined) {
      throw new TypeError("A filter containing a subclassCode must also contain a classCode.");
    }
    if (f.protocolCode !== undefined && f.subclassCode === undefined) {
      throw new TypeError("A filter containing a protocolCode must also contain a subclassCode.");
    }
    return f;
  });
}

function numberField(device: DeviceSummary, key: string): number | undefined {
  const value = stateField(device, key);
  return typeof value === "number" ? value : undefined;
}

/** The USB descriptors of a printer's state (`state.usb`). */
function usbInfo(device: DeviceSummary): Record<string, unknown> {
  const usb = stateField(device, "usb");
  return usb && typeof usb === "object" ? (usb as Record<string, unknown>) : {};
}

function stringInfo(device: DeviceSummary, key: string): string | null {
  const value = usbInfo(device)[key];
  return typeof value === "string" ? value : null;
}

/** Whether a device matches a filter: the device's IDs, and the class of its interface. */
export function matchesUsbFilter(device: DeviceSummary, filter: USBDeviceFilter): boolean {
  if (filter.vendorId !== undefined && numberField(device, "usbVendorId") !== filter.vendorId) {
    return false;
  }
  if (filter.productId !== undefined && numberField(device, "usbProductId") !== filter.productId) {
    return false;
  }
  // The device class is 0 (defined per interface), so class filters match the interface.
  if (filter.classCode !== undefined && filter.classCode !== PRINTER_CLASS) return false;
  if (filter.subclassCode !== undefined && filter.subclassCode !== PRINTER_SUBCLASS) return false;
  if (filter.protocolCode !== undefined && filter.protocolCode !== PRINTER_PROTOCOL) return false;
  if (
    filter.serialNumber !== undefined &&
    stringInfo(device, "serialNumber") !== filter.serialNumber
  ) {
    return false;
  }
  return true;
}

/** Devices that appear as USB devices: printers on the `usb` transport. */
function isUsbDevice(device: DeviceSummary): boolean {
  return device.kind === "printer-escpos" && stateField(device, "transport") === "usb";
}

function hasFault(device: DeviceSummary | undefined, fault: string): boolean {
  return device?.faults.some((f) => f.id === fault) ?? false;
}

/** WebIDL `[EnforceRange] unsigned short` / `octet`. */
function enforceRange(value: unknown, name: string, max: number): number {
  const n = Number(value);
  if (!Number.isFinite(n)) throw new TypeError(`${name} must be a finite number`);
  const t = Math.trunc(n);
  if (t < 0 || t > max) throw new TypeError(`${name} is outside the range 0 to ${String(max)}`);
  return t;
}

function toBytes(data: unknown): Uint8Array {
  if (data instanceof ArrayBuffer) return new Uint8Array(data);
  if (ArrayBuffer.isView(data))
    return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  throw new TypeError("The data must be an ArrayBuffer or an ArrayBufferView.");
}

/** A `USBControlTransferParameters`, checked like WebIDL. */
interface Setup {
  requestType: string;
  recipient: string;
  request: number;
  value: number;
  index: number;
}

function validateSetup(setup: unknown): Setup {
  const s = (setup ?? {}) as Record<string, unknown>;
  for (const key of ["requestType", "recipient", "request", "value", "index"]) {
    if (s[key] === undefined) {
      throw new TypeError(`USBControlTransferParameters is missing required member ${key}.`);
    }
  }
  const requestType = String(s.requestType);
  if (!["standard", "class", "vendor"].includes(requestType)) {
    throw new TypeError(`'${requestType}' is not a valid USBRequestType.`);
  }
  const recipient = String(s.recipient);
  if (!["device", "interface", "endpoint", "other"].includes(recipient)) {
    throw new TypeError(`'${recipient}' is not a valid USBRecipient.`);
  }
  return {
    requestType,
    recipient,
    request: enforceRange(s.request, "request", 0xff),
    value: enforceRange(s.value, "value", 0xffff),
    index: enforceRange(s.index, "index", 0xffff),
  };
}

/** Maps a transport error to the `DOMException` WebUSB uses. */
function toDomError(error: unknown, network: string): unknown {
  if (error instanceof TransportError) {
    switch (error.code) {
      case "not_found":
        return domError("NotFoundError", GONE);
      case "network":
        return domError("NetworkError", network);
      case "invalid_state":
        return domError("InvalidStateError", error.message);
      default:
        return domError("NetworkError", error.message);
    }
  }
  return error;
}

/** `USBDevice`: an emulated USB receipt printer. */
export class HardlessUSBDevice {
  readonly usbVersionMajor = 2;
  readonly usbVersionMinor = 0;
  readonly usbVersionSubminor = 0;
  readonly deviceClass = 0;
  readonly deviceSubclass = 0;
  readonly deviceProtocol = 0;
  readonly deviceVersionMajor = 1;
  readonly deviceVersionMinor = 0;
  readonly deviceVersionSubminor = 0;
  readonly vendorId: number;
  readonly productId: number;
  readonly manufacturerName: string | null;
  readonly productName: string | null;
  readonly serialNumber: string | null;
  readonly configurations: readonly HardlessUSBConfiguration[];
  /** The active configuration; a printer is configured by the OS when it is plugged in. */
  configuration: HardlessUSBConfiguration | null;

  private isOpen = false;
  private claimed = false;
  private gone = false;
  /** Device → host bytes waiting for `transferIn`. */
  private incoming: number[] = [];
  private readers: {
    length: number;
    resolve: (r: HardlessUSBInTransferResult) => void;
    reject: (e: unknown) => void;
  }[] = [];

  constructor(
    readonly deviceId: string,
    summary: DeviceSummary,
    private readonly usb: HardlessUSB,
  ) {
    this.vendorId = numberField(summary, "usbVendorId") ?? 0;
    this.productId = numberField(summary, "usbProductId") ?? 0;
    this.manufacturerName = stringInfo(summary, "manufacturerName");
    this.productName = stringInfo(summary, "productName");
    this.serialNumber = stringInfo(summary, "serialNumber");
    const configuration = new HardlessUSBConfiguration(this);
    this.configurations = Object.freeze([configuration]);
    this.configuration = configuration;
  }

  get opened(): boolean {
    return this.isOpen;
  }

  /** Whether interface 0 is claimed (`USBInterface.claimed`). */
  isClaimed(): boolean {
    return this.claimed;
  }

  private summary(): DeviceSummary | undefined {
    return this.usb.transport.devices().find((d) => d.id === this.deviceId);
  }

  private check(needOpen = true): void {
    if (this.gone) throw domError("NotFoundError", GONE);
    if (needOpen && !this.isOpen) throw domError("InvalidStateError", NOT_OPEN);
  }

  async open(): Promise<void> {
    await this.usb.transport.ready;
    this.check(false);
    this.isOpen = true;
  }

  async close(): Promise<void> {
    this.check(false);
    if (this.claimed) await this.release();
    this.isOpen = false;
  }

  async forget(): Promise<void> {
    if (this.isOpen && !this.gone) await this.close();
    usbGrants.remove(this.deviceId);
  }

  async selectConfiguration(configurationValue: number): Promise<void> {
    this.check();
    const value = enforceRange(configurationValue, "configurationValue", 0xff);
    if (value !== 1) {
      throw domError(
        "NotFoundError",
        "The configuration value provided is not supported by the device.",
      );
    }
    if (this.configuration?.configurationValue === value) return;
    if (this.claimed) await this.release();
    this.configuration = this.configurations[0] ?? null;
  }

  private checkInterface(interfaceNumber: number): void {
    const n = enforceRange(interfaceNumber, "interfaceNumber", 0xff);
    if (!this.configuration) {
      throw domError("InvalidStateError", "The device must have a configuration selected.");
    }
    if (n !== 0) {
      throw domError(
        "NotFoundError",
        "The interface number provided is not supported by the device in its current configuration.",
      );
    }
  }

  async claimInterface(interfaceNumber: number): Promise<void> {
    this.check();
    this.checkInterface(interfaceNumber);
    if (this.claimed) return;
    try {
      await this.usb.transport.open(this.deviceId, {});
    } catch (error) {
      throw toDomError(error, "Unable to claim interface.");
    }
    this.claimed = true;
    this.incoming = [];
  }

  async releaseInterface(interfaceNumber: number): Promise<void> {
    this.check();
    this.checkInterface(interfaceNumber);
    if (this.claimed) await this.release();
  }

  private async release(): Promise<void> {
    this.claimed = false;
    this.failReaders(domError("AbortError", "The transfer was cancelled."));
    await this.usb.transport.close(this.deviceId).catch(() => undefined);
  }

  async selectAlternateInterface(interfaceNumber: number, alternateSetting: number): Promise<void> {
    // Device calls go through the transport, so they wait until it is ready.
    await this.usb.transport.ready;
    this.check();
    this.checkInterface(interfaceNumber);
    if (!this.claimed) throw domError("InvalidStateError", "The interface is not claimed.");
    if (enforceRange(alternateSetting, "alternateSetting", 0xff) !== 0) {
      throw domError(
        "NotFoundError",
        "The alternate setting provided is not supported by the interface.",
      );
    }
  }

  private checkEndpoint(endpointNumber: number, direction: "in" | "out"): void {
    const n = enforceRange(endpointNumber, "endpointNumber", 0xff);
    if (n < 1 || n > 15)
      throw domError("IndexSizeError", "The specified endpoint number is out of range.");
    const wanted = direction === "out" ? OUT_ENDPOINT : IN_ENDPOINT;
    if (!this.claimed || n !== wanted) throw domError("NotFoundError", NOT_CLAIMED);
  }

  async transferOut(endpointNumber: number, data: unknown): Promise<HardlessUSBOutTransferResult> {
    this.check();
    this.checkEndpoint(endpointNumber, "out");
    const bytes = toBytes(data);
    if (hasFault(this.summary(), "stall")) return new HardlessUSBOutTransferResult("stall", 0);
    try {
      await this.usb.transport.write(this.deviceId, bytes.slice());
    } catch (error) {
      if (error instanceof TransportError && error.code === "network") {
        throw domError("NotFoundError", GONE);
      }
      throw toDomError(error, "A transfer error has occurred.");
    }
    return new HardlessUSBOutTransferResult("ok", bytes.byteLength);
  }

  transferIn(endpointNumber: number, length: number): Promise<HardlessUSBInTransferResult> {
    try {
      this.check();
      this.checkEndpoint(endpointNumber, "in");
    } catch (error) {
      return Promise.reject(error instanceof Error ? error : new Error(String(error)));
    }
    const max = enforceRange(length, "length", 0xffffffff);
    return new Promise((resolve, reject) => {
      this.readers.push({ length: max, resolve, reject });
      this.flush();
    });
  }

  /** Called by `navigator.usb`: bytes from the printer, e.g. `DLE EOT` status. */
  receive(bytes: Uint8Array): void {
    if (!this.claimed) return;
    for (const b of bytes) this.incoming.push(b);
    this.flush();
  }

  private flush(): void {
    while (this.readers.length > 0 && this.incoming.length > 0) {
      const reader = this.readers.shift();
      if (!reader) break;
      const chunk = new Uint8Array(this.incoming.splice(0, reader.length));
      reader.resolve(new HardlessUSBInTransferResult("ok", new DataView(chunk.buffer)));
    }
  }

  private failReaders(error: unknown): void {
    for (const reader of this.readers.splice(0)) reader.reject(error);
  }

  async controlTransferIn(setup: unknown, length: number): Promise<HardlessUSBInTransferResult> {
    // Device calls go through the transport, so they wait until it is ready.
    await this.usb.transport.ready;
    this.check();
    const s = validateSetup(setup);
    const max = enforceRange(length, "length", 0xffff);
    if (s.recipient === "interface") {
      // GET_DEVICE_ID puts the interface in the high byte of wIndex; other requests in the low.
      const iface = s.request === GET_DEVICE_ID ? s.index >> 8 : s.index & 0xff;
      if (iface !== 0 || !this.claimed) {
        throw domError("InvalidStateError", "The specified interface is not claimed.");
      }
    }
    if (s.requestType !== "class" || s.recipient !== "interface") {
      // Standard and vendor requests are not emulated: the device stalls them.
      return new HardlessUSBInTransferResult("stall");
    }
    const usb = usbInfo(this.summary() ?? ({ state: null } as unknown as DeviceSummary));
    let reply: number[];
    if (s.request === GET_DEVICE_ID) {
      const id = new TextEncoder().encode(typeof usb.ieee1284Id === "string" ? usb.ieee1284Id : "");
      // A two-byte big-endian length (including itself), then the ID.
      const total = id.length + 2;
      reply = [(total >> 8) & 0xff, total & 0xff, ...id];
    } else if (s.request === GET_PORT_STATUS) {
      reply = [typeof usb.portStatus === "number" ? usb.portStatus : 0];
    } else {
      return new HardlessUSBInTransferResult("stall");
    }
    const bytes = new Uint8Array(reply.slice(0, max));
    return new HardlessUSBInTransferResult("ok", new DataView(bytes.buffer));
  }

  async controlTransferOut(setup: unknown, data?: unknown): Promise<HardlessUSBOutTransferResult> {
    // Device calls go through the transport, so they wait until it is ready.
    await this.usb.transport.ready;
    this.check();
    const s = validateSetup(setup);
    const bytes = data === undefined ? new Uint8Array() : toBytes(data);
    if (s.recipient === "interface" && ((s.index & 0xff) !== 0 || !this.claimed)) {
      throw domError("InvalidStateError", "The specified interface is not claimed.");
    }
    if (s.requestType === "class" && s.recipient === "interface" && s.request === SOFT_RESET) {
      this.incoming = [];
      return new HardlessUSBOutTransferResult("ok", bytes.byteLength);
    }
    return new HardlessUSBOutTransferResult("stall", 0);
  }

  async clearHalt(direction: string, endpointNumber: number): Promise<void> {
    await this.usb.transport.ready;
    this.check();
    if (direction !== "in" && direction !== "out") {
      throw new TypeError(`'${direction}' is not a valid USBDirection.`);
    }
    this.checkEndpoint(endpointNumber, direction);
  }

  async reset(): Promise<void> {
    // Device calls go through the transport, so they wait until it is ready.
    await this.usb.transport.ready;
    this.check();
    this.incoming = [];
    this.failReaders(domError("AbortError", "The device was reset."));
  }

  async isochronousTransferIn(): Promise<never> {
    // Device calls go through the transport, so they wait until it is ready.
    await this.usb.transport.ready;
    this.check();
    throw domError("NotFoundError", NOT_CLAIMED);
  }

  async isochronousTransferOut(): Promise<never> {
    // Device calls go through the transport, so they wait until it is ready.
    await this.usb.transport.ready;
    this.check();
    throw domError("NotFoundError", NOT_CLAIMED);
  }

  /** Called by `navigator.usb` when the device goes away. */
  lost(): void {
    this.gone = true;
    this.isOpen = false;
    this.claimed = false;
    this.failReaders(domError("NotFoundError", GONE));
  }
}

/** `navigator.usb`. */
export class HardlessUSB extends EventTarget {
  onconnect: ((this: HardlessUSB, ev: Event) => unknown) | null = null;
  ondisconnect: ((this: HardlessUSB, ev: Event) => unknown) | null = null;
  /** The current `USBDevice` of each device; a new one after each reconnect, as in Chrome. */
  private readonly objects = new Map<string, HardlessUSBDevice>();

  constructor(readonly transport: Transport) {
    super();
    this.addEventListener("connect", (e) => this.onconnect?.call(this, e));
    this.addEventListener("disconnect", (e) => this.ondisconnect?.call(this, e));
  }

  private object(summary: DeviceSummary): HardlessUSBDevice {
    let device = this.objects.get(summary.id);
    if (!device) {
      device = new HardlessUSBDevice(summary.id, summary, this);
      this.objects.set(summary.id, device);
    }
    return device;
  }

  private usbDevices(): DeviceSummary[] {
    return this.transport.devices().filter((d) => isUsbDevice(d) && isConnected(d));
  }

  /** Called by the transport. */
  handleConnect(deviceId: string): void {
    if (!usbGrants.has(deviceId)) return;
    const summary = this.usbDevices().find((d) => d.id === deviceId);
    if (!summary) return;
    this.dispatchEvent(new HardlessUSBConnectionEvent("connect", { device: this.object(summary) }));
  }

  /** Called by the transport. */
  handleDisconnect(deviceId: string): void {
    const device = this.objects.get(deviceId);
    if (!device) return;
    device.lost();
    this.objects.delete(deviceId);
    if (usbGrants.has(deviceId)) {
      this.dispatchEvent(new HardlessUSBConnectionEvent("disconnect", { device }));
    }
  }

  /** Called by the transport. */
  handleData(deviceId: string, bytes: Uint8Array): void {
    this.objects.get(deviceId)?.receive(bytes);
  }

  async getDevices(): Promise<HardlessUSBDevice[]> {
    await this.transport.ready;
    return this.usbDevices()
      .filter((d) => usbGrants.has(d.id))
      .map((d) => this.object(d));
  }

  async requestDevice(options?: {
    filters?: unknown;
    exclusionFilters?: unknown;
  }): Promise<HardlessUSBDevice> {
    const filters = validateUsbFilters(options?.filters);
    const exclusions =
      options?.exclusionFilters === undefined
        ? []
        : validateUsbFilters(options.exclusionFilters, "exclusionFilters");
    if (options?.exclusionFilters !== undefined && exclusions.length === 0) {
      throw new TypeError("'exclusionFilters', if present, must contain at least one filter.");
    }
    // Older browsers lack navigator.userActivation; then the check cannot be made.
    const activation = (navigator as { userActivation?: { isActive: boolean } }).userActivation;
    if (activation !== undefined && !activation.isActive) {
      throw domError(
        "SecurityError",
        "Failed to execute 'requestDevice' on 'USB': Must be handling a user gesture to show a permission request.",
      );
    }
    await this.transport.ready;
    const candidates = this.usbDevices()
      .filter((d) => filters.length === 0 || filters.some((f) => matchesUsbFilter(d, f)))
      .filter((d) => !exclusions.some((f) => matchesUsbFilter(d, f)));

    const cancel = candidates.find((d) => hasFault(d, "picker-cancel"));
    if (cancel) {
      // `picker-cancel` affects the next request only (PRD section 10).
      await this.transport.clearFault(cancel.id, "picker-cancel").catch(() => undefined);
      throw domError("NotFoundError", "No device selected.");
    }

    const auto = candidates.find((d) => stateField(d, "autoSelect") === true);
    const origin = window.location.origin;
    const chosen =
      auto?.id ??
      (await showPicker(candidates, origin, {
        title: `${origin} wants to connect`,
        label: (d) => `${stringInfo(d, "productName") ?? "USB printer"} (${d.id})`,
      }));
    if (chosen === null) throw domError("NotFoundError", "No device selected.");
    usbGrants.add(chosen);
    const summary = candidates.find((d) => d.id === chosen);
    if (!summary) throw domError("NotFoundError", "No device selected.");
    return this.object(summary);
  }
}
