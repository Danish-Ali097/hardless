import type { DeviceSummary, Job } from "@hardless/types";
import { initialState, type DashboardState } from "../src/store.js";

export function printer(overrides: Partial<DeviceSummary> = {}, state: object = {}): DeviceSummary {
  return {
    id: "printer-1",
    kind: "printer-escpos",
    state: { connected: true, paperOut: false, coverOpen: false, paperWidth: 80, ...state },
    faults: [],
    supportedFaults: [
      "paper-out",
      "cover-open",
      "offline",
      "disconnect-mid-write",
      "slow-write",
      "picker-cancel",
    ],
    attachedTo: null,
    ...overrides,
  };
}

export function drawer(state: object = {}, overrides: Partial<DeviceSummary> = {}): DeviceSummary {
  return {
    id: "drawer-1",
    kind: "cash-drawer",
    state: { state: "closed", kicks: 0, pin: 2, connected: true, ...state },
    faults: [],
    supportedFaults: ["jammed", "not-connected"],
    attachedTo: "printer-1",
    ...overrides,
  };
}

export function job(id: number, at = id * 100): Job {
  return { id, at, ast: [], text: `Receipt ${String(id)}\n`, bytesHex: "1b40" };
}

export function stateWith(
  devices: DeviceSummary[],
  extra: Partial<DashboardState> = {},
): DashboardState {
  return {
    ...initialState,
    devices: Object.fromEntries(devices.map((d) => [d.id, d])),
    selected: devices[0]?.id ?? null,
    connected: true,
    ...extra,
  };
}
