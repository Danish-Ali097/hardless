import type { DeviceSummary } from "@hardless/types";
import { closeDrawer } from "../actions.js";
import { stateOf } from "../devices.js";

/**
 * FR-25: the drawer slides open when kicked and shut when closed, with its kick counter
 * and a button to close it by hand.
 */
export function DrawerPanel({ drawer }: { drawer: DeviceSummary }) {
  const state = stateOf.string(drawer, "state");
  const open = state === "open";
  const opening = state === "opening";
  const kicks = stateOf.number(drawer, "kicks");
  const connected = stateOf.bool(drawer, "connected");
  const words = !connected ? "Not connected" : open ? "Open" : opening ? "Opening" : "Closed";

  return (
    <section aria-labelledby={`drawer-${drawer.id}`} className="flex flex-col gap-3">
      <div className="flex items-baseline justify-between">
        <h2 id={`drawer-${drawer.id}`} className="text-sm font-semibold">
          {drawer.id}
        </h2>
        <span className="text-xs text-muted">pin {stateOf.number(drawer, "pin")}</span>
      </div>

      <div className="relative h-24 overflow-hidden" aria-hidden="true">
        {/* The till: a body with a drawer that slides out towards the viewer. */}
        <div className="absolute inset-x-2 top-2 h-12 rounded-t-md bg-line" />
        <div
          className={`absolute inset-x-0 top-10 h-10 rounded-md border-2 transition-transform duration-300 ease-out ${
            open || opening ? "translate-y-6 border-brass bg-brass-soft" : "border-line bg-raised"
          }`}
        >
          <div className="mx-auto mt-3.5 h-1.5 w-12 rounded-full bg-muted/60" />
        </div>
      </div>

      <p className="text-sm" role="status">
        <span className={open || opening ? "font-semibold text-brass-ink" : "font-semibold"}>
          {words}
        </span>
        <span className="text-muted">
          , kicked {kicks} {kicks === 1 ? "time" : "times"}
        </span>
      </p>

      <button
        type="button"
        disabled={!open && !opening}
        onClick={() => void closeDrawer(drawer.id)}
        className="self-start rounded-md border border-line bg-raised px-3 py-1.5 text-sm font-medium hover:bg-panel disabled:opacity-40"
      >
        Close drawer
      </button>
    </section>
  );
}
