import type { DeviceSummary, NdefRecord, NfcReading } from "@hardless/types";
import { fromBase64, stateField, toBase64, TransportError, type Transport } from "./transport.js";

/**
 * `NDEFReader`, `NDEFReadingEvent`, `NDEFMessage` and `NDEFRecord`, following the W3C Web NFC
 * specification (PRD section 8.3, FR-16). Cards are tapped on emulated `nfc-reader` devices
 * from the dashboard, REST API, CLI, a scenario or the Playwright fixture.
 */

function domError(name: string, message: string): DOMException {
  return new DOMException(message, name);
}

/**
 * The error an aborted call rejects with: the signal's reason, as in the spec (an
 * `AbortError` unless the caller gave another reason).
 */
function abortError(signal: AbortSignal): Error {
  const reason: unknown = signal.reason;
  // Any value can be a reason; the spec rejects with it unchanged.
  return (reason ?? domError("AbortError", "The operation was aborted.")) as Error;
}

/** Reads `aborted` afresh (after an `await`, the signal may have been aborted). */
function isAborted(signal: AbortSignal | undefined): boolean {
  return signal?.aborted === true;
}

/** Maps a transport error to the `DOMException` Web NFC uses. */
function toDomError(error: unknown): unknown {
  if (error instanceof TransportError) {
    switch (error.code) {
      case "not_allowed":
        return domError("NotAllowedError", error.message);
      case "network":
        return domError("NetworkError", error.message);
      case "not_found":
        return domError("NotFoundError", error.message);
      default:
        return domError("NotReadableError", error.message);
    }
  }
  return error;
}

/** The document's language, Web NFC's default `lang` for text records. */
function documentLang(): string {
  return document.documentElement.lang || "en";
}

/** A text record's payload in its `encoding` (big-endian for plain `utf-16`). */
function encodeText(text: string, encoding: string | undefined): Uint8Array {
  if (encoding === "utf-16" || encoding === "utf-16be" || encoding === "utf-16le") {
    const bytes = new Uint8Array(text.length * 2);
    const view = new DataView(bytes.buffer);
    for (let i = 0; i < text.length; i++) {
      view.setUint16(i * 2, text.charCodeAt(i), encoding === "utf-16le");
    }
    return bytes;
  }
  return new TextEncoder().encode(text);
}

/** The payload bytes of a wire record; `null` for an `empty` record. */
function payload(record: NdefRecord): Uint8Array | null {
  if (record.recordType === "empty") return null;
  if (record.data !== undefined) {
    return record.recordType === "text"
      ? encodeText(record.data, record.encoding)
      : new TextEncoder().encode(record.data);
  }
  return record.dataBase64 === undefined ? new Uint8Array() : fromBase64(record.dataBase64);
}

function isBufferSource(value: unknown): value is ArrayBuffer | ArrayBufferView {
  return value instanceof ArrayBuffer || ArrayBuffer.isView(value);
}

function bytesOf(value: ArrayBuffer | ArrayBufferView): Uint8Array {
  return value instanceof ArrayBuffer
    ? new Uint8Array(value)
    : new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
}

/**
 * Turns an `NDEFRecordInit` into a wire record, checking it like the spec's "create an NDEF
 * record" steps: `TypeError` for bad input, `NotSupportedError` for nested messages, which
 * Hardless does not emulate yet.
 */
