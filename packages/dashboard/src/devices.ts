import type { DeviceSummary } from "@hardless/types";

/** A device kind in plain words. */
export function kindName(kind: string): string {
  switch (kind) {
    case "printer-escpos":
      return "Receipt printer";
    case "cash-drawer":
      return "Cash drawer";
    case "scanner-wedge":
      return "Barcode scanner";
    case "nfc-reader":
      return "NFC reader";
    default:
      return kind;
  }
}

function field(device: DeviceSummary, key: string): unknown {
  const state = device.state;
  if (state && typeof state === "object" && !Array.isArray(state)) {
    return (state as Record<string, unknown>)[key];
  }
  return undefined;
}

/** Reads a field of a device's state. */
export const stateOf = {
  bool: (d: DeviceSummary, key: string) => field(d, key) === true,
  number: (d: DeviceSummary, key: string) => {
    const value = field(d, key);
    return typeof value === "number" ? value : 0;
  },
  string: (d: DeviceSummary, key: string) => {
    const value = field(d, key);
    return typeof value === "string" ? value : "";
  },
};

/** How a device is doing, in a few words, and whether it needs attention. */
export function status(device: DeviceSummary): { text: string; tone: "ok" | "attention" | "off" } {
  if (device.kind === "printer-escpos") {
    if (!stateOf.bool(device, "connected")) return { text: "Disconnected", tone: "off" };
    if (stateOf.bool(device, "paperOut")) return { text: "Paper out", tone: "attention" };
    if (stateOf.bool(device, "coverOpen")) return { text: "Cover open", tone: "attention" };
    if (stateOf.bool(device, "paperNearEnd")) return { text: "Paper low", tone: "attention" };
    return { text: "Online", tone: "ok" };
  }
  if (device.kind === "cash-drawer") {
    if (!stateOf.bool(device, "connected")) return { text: "Not connected", tone: "off" };
    const drawer = stateOf.string(device, "state");
    if (drawer === "open") return { text: "Open", tone: "attention" };
    if (drawer === "opening") return { text: "Opening", tone: "attention" };
    return { text: "Closed", tone: "ok" };
  }
  if (device.kind === "scanner-wedge") return { text: "Ready", tone: "ok" };
  if (device.kind === "nfc-reader") {
    return stateOf.string(device, "card")
      ? { text: "Card on the reader", tone: "ok" }
      : { text: "Ready", tone: "ok" };
  }
  return { text: "", tone: "ok" };
}

/** Lowercase hex grouped in bytes, 16 per line. */
export function hexDump(hex: string): string {
  const bytes = hex.match(/../g) ?? [];
  const lines: string[] = [];
  for (let i = 0; i < bytes.length; i += 16) lines.push(bytes.slice(i, i + 16).join(" "));
  return lines.join("\n");
}
