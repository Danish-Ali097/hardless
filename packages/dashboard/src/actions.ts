import type { DeviceSummary, NdefRecord } from "@hardless/types";
import { api, ApiError, subscribe } from "./api.js";
import { applyEvent, firstDevice, needsRefresh, useDashboard } from "./store.js";

const set = useDashboard.setState;
const get = useDashboard.getState;

function report(error: unknown): void {
  const message =
    error instanceof ApiError || error instanceof Error ? error.message : String(error);
  set({ error: message });
}

function putDevice(device: DeviceSummary): void {
  set((s) => ({ devices: { ...s.devices, [device.id]: device } }));
}

/** Loads every device and each printer's recent receipts. */
export async function load(): Promise<void> {
  try {
    const [list, info] = await Promise.all([api.devices(), api.info().catch(() => null)]);
    const devices = Object.fromEntries(list.map((d) => [d.id, d]));
    const printers = list.filter((d) => d.kind === "printer-escpos");
    const jobs = Object.fromEntries(
      await Promise.all(printers.map(async (p) => [p.id, await api.jobs(p.id)] as const)),
    );
    set((s) => ({
      devices,
      jobs,
      info,
      error: null,
      selected: s.selected && devices[s.selected] ? s.selected : firstDevice(devices),
    }));
  } catch (error) {
    report(error);
  }
}

const pending = new Map<string, ReturnType<typeof setTimeout>>();

/** Reloads one device's summary soon, coalescing bursts of state events. */
function refreshSoon(id: string): void {
  clearTimeout(pending.get(id));
  pending.set(
    id,
    setTimeout(() => {
      pending.delete(id);
      api.device(id).then(putDevice, () => undefined);
    }, 150),
  );
}

/** Connects to the live event stream. Returns a function that disconnects. */
export function connect(): () => void {
  return subscribe({
    onOpen: () => {
      set({ connected: true });
      void load();
    },
    onClose: () => {
      set({ connected: false });
    },
    onLagged: () => void load(),
    onEvent: (event) => {
      set((s) => applyEvent(s, event));
      const refresh = needsRefresh(event);
      if (refresh) refreshSoon(refresh);
    },
  });
}

export function select(id: string): void {
  set({ selected: id });
}

export async function setFault(
  id: string,
  fault: string,
  active: boolean,
  options?: Record<string, number>,
): Promise<void> {
  try {
    putDevice(await api.setFault(id, fault, active, options));
  } catch (error) {
    report(error);
  }
}

export async function closeDrawer(id: string): Promise<void> {
  try {
    putDevice(await api.closeDrawer(id));
  } catch (error) {
    report(error);
  }
}

/** Scans a code (FR-26). Returns whether the server took it. */
export async function scan(id: string, code: string): Promise<boolean> {
  try {
    putDevice(await api.scan(id, code));
    return true;
  } catch (error) {
    report(error);
    return false;
  }
}

/** Taps a card on an NFC reader (FR-27). Returns whether the server took it. */
export async function tap(id: string, uid: string, records?: NdefRecord[]): Promise<boolean> {
  try {
    putDevice(await api.tap(id, uid, records));
    return true;
  } catch (error) {
    report(error);
    return false;
  }
}

export async function resetDevice(id: string): Promise<void> {
  try {
    putDevice(await api.resetDevice(id));
    set((s) => ({ jobs: { ...s.jobs, [id]: [] } }));
  } catch (error) {
    report(error);
  }
}

export async function removeDevice(id: string): Promise<void> {
  try {
    await api.removeDevice(id);
  } catch (error) {
    report(error);
  }
}

export async function createDevice(
  kind: string,
  id: string | undefined,
  config: Record<string, unknown>,
): Promise<boolean> {
  try {
    const device = await api.createDevice(kind, id, config);
    putDevice(device);
    set({ selected: device.id, error: null });
    return true;
  } catch (error) {
    report(error);
    return false;
  }
}

/** Hides the receipts printed so far (the printer keeps them). */
export function clearReceipts(id: string): void {
  const newest = get().jobs[id]?.[0]?.at ?? 0;
  set((s) => ({ clearedAt: { ...s.clearedAt, [id]: newest } }));
}

export function setWidth(id: string, width: 58 | 80): void {
  set((s) => ({ widths: { ...s.widths, [id]: width } }));
}

export function setShowUnknown(show: boolean): void {
  set({ showUnknown: show });
}

export function dismissError(): void {
  set({ error: null });
}