export function toWireRecord(init: unknown, defaultLang = "en"): NdefRecord {
  if (!init || typeof init !== "object") {
    throw new TypeError("An NDEFRecordInit must be an object.");
  }
  const r = init as Record<string, unknown>;
  const recordType = r.recordType;
  if (typeof recordType !== "string" || recordType === "") {
    throw new TypeError("NDEFRecordInit.recordType is required.");
  }
  const record: NdefRecord = { recordType };
  if (typeof r.id === "string") record.id = r.id;
  const data = r.data;
  const binary = (what: string) => {
    if (!isBufferSource(data)) throw new TypeError(`The data of ${what} must be a BufferSource.`);
    record.dataBase64 = toBase64(bytesOf(data));
  };
  switch (recordType) {
    case "empty":
      if (data !== undefined && data !== null) {
        throw new TypeError("An 'empty' record has no data.");
      }
      break;
    case "text": {
      const encoding = r.encoding;
      if (encoding !== undefined) {
        if (typeof encoding !== "string" || !ENCODINGS.has(encoding)) {
          throw new TypeError(
            `Unsupported text encoding '${typeof encoding === "string" ? encoding : typeof encoding}'.`,
          );
        }
        record.encoding = encoding;
      }
      if (typeof data === "string") {
        if (encoding !== undefined && encoding !== "utf-8") {
          throw new TypeError("A text record given as a string is encoded as utf-8.");
        }
        record.data = data;
      } else {
        binary("a 'text' record given as bytes");
      }
      record.lang = typeof r.lang === "string" ? r.lang : defaultLang;
      break;
    }
    case "url":
    case "absolute-url":
      if (typeof data !== "string") {
        throw new TypeError(`The data of a '${recordType}' record must be a string.`);
      }
      record.data = data;
      break;
    case "mime":
      binary("a 'mime' record");
      record.mediaType = typeof r.mediaType === "string" ? r.mediaType : "application/octet-stream";
      break;
    case "unknown":
      binary("an 'unknown' record");
      break;
    case "smart-poster":
      throw domError("NotSupportedError", "Hardless does not emulate smart-poster records yet.");
    default:
      if (recordType.startsWith(":")) {
        throw new TypeError("Local record types are only allowed inside nested messages.");
      }
      if (!recordType.includes(":")) throw new TypeError(`Unknown recordType '${recordType}'.`);
      if (data !== undefined && !isBufferSource(data)) {
        throw domError("NotSupportedError", "Hardless does not emulate nested messages yet.");
      }
      if (data !== undefined) record.dataBase64 = toBase64(bytesOf(data));
  }
  return record;
}

const ENCODINGS = new Set(["utf-8", "utf-16", "utf-16be", "utf-16le"]);

/** An `NDEFMessageSource` (string, BufferSource or `NDEFMessageInit`) as wire records. */
export function toWireMessage(message: unknown, defaultLang = "en"): NdefRecord[] {
  if (typeof message === "string") {
    return [{ recordType: "text", data: message, lang: defaultLang }];
  }
  if (isBufferSource(message)) {
    return [
      {
        recordType: "mime",
        mediaType: "application/octet-stream",
        dataBase64: toBase64(bytesOf(message)),
      },
    ];
  }
  const records = (message as { records?: unknown } | null)?.records;
  if (Array.isArray(records)) {
    if (records.length === 0) throw new TypeError("An NDEF message needs at least one record.");
    return records.map((r) => toWireRecord(r, defaultLang));
  }
  throw new TypeError("The message must be a string, a BufferSource or an NDEFMessageInit.");
}

/** Marks an init that already holds a wire record (a card read), so it is not re-checked. */
const WIRE = Symbol("hardless.wire");

interface WireInit {
  [WIRE]: NdefRecord;
}

function isWireInit(init: unknown): init is WireInit {
  return typeof init === "object" && init !== null && WIRE in init;
}

/** `NDEFRecord`. */
export class HardlessNDEFRecord {
  readonly recordType: string;
  readonly mediaType: string | null;
  readonly id: string | null;
  readonly data: DataView | null;
  readonly encoding: string | null;
  readonly lang: string | null;

  constructor(init: unknown) {
    const record = isWireInit(init) ? init[WIRE] : toWireRecord(init, documentLang());
    this.recordType = record.recordType;
    this.mediaType =
      record.recordType === "mime" ? (record.mediaType ?? "application/octet-stream") : null;
    this.id = record.id ?? null;
    const bytes = payload(record);
    this.data =
      bytes === null ? null : new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const text = record.recordType === "text";
    this.encoding = text ? (record.encoding ?? "utf-8") : null;
    this.lang = text ? (record.lang ?? documentLang()) : null;
  }

  /** Records nested in this one; Hardless does not emulate nested messages yet. */
  toRecords(): HardlessNDEFRecord[] | null {
    return null;
  }
}

/** `NDEFMessage`. */
export class HardlessNDEFMessage {
  readonly records: readonly HardlessNDEFRecord[];

  constructor(init: unknown) {
    const records = (init as { records?: unknown } | null)?.records;
    if (!Array.isArray(records)) {
      throw new TypeError(
        "Failed to construct 'NDEFMessage': required member records is undefined.",
      );
    }
    this.records = Object.freeze(records.map((r: unknown) => new HardlessNDEFRecord(r)));
  }
}

