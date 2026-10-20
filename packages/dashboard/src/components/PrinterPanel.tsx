import type { DeviceSummary, Job } from "@hardless/types";
import { clearReceipts, resetDevice, setShowUnknown, setWidth } from "../actions.js";
import { status, stateOf } from "../devices.js";
import type { ServerInfo } from "../api.js";
import { HttpPrinterHelp } from "./HttpPrinterHelp.js";
import { Receipt } from "./Receipt.js";

/**
 * FR-24: the printer's output tray. Receipts stack newest first as they print, at the
 * chosen paper width, with their raw bytes and PNG and HTML downloads.
 */
export function PrinterPanel({
  printer,
  jobs,
  width,
  showUnknown,
  info = null,
  isDefaultHttp = false,
}: {
  printer: DeviceSummary;
  info?: ServerInfo | null;
  isDefaultHttp?: boolean;
  jobs: Job[];
  width: 58 | 80;
  showUnknown: boolean;
}) {
  const s = status(printer);
  const connected = stateOf.bool(printer, "connected");
  const http = stateOf.string(printer, "transport") === "http";
  return (
    <section aria-labelledby="printer-title" className="flex min-h-0 flex-col">
      <header className="flex flex-wrap items-end justify-between gap-x-6 gap-y-3 border-b border-line pb-4">
        <div>
          <h1 id="printer-title" className="text-2xl font-semibold tracking-tight">
            {printer.id}
          </h1>
          <p className="text-sm text-muted">
            {http ? "HTTP receipt printer" : "Receipt printer on Web Serial"},{" "}
            {stateOf.number(printer, "paperWidth")} mm paper,{" "}
            <span className={s.tone === "ok" ? "text-ok" : connected ? "text-brass-ink" : ""}>
              {s.text.toLowerCase()}
            </span>
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <div
            role="radiogroup"
            aria-label="Paper width"
            className="flex rounded-md border border-line bg-raised p-0.5"
          >
            {([58, 80] as const).map((mm) => (
              <button
                key={mm}
                type="button"
                role="radio"
                aria-checked={width === mm}
                onClick={() => {
                  setWidth(printer.id, mm);
                }}
                className={`rounded px-2.5 py-1 ${width === mm ? "bg-ink text-counter" : "text-muted hover:text-ink"}`}
              >
                {mm} mm
              </button>
            ))}
          </div>
          <label className="flex items-center gap-1.5 px-2 text-muted">
            <input
              type="checkbox"
              checked={showUnknown}
              onChange={(e) => {
                setShowUnknown(e.target.checked);
              }}
            />
            Mark unknown commands
          </label>
          <button
            type="button"
            onClick={() => {
              clearReceipts(printer.id);
            }}
            className="rounded-md px-2.5 py-1 text-muted hover:bg-panel hover:text-ink"
          >
            Clear
          </button>
          <button
            type="button"
            onClick={() => void resetDevice(printer.id)}
            className="rounded-md px-2.5 py-1 text-muted hover:bg-panel hover:text-ink"
          >
            Reset printer
          </button>
        </div>
      </header>

      {http && <HttpPrinterHelp printer={printer} info={info} isDefault={isDefaultHttp} />}

      <div className="flex flex-col items-center gap-8 py-8" aria-live="polite">
        {jobs.length === 0 ? (
          <EmptyTray id={printer.id} http={http} />
        ) : (
          jobs.map((job) => (
            <Receipt
              key={`${String(job.id)}-${String(job.at)}`}
              deviceId={printer.id}
              job={job}
              width={width}
              showUnknown={showUnknown}
            />
          ))
        )}
      </div>
    </section>
  );
}

function EmptyTray({ id, http }: { id: string; http: boolean }) {
  return (
    <div className="max-w-sm py-16 text-center text-muted">
      <p className="text-base text-ink">Nothing printed yet.</p>
      <p className="mt-1">
        Receipts from your app appear here as they print.{" "}
        {http
          ? "Send a print to one of the addresses above."
          : `Connect to ${id} from the Web Serial port picker and print.`}
      </p>
    </div>
  );
}
