/**
 * How each fault from PRD section 10 is shown in the fault panel (FR-28): a plain-language
 * name, what it does, and its option with the catalogue default.
 */
export interface FaultInfo {
  name: string;
  effect: string;
  option?: { key: string; label: string; defaultValue: number; min: number; step?: number };
}

export const FAULTS: Record<string, FaultInfo> = {
  "paper-out": { name: "Paper out", effect: "Prints are rejected and status reports paper end." },
  "paper-near-end": { name: "Paper near end", effect: "Prints work; status reports near end." },
  "cover-open": {
    name: "Cover open",
    effect: "Prints are rejected and status reports cover open.",
  },
  offline: { name: "Offline", effect: "The printer disconnects until this is turned off." },
  "disconnect-mid-write": {
    name: "Disconnect mid-print",
    effect: "The printer disconnects partway through the next print.",
    option: { key: "afterBytes", label: "after bytes", defaultValue: 64, min: 0 },
  },
  "slow-write": {
    name: "Slow writes",
    effect: "Each write from the app is acknowledged late.",
    option: { key: "msPerChunk", label: "ms per write", defaultValue: 200, min: 0, step: 50 },
  },
  "baud-mismatch": { name: "Baud rate mismatch", effect: "Received bytes are scrambled." },
  garbled: {
    name: "Garbled bytes",
    effect: "Random bytes are corrupted.",
    option: { key: "rate", label: "rate (0 to 1)", defaultValue: 0.05, min: 0, step: 0.01 },
  },
  timeout: { name: "Timeout", effect: "Responses arrive after the request times out." },
  "cors-blocked": { name: "CORS blocked", effect: "Responses leave out CORS headers." },
  stall: { name: "Stall", effect: "USB transfers return a stall." },
  "interface-busy": { name: "Interface busy", effect: "Claiming the USB interface fails." },
  "picker-cancel": {
    name: "Cancel the next prompt",
    effect: "The next port picker or NFC permission prompt is cancelled by the user.",
  },
  "permission-denied": {
    name: "Permission denied",
    effect: "NFC scanning and writing are refused.",
  },
  "read-error": { name: "Read error", effect: "The next tap fails to read, then this turns off." },
  jammed: { name: "Jammed", effect: "Kicks are counted but the drawer stays shut." },
  "not-connected": {
    name: "Not connected",
    effect: "Kicks are ignored and the printer sees no drawer.",
  },
  "double-scan": {
    name: "Double scan",
    effect: "Each scan is sent twice.",
    option: { key: "gapMs", label: "ms between", defaultValue: 30, min: 0 },
  },
  "partial-scan": { name: "Partial scan", effect: "The terminator is dropped." },
};

/** Display info for a fault, including faults a plugin defines. */
export function faultInfo(id: string): FaultInfo {
  return FAULTS[id] ?? { name: id, effect: "Defined by the device plugin." };
}