/** `NDEFReadingEvent`: the `reading` event, fired when a card is tapped. */
export class HardlessNDEFReadingEvent extends Event {
  readonly serialNumber: string;
  readonly message: HardlessNDEFMessage;

  constructor(type: string, init: { serialNumber?: string; message?: unknown } & EventInit) {
    super(type, init);
    this.serialNumber = init.serialNumber ?? "";
    this.message = new HardlessNDEFMessage(init.message);
  }
}

function isReader(device: DeviceSummary): boolean {
  return device.kind === "nfc-reader";
}

function hasFault(device: DeviceSummary, fault: string): boolean {
  return device.faults.some((f) => f.id === fault);
}

/** One signal that aborts when any of `signals` does. */
function anySignal(signals: (AbortSignal | undefined)[]): AbortSignal {
  const controller = new AbortController();
  for (const signal of signals) {
    if (!signal) continue;
    if (signal.aborted) {
      controller.abort(signal.reason);
      break;
    }
    signal.addEventListener(
      "abort",
      () => {
        controller.abort(signal.reason);
      },
      { once: true },
    );
  }
  return controller.signal;
}

/**
 * What every `NDEFReader` of the page shares: the transport, the readers to notify of taps,
 * writes waiting for a card, and whether NFC permission has been granted.
 */
export class NfcHub {
  private readonly readers = new Set<HardlessNDEFReader>();
  private readonly waiting = new Set<(deviceId: string) => void>();
  /** Whether the page was granted NFC: the first prompt needs a user gesture. */
  granted = false;

  constructor(readonly transport: Transport) {}

  /** Called by each new `NDEFReader`. */
  add(reader: HardlessNDEFReader): void {
    this.readers.add(reader);
  }

  /** Called by the transport for every tap. */
  handleReading(deviceId: string, reading: NfcReading, error: boolean): void {
    for (const reader of this.readers) reader.deliver(reading, error);
    if (!error) for (const wake of [...this.waiting]) wake(deviceId);
  }

  /** Resolves with the next reader a card is tapped on. */
  private nextTap(signal: AbortSignal): Promise<string> {
    return new Promise((resolve, reject) => {
      const onAbort = () => {
        this.waiting.delete(wake);
        reject(abortError(signal));
      };
      const wake = (deviceId: string) => {
        this.waiting.delete(wake);
        signal.removeEventListener("abort", onAbort);
        resolve(deviceId);
      };
      this.waiting.add(wake);
      signal.addEventListener("abort", onAbort, { once: true });
    });
  }

  /**
   * The permission step of `scan()`, `write()` and `makeReadOnly()`: the `permission-denied`
   * and `picker-cancel` faults, and a first prompt that needs transient user activation.
   */
  async checkPermission(method: string): Promise<void> {
    await this.transport.ready;
    const readers = this.transport.devices().filter(isReader);
    if (readers.length === 0) {
      throw domError("NotSupportedError", "No NFC reader is emulated (add an nfc-reader).");
    }
    const prefix = `Failed to execute '${method}' on 'NDEFReader'`;
    if (readers.some((d) => hasFault(d, "permission-denied"))) {
      throw domError("NotAllowedError", `${prefix}: NFC permission request denied.`);
    }
    const cancel = readers.find((d) => hasFault(d, "picker-cancel"));
    if (cancel) {
      // A dismissed permission prompt fails this call only (PRD section 10).
      await this.transport.clearFault(cancel.id, "picker-cancel").catch(() => undefined);
      throw domError("NotAllowedError", `${prefix}: NFC permission request was dismissed.`);
    }
    if (this.granted) return;
    // Older browsers lack navigator.userActivation; then the check cannot be made.
    const activation = (navigator as { userActivation?: { isActive: boolean } }).userActivation;
    if (activation !== undefined && !activation.isActive) {
      throw domError(
        "SecurityError",
        `${prefix}: Must be handling a user gesture to show a permission request.`,
      );
    }
    this.granted = true;
  }

