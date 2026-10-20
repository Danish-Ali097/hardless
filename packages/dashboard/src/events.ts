import type {
  ControlCommand,
  Direction,
  EventRecord,
  NfcEvent,
  PeripheralSignal,
} from "@hardless/types";
import { describeRecord } from "./nfc.js";

/** Most records the event log view keeps; the oldest go first. */
export const MAX_LOG_RECORDS = 2_000;

/**
 * Which way a record travelled, as the server's `EventRecord::direction()`: bytes, keystrokes
 * and NFC traffic have one; everything else (state, faults, logs) has none.
 */
export function directionOf(record: EventRecord): Direction | null {
  switch (record.type) {
    case "bytes-in":
      return "host-to-device";
    case "control":
      return record.command.type === "nfc-write" || record.command.type === "nfc-make-read-only"
        ? "host-to-device"
        : null;
    case "bytes-out":
    case "wedge":
      return "device-to-host";
    case "nfc":
      return record.event.type === "reading" || record.event.type === "read-error"
        ? "device-to-host"
        : null;
    default:
      return null;
  }
}

/** Which records the log shows. */
export interface LogFilter {
  /** A device ID, or `null` for every device. */
  device: string | null;
  /** A direction, or `null` for everything (including records without one). */
  direction: Direction | null;
}

export function matches(record: EventRecord, filter: LogFilter): boolean {
  if (filter.device !== null && record.deviceId !== filter.device) return false;
  return filter.direction === null || directionOf(record) === filter.direction;
}

/** Adds `incoming` to `log` (both sorted by `seq`), without duplicates, keeping the newest. */
export function mergeRecords(log: EventRecord[], incoming: EventRecord[]): EventRecord[] {
  if (incoming.length === 0) return log;
  const seen = new Set(log.map((r) => r.seq));
  const merged = [...log, ...incoming.filter((r) => !seen.has(r.seq))].sort(
    (a, b) => a.seq - b.seq,
  );
  return merged.slice(-MAX_LOG_RECORDS);
}

/** Hex bytes as `1b 40 0a`, with a printable-ASCII rendering, cut after `max` bytes. */
export function bytesPreview(hex: string, max = 24): { hex: string; text: string; count: number } {
  const bytes = (hex.match(/../g) ?? []).map((b) => parseInt(b, 16));
  const shown = bytes.slice(0, max);
  const more = bytes.length > max ? " …" : "";
  return {
    count: bytes.length,
    hex: shown.map((b) => b.toString(16).padStart(2, "0")).join(" ") + more,
    text: shown.map((b) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : "·")).join("") + more,
  };
}

function plural(n: number, word: string): string {
  return `${String(n)} ${word}${n === 1 ? "" : "s"}`;
}

function describeCommand(command: ControlCommand): string {
  switch (command.type) {
    case "scan":
      return `Scan ${command.code}`;
    case "tap":
      return command.records === undefined
        ? `Tap card ${command.uid}`
        : `Tap card ${command.uid} with ${plural(command.records.length, "record")}`;
    case "close-drawer":
      return "Close the drawer by hand";
    case "nfc-write":
      return `App writes ${plural(command.records.length, "record")}${
        command.records.length > 0 ? `: ${command.records.map(describeRecord).join("; ")}` : ""
      }`;
    case "nfc-make-read-only":
      return "App makes the card read-only";
    case "custom":
      // The HTTP endpoints end each print request with `close-job`.
      return command.name === "close-job" ? "End of the print request" : `Command ${command.name}`;
  }
}

function describeNfc(event: NfcEvent): string {
  switch (event.type) {
    case "reading":
      return `Card ${event.serialNumber} read, ${plural(event.records.length, "record")}`;
    case "read-error":
      return `Card ${event.serialNumber} could not be read`;
    case "written":
      return `Card ${event.serialNumber} written`;
    case "made-read-only":
      return `Card ${event.serialNumber} is now read-only`;
    case "rejected":
      return `Refused: ${event.message}`;
  }
}

function describeSignal(signal: PeripheralSignal): string {
  return signal.type === "pulse"
    ? `drawer kick on pin ${String(signal.pin)} (${String(signal.onMs)} ms)`
    : `drawer sensor: ${signal.open ? "open" : "closed"}`;
}

/** A record in plain words: a short title, and details shown under it. */
export function describe(record: EventRecord): { title: string; detail?: string } {
  switch (record.type) {
    case "created":
      return { title: `Created (${record.kind})` };
    case "bytes-in":
    case "bytes-out": {
      const p = bytesPreview(record.dataHex);
      const who = record.type === "bytes-in" ? "App sent" : "Device replied with";
      return { title: `${who} ${plural(p.count, "byte")}`, detail: `${p.hex}  ${p.text}` };
    }
    case "control":
      return { title: describeCommand(record.command) };
    case "fault":
      return {
        title: `Fault ${record.faultId} ${record.active ? "on" : "off"}`,
        ...(Object.keys(record.options).length > 0
          ? { detail: JSON.stringify(record.options) }
          : {}),
      };
    case "state-changed":
      return { title: "State changed", detail: JSON.stringify(record.state) };
    case "connected":
      return { title: "Connected" };
    case "disconnected":
      return { title: "Disconnected" };
    case "job-completed":
      return { title: `Receipt ${String(record.jobId)} printed` };
    case "log":
      return {
        title: `${record.level === "error" ? "Error" : record.level === "warn" ? "Warning" : "Note"}: ${record.message}`,
      };
    case "reset":
      return { title: "Reset" };
    case "signals": {
      const s = record.signals;
      const parts = [
        s.dataTerminalReady === undefined ? null : `DTR ${s.dataTerminalReady ? "on" : "off"}`,
        s.requestToSend === undefined ? null : `RTS ${s.requestToSend ? "on" : "off"}`,
        s.break === undefined ? null : `break ${s.break ? "on" : "off"}`,
      ].filter((p) => p !== null);
      return { title: `Signals: ${parts.join(", ") || "none"}` };
    }
    case "peripheral":
      return { title: `${record.from} → ${record.to}: ${describeSignal(record.signal)}` };
    case "wedge":
      return {
        title: `Typed ${record.scan.code}${
          record.scan.terminator === "none"
            ? ""
            : ` + ${record.scan.terminator === "tab" ? "Tab" : "Enter"}`
        }`,
      };
    case "nfc":
      return { title: describeNfc(record.event) };
  }
}

/** Milliseconds since the server started, as `12.345 s`. */
export function formatAt(at: number): string {
  return `${(at / 1000).toFixed(3)} s`;
}
