import type { DeviceSummary } from "@hardless/types";
import { useState } from "react";
import { setFault } from "../actions.js";
import { faultInfo } from "../faults.js";

/** FR-28: a switch for every fault the device supports, with its option if it has one. */
export function FaultPanel({ device }: { device: DeviceSummary }) {
  if (device.supportedFaults.length === 0) return null;
  return (
    <section aria-labelledby={`faults-${device.id}`} className="flex flex-col gap-3">
      <h2 id={`faults-${device.id}`} className="text-sm font-semibold">
        Faults for {device.id}
      </h2>
      <ul className="flex flex-col gap-2">
        {device.supportedFaults.map((fault) => (
          <FaultRow key={fault} device={device} fault={fault} />
        ))}
      </ul>
    </section>
  );
}

function FaultRow({ device, fault }: { device: DeviceSummary; fault: string }) {
  const info = faultInfo(fault);
  const active = device.faults.find((f) => f.id === fault);
  const current = active?.options[info.option?.key ?? ""];
  const [value, setValue] = useState<string>(
    typeof current === "number" ? String(current) : String(info.option?.defaultValue ?? ""),
  );
  const toggle = () => {
    const options = info.option && value !== "" ? { [info.option.key]: Number(value) } : undefined;
    void setFault(device.id, fault, !active, options);
  };
  const descriptionId = `fault-${device.id}-${fault}`;
  return (
    <li
      className={`rounded-md border px-3 py-2 ${
        active ? "border-brass bg-brass-soft" : "border-line bg-raised"
      }`}
    >
      <div className="flex items-center justify-between gap-3">
        <span className="font-medium">{info.name}</span>
        <button
          type="button"
          role="switch"
          aria-checked={Boolean(active)}
          aria-label={info.name}
          aria-describedby={descriptionId}
          onClick={toggle}
          className={`relative h-5 w-9 shrink-0 rounded-full transition-colors ${
            active ? "bg-brass" : "bg-line"
          }`}
        >
          <span
            aria-hidden="true"
            className={`absolute left-0 top-0.5 size-4 rounded-full bg-raised shadow transition-transform ${
              active ? "translate-x-4.5" : "translate-x-0.5"
            }`}
          />
        </button>
      </div>
      <p
        id={descriptionId}
        className={`mt-0.5 text-xs ${active ? "text-brass-ink" : "text-muted"}`}
      >
        {info.effect}
      </p>
      {info.option && (
        <label className="mt-2 flex items-center gap-2 text-xs text-muted">
          <input
            type="number"
            min={info.option.min}
            step={info.option.step ?? 1}
            value={value}
            disabled={Boolean(active)}
            onChange={(e) => {
              setValue(e.target.value);
            }}
            className="w-20 rounded border border-line bg-raised px-1.5 py-0.5 text-ink disabled:opacity-60"
          />
          {info.option.label}
        </label>
      )}
    </li>
  );
}
