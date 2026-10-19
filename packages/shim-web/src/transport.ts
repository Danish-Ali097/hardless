import type {
  BridgeErrorCode,
  DeviceSummary,
  SerialSignals,
  Terminator,
  WedgeScan,
} from "@hardless/types";

/** Per-scan overrides of a scanner's settings. */
export interface ScanOptions {
  terminator?: Terminator;
  delayMs?: number;
}

/** Things a transport tells the shim about. */
export interface TransportListener {
  /** The device list changed (added, removed, state or faults). */
  devices(list: DeviceSummary[]): void;
  /** Device -> host bytes for an open port. */
  data(deviceId: string, bytes: Uint8Array): void;
  /** A device became reachable. */
  connect(deviceId: string): void;
  /** A device stopped being reachable (or the bridge dropped). */
  disconnect(deviceId: string): void;
  /** A keyboard-wedge scanner typed a scan (PRD section 8.4). */
  wedge(deviceId: string, scan: WedgeScan): void;
}

/**
 * How the shim reaches the emulated devices (FR-15): the bridge to a Hardless server, or
 * the engine compiled to WebAssembly running in the page.
 */
export interface Transport {
  /** Resolves once the device list is known, or the first connection attempt failed. */
  readonly ready: Promise<void>;
  /** The latest device list. */
  devices(): DeviceSummary[];
  open(deviceId: string, options: Record<string, unknown>): Promise<void>;
  close(deviceId: string): Promise<void>;
  /** Resolves when the device has the bytes (and any slow-write delay has passed). */
  write(deviceId: string, data: Uint8Array): Promise<void>;
  signals(deviceId: string, signals: SerialSignals): Promise<void>;
  /** Turns a fault off, e.g. a one-shot `picker-cancel` once it has been used. */
  clearFault(deviceId: string, fault: string): Promise<void>;
  /** Scans `code` on a keyboard-wedge scanner (FR-26); the wedge types it into the page. */
  scan(deviceId: string, code: string, options?: ScanOptions): Promise<void>;
  /** Disconnects and stops reconnecting. */
  stop(): void;
}

/** A failed transport call, with the bridge error code. */
export class TransportError extends Error {
  constructor(
    readonly code: BridgeErrorCode,
    message: string,
  ) {
    super(message);
    this.name = "TransportError";
  }
}

/** Standard base64 for byte payloads, without depending on Node's Buffer. */
export function toBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(binary);
}

export function fromBase64(text: string): Uint8Array {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** Reads a field of a device's state. */
export function stateField(device: DeviceSummary, key: string): unknown {
  const state = device.state;
  if (state && typeof state === "object" && !Array.isArray(state)) {
    return (state as Record<string, unknown>)[key];
  }
  return undefined;
}

/** Whether a device is reachable. Devices without a `connected` state always are. */
export function isConnected(device: DeviceSummary): boolean {
  return stateField(device, "connected") !== false;
}
