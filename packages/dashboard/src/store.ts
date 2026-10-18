import type { BusEvent, DeviceSummary, Job } from "@hardless/types";
import { create } from "zustand";

/** How many receipts the dashboard keeps per printer. */
export const MAX_JOBS = 50;

/** Dashboard state: devices, their receipts, and what the user picked. */
export interface DashboardState {
  devices: Record<string, DeviceSummary>;
  /** Receipts per device, newest first. */
  jobs: Record<string, Job[]>;
  /** Receipts printed at or before this time were cleared from view, per device. Job ids
   * restart when a device is reset, so the print time is used instead. */
  clearedAt: Record<string, number>;
  selected: string | null;
  connected: boolean;
  /** Paper width per printer view; defaults to the printer's own. */
  widths: Record<string, 58 | 80>;
  showUnknown: boolean;
  error: string | null;
}

export const initialState: DashboardState = {
  devices: {},
  jobs: {},
  clearedAt: {},
  selected: null,
  connected: false,
  widths: {},
  showUnknown: true,
  error: null,
};

/** Applies one event bus event. Pure, so it can be tested without a server. */
export function applyEvent(state: DashboardState, event: BusEvent): Partial<DashboardState> {
  switch (event.type) {
    case "device:added":
      // The summary arrives separately (see `needsRefresh`).
      return {};
    case "device:removed": {
      const devices = Object.fromEntries(
        Object.entries(state.devices).filter(([id]) => id !== event.deviceId),
      );
      const jobs = Object.fromEntries(
        Object.entries(state.jobs).filter(([id]) => id !== event.deviceId),
      );
      const selected = state.selected === event.deviceId ? firstDevice(devices) : state.selected;
      return { devices, jobs, selected };
    }
    case "device:state": {
      const device = state.devices[event.deviceId];
      if (!device) return {};
      return { devices: { ...state.devices, [event.deviceId]: { ...device, state: event.state } } };
    }
    case "device:output": {
      const existing = state.jobs[event.deviceId] ?? [];
      if (existing.some((j) => j.id === event.job.id && j.at === event.job.at)) return {};
      const jobs = [event.job, ...existing].slice(0, MAX_JOBS);
      return { jobs: { ...state.jobs, [event.deviceId]: jobs } };
    }
  }
}

/**
 * Whether an event means the device's summary (faults, attachment) should be reloaded: bus
 * state events carry the device state but not its active faults.
 */
export function needsRefresh(event: BusEvent): string | null {
  return event.type === "device:added" || event.type === "device:state" ? event.deviceId : null;
}

/** The first device by id, printers first, for the initial selection. */
export function firstDevice(devices: Record<string, DeviceSummary>): string | null {
  const list = Object.values(devices).sort((a, b) =>
    a.kind === b.kind ? a.id.localeCompare(b.id) : a.kind === "printer-escpos" ? -1 : 1,
  );
  return list[0]?.id ?? null;
}

/** Receipts still in view for a device. */
export function visibleJobs(state: DashboardState, deviceId: string): Job[] {
  const cleared = state.clearedAt[deviceId] ?? -1;
  return (state.jobs[deviceId] ?? []).filter((j) => j.at > cleared);
}

export const useDashboard = create<DashboardState>(() => initialState);
