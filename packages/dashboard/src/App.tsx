import type { DeviceSummary } from "@hardless/types";
import { removeDevice, dismissError } from "./actions.js";
import { DeviceList } from "./components/DeviceList.js";
import { DrawerPanel } from "./components/DrawerPanel.js";
import { FaultPanel } from "./components/FaultPanel.js";
import { PrinterPanel } from "./components/PrinterPanel.js";
import { ScannerPanel } from "./components/ScannerPanel.js";
import { kindName, stateOf } from "./devices.js";
import { type DashboardState, useDashboard, visibleJobs } from "./store.js";

/** The dashboard, from a store snapshot (FR-23 to FR-26, FR-28). */
export function Dashboard({ state }: { state: DashboardState }) {
  const devices = Object.values(state.devices).sort((a, b) => a.id.localeCompare(b.id));
  const selected = state.selected ? state.devices[state.selected] : undefined;

  return (
    <div className="flex min-h-full flex-col">
      <header className="flex items-center justify-between border-b border-line px-5 py-3">
        <span className="text-lg font-semibold tracking-tight">Hardless</span>
        <span className="flex items-center gap-2 text-xs text-muted" role="status">
          <span
            aria-hidden="true"
            className={`size-2 rounded-full ${state.connected ? "bg-ok" : "bg-brass"}`}
          />
          {state.connected ? "Live" : "Connecting to the server"}
        </span>
      </header>

      {state.error && (
        <div
          role="alert"
          className="flex items-center justify-between gap-4 bg-brass-soft px-5 py-2 text-sm text-brass-ink"
        >
          <span>{state.error}</span>
          <button type="button" onClick={dismissError} className="font-medium underline">
            Dismiss
          </button>
        </div>
      )}

      <div className="grid flex-1 grid-cols-1 gap-6 p-5 md:grid-cols-[14rem_minmax(0,1fr)] xl:grid-cols-[14rem_minmax(0,1fr)_19rem]">
        <aside>
          <DeviceList devices={devices} selected={state.selected} />
        </aside>
        <main className="min-w-0">
          {selected ? (
            <Selected state={state} device={selected} />
          ) : (
            <p className="py-16 text-center text-muted">
              No devices yet. Add a receipt printer to start.
            </p>
          )}
        </main>
        {selected && (
          <aside className="flex flex-col gap-8 md:col-start-2 xl:col-start-auto">
            <Rail state={state} device={selected} devices={devices} />
          </aside>
        )}
      </div>
    </div>
  );
}

function Selected({ state, device }: { state: DashboardState; device: DeviceSummary }) {
  if (device.kind === "printer-escpos") {
    const own = stateOf.number(device, "paperWidth") === 58 ? 58 : 80;
    return (
      <PrinterPanel
        printer={device}
        jobs={visibleJobs(state, device.id)}
        width={state.widths[device.id] ?? own}
        showUnknown={state.showUnknown}
      />
    );
  }
  if (device.kind === "scanner-wedge") return <ScannerPanel scanner={device} />;
  return (
    <section className="flex flex-col gap-1 border-b border-line pb-4">
      <h1 className="text-2xl font-semibold tracking-tight">{device.id}</h1>
      <p className="text-sm text-muted">
        {kindName(device.kind)}
        {device.attachedTo ? ` attached to ${device.attachedTo}` : ""}
      </p>
    </section>
  );
}

/** The right rail: the device's faults, and the drawer that belongs with it. */
function Rail({
  device,
  devices,
}: {
  state: DashboardState;
  device: DeviceSummary;
  devices: DeviceSummary[];
}) {
  const drawers =
    device.kind === "cash-drawer"
      ? [device]
      : devices.filter((d) => d.kind === "cash-drawer" && d.attachedTo === device.id);
  return (
    <>
      {drawers.map((drawer) => (
        <DrawerPanel key={drawer.id} drawer={drawer} />
      ))}
      <FaultPanel device={device} />
      {drawers
        .filter((d) => d.id !== device.id)
        .map((drawer) => (
          <FaultPanel key={drawer.id} device={drawer} />
        ))}
      <button
        type="button"
        onClick={() => {
          if (window.confirm(`Remove ${device.id}? Its receipts and event log go with it.`)) {
            void removeDevice(device.id);
          }
        }}
        className="self-start text-sm text-muted underline-offset-2 hover:text-danger hover:underline"
      >
        Remove {device.id}
      </button>
    </>
  );
}

/** The dashboard bound to the live store. */
export function App() {
  const state = useDashboard();
  return <Dashboard state={state} />;
}