  /**
   * Runs `op` on the reader a card is on, waiting for a tap when there is none, as Web NFC
   * writes to the next tag that comes into range.
   */
  async onCard(signal: AbortSignal, op: (deviceId: string) => Promise<void>): Promise<void> {
    let deviceId = this.transport.devices().find((d) => isReader(d) && stateField(d, "card"))?.id;
    for (;;) {
      if (signal.aborted) throw abortError(signal);
      deviceId ??= await this.nextTap(signal);
      if (isAborted(signal)) throw abortError(signal);
      try {
        await op(deviceId);
        return;
      } catch (error) {
        // The card left before the write reached it: wait for the next tap.
        if (error instanceof TransportError && error.code === "invalid_state") {
          deviceId = undefined;
          continue;
        }
        throw toDomError(error);
      }
    }
  }
}

/** `NDEFReader`. */
export class HardlessNDEFReader extends EventTarget {
  onreading: ((this: HardlessNDEFReader, ev: Event) => unknown) | null = null;
  onreadingerror: ((this: HardlessNDEFReader, ev: Event) => unknown) | null = null;
  /** The signal of the running `scan()` (`null` if it has none); `undefined` when idle. */
  private scanning: AbortSignal | null | undefined;
  private pendingWrite: AbortController | null = null;
  private pendingLock: AbortController | null = null;

  constructor(private readonly hub: NfcHub) {
    super();
    this.addEventListener("reading", (e) => this.onreading?.call(this, e));
    this.addEventListener("readingerror", (e) => this.onreadingerror?.call(this, e));
    hub.add(this);
  }

  /** Called by the hub for every tap; fires only while scanning. */
  deliver(reading: NfcReading, error: boolean): void {
    if (this.scanning === undefined) return;
    if (error) {
      this.dispatchEvent(new Event("readingerror"));
      return;
    }
    const records = reading.records.map((record): WireInit => ({ [WIRE]: record }));
    this.dispatchEvent(
      new HardlessNDEFReadingEvent("reading", {
        serialNumber: reading.serialNumber,
        message: { records },
      }),
    );
  }

  /** Starts listening for taps; `signal` stops it (PRD section 8.3). */
  async scan(options: { signal?: AbortSignal } = {}): Promise<void> {
    const { signal } = options;
    if (signal?.aborted) throw abortError(signal);
    if (this.scanning !== undefined) {
      throw domError(
        "InvalidStateError",
        "Failed to execute 'scan' on 'NDEFReader': A scan() operation is ongoing.",
      );
    }
    await this.hub.checkPermission("scan");
    if (signal && isAborted(signal)) throw abortError(signal);
    this.scanning = signal ?? null;
    signal?.addEventListener(
      "abort",
      () => {
        if (this.scanning === signal) this.scanning = undefined;
      },
      { once: true },
    );
  }

  /**
   * Writes to the card on the reader, or to the next one tapped. A newer `write()` aborts a
   * pending one, as in the spec.
   */
  async write(
    message: unknown,
    options: { overwrite?: boolean; signal?: AbortSignal } = {},
  ): Promise<void> {
    const { signal } = options;
    if (signal?.aborted) throw abortError(signal);
    const records = toWireMessage(message, documentLang());
    await this.hub.checkPermission("write");
    this.pendingWrite?.abort(domError("AbortError", "A newer write() replaced this one."));
    const controller = new AbortController();
    this.pendingWrite = controller;
    try {
      await this.hub.onCard(anySignal([signal, controller.signal]), (deviceId) =>
        this.hub.transport.nfcWrite(deviceId, records, options.overwrite ?? true),
      );
    } finally {
      if (this.pendingWrite === controller) this.pendingWrite = null;
    }
  }

  /** Makes the card on the reader, or the next one tapped, read-only. */
  async makeReadOnly(options: { signal?: AbortSignal } = {}): Promise<void> {
    const { signal } = options;
    if (signal?.aborted) throw abortError(signal);
    await this.hub.checkPermission("makeReadOnly");
    this.pendingLock?.abort(domError("AbortError", "A newer makeReadOnly() replaced this one."));
    const controller = new AbortController();
    this.pendingLock = controller;
    try {
      await this.hub.onCard(anySignal([signal, controller.signal]), (deviceId) =>
        this.hub.transport.nfcMakeReadOnly(deviceId),
      );
    } finally {
      if (this.pendingLock === controller) this.pendingLock = null;
    }
  }
}
