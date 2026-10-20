import type { DeviceSummary } from "@hardless/types";
import { useState, type FormEvent } from "react";
import { createDevice, select } from "../actions.js";
import { kindName, status } from "../devices.js";

const DOT = {
  ok: "bg-ok",
  attention: "bg-brass",
  off: "bg-muted/50",
} as const;

/** FR-23: every device with its live status, and a form to add one. */
export function DeviceList({
  devices,
  selected,
}: {
  devices: DeviceSummary[];
  selected: string | null;
}) {
  return (
    <nav aria-label="Devices" className="flex flex-col gap-4">
      <ul className="flex flex-col gap-1">
        {devices.map((device) => {
          const s = status(device);
          const active = device.id === selected;
          return (
            <li key={device.id}>
              <button
                type="button"
                onClick={() => {
                  select(device.id);
                }}
                aria-current={active ? "true" : undefined}
                className={`flex w-full items-start gap-3 rounded-md px-3 py-2 text-left ${
                  active ? "bg-raised shadow-sm" : "hover:bg-panel"
                }`}
              >
                <span
                  aria-hidden="true"
                  className={`mt-1.5 size-2 shrink-0 rounded-full ${DOT[s.tone]}`}
                />
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-medium">{device.id}</span>
                  <span className="block text-xs text-muted">
                    {kindName(device.kind)}, {s.text.toLowerCase()}
                    {device.faults.length > 0 && (
                      <span className="text-brass-ink">
                        , {device.faults.length} {device.faults.length === 1 ? "fault" : "faults"}
                      </span>
                    )}
                  </span>
                </span>
              </button>
            </li>
          );
        })}
      </ul>
      <AddDevice printers={devices.filter((d) => d.kind === "printer-escpos")} />
    </nav>
  );
}

function AddDevice({ printers }: { printers: DeviceSummary[] }) {
  const [open, setOpen] = useState(false);
  const [kind, setKind] = useState("printer-escpos");
  const [id, setId] = useState("");
  const [attachedTo, setAttachedTo] = useState("");
  const [pin, setPin] = useState("2");

  if (!open) {
    return (
      <button
        type="button"
        onClick={() => {
          setOpen(true);
        }}
        className="self-start rounded-md px-3 py-1.5 text-sm text-muted hover:bg-panel hover:text-ink"
      >
        Add device
      </button>
    );
  }

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const config: Record<string, unknown> =
      kind === "cash-drawer"
        ? { attachedTo: attachedTo || printers[0]?.id, pin: Number(pin) }
        : kind === "printer-http"
          ? { transport: "http" }
          : {};
    const deviceKind = kind === "printer-http" ? "printer-escpos" : kind;
    if (await createDevice(deviceKind, id.trim() || undefined, config)) {
      setOpen(false);
      setId("");
    }
  };

  const input = "mt-1 w-full rounded-md border border-line bg-raised px-2 py-1.5 text-sm";
  return (
    <form
      onSubmit={(e) => void submit(e)}
      className="flex flex-col gap-3 rounded-md border border-line bg-panel p-3"
    >
      <label className="text-xs text-muted">
        Kind
        <select
          value={kind}
          onChange={(e) => {
            setKind(e.target.value);
          }}
          className={input}
        >
          <option value="printer-escpos">Receipt printer (Web Serial)</option>
          <option value="printer-http">Receipt printer (HTTP)</option>
          <option value="cash-drawer">Cash drawer</option>
          <option value="scanner-wedge">Barcode scanner</option>
          <option value="nfc-reader">NFC reader</option>
        </select>
      </label>
      <label className="text-xs text-muted">
        Name (optional)
        <input
          value={id}
          onChange={(e) => {
            setId(e.target.value);
          }}
          placeholder={
            kind === "cash-drawer"
              ? "drawer-2"
              : kind === "scanner-wedge"
                ? "scanner-2"
                : kind === "nfc-reader"
                  ? "nfc-2"
                  : "printer-2"
          }
          className={input}
        />
      </label>
      {kind === "cash-drawer" && (
        <div className="flex gap-2">
          <label className="flex-1 text-xs text-muted">
            Attached to
            <select
              value={attachedTo || printers[0]?.id || ""}
              onChange={(e) => {
                setAttachedTo(e.target.value);
              }}
              className={input}
            >
              {printers.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.id}
                </option>
              ))}
            </select>
          </label>
          <label className="w-20 text-xs text-muted">
            Pin
            <select
              value={pin}
              onChange={(e) => {
                setPin(e.target.value);
              }}
              className={input}
            >
              <option value="2">2</option>
              <option value="5">5</option>
            </select>
          </label>
        </div>
      )}
      <div className="flex gap-2">
        <button
          type="submit"
          disabled={kind === "cash-drawer" && printers.length === 0}
          className="rounded-md bg-ink px-3 py-1.5 text-sm font-medium text-counter disabled:opacity-40"
        >
          Add device
        </button>
        <button
          type="button"
          onClick={() => {
            setOpen(false);
          }}
          className="rounded-md px-3 py-1.5 text-sm text-muted hover:text-ink"
        >
          Cancel
        </button>
      </div>
    </form>
  );
}
