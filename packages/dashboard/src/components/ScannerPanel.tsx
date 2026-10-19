import type { DeviceSummary } from "@hardless/types";
import { useEffect, useRef, useState } from "react";
import { scan } from "../actions.js";
import { stateOf } from "../devices.js";
import { loadPresets, type Preset, savePresets, sequenceCodes } from "../presets.js";

const TERMINATORS: Record<string, string> = {
  enter: "Enter after each code",
  tab: "Tab after each code",
  none: "no key after the code",
};

const sleep = (ms: number) =>
  new Promise<void>((resolve) => {
    setTimeout(resolve, ms);
  });

/**
 * FR-26: the keyboard-wedge scanner. Scan a code by hand, keep presets, or run a sequence
 * of codes with a delay between scans. Scans are typed into the focused field of every page
 * running the Hardless shim with the wedge on.
 */
export function ScannerPanel({ scanner }: { scanner: DeviceSummary }) {
  const [code, setCode] = useState("");
  const [presets, setPresets] = useState<Preset[]>(loadPresets);
  const [label, setLabel] = useState("");
  const scans = stateOf.number(scanner, "scans");
  const last = stateOf.string(scanner, "lastCode");
  const terminator = stateOf.string(scanner, "terminator");

  const updatePresets = (next: Preset[]) => {
    setPresets(next);
    savePresets(next);
  };

  return (
    <section aria-labelledby="scanner-title" className="flex flex-col gap-8">
      <header className="flex flex-wrap items-end justify-between gap-x-6 gap-y-3 border-b border-line pb-4">
        <div>
          <h1 id="scanner-title" className="text-2xl font-semibold tracking-tight">
            {scanner.id}
          </h1>
          <p className="text-sm text-muted">
            Keyboard-wedge barcode scanner, {TERMINATORS[terminator] ?? terminator},{" "}
            {stateOf.number(scanner, "delayMs")} ms between keys
          </p>
        </div>
        <p className="text-sm text-muted" role="status">
          {scans} {scans === 1 ? "scan" : "scans"}
          {last && (
            <>
              , last <span className="font-mono text-ink">{last}</span>
            </>
          )}
        </p>
      </header>

      <form
        className="flex flex-col gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (code.trim()) void scan(scanner.id, code.trim());
        }}
      >
        <label htmlFor="scan-code" className="text-sm font-semibold">
          Barcode
        </label>
        <div className="flex flex-wrap gap-2">
          <input
            id="scan-code"
            value={code}
            onChange={(e) => {
              setCode(e.target.value);
            }}
            autoComplete="off"
            spellCheck={false}
            placeholder="4006381333931"
            className="min-w-0 flex-1 rounded-md border border-line bg-raised px-3 py-1.5 font-mono text-sm"
          />
          <button
            type="submit"
            disabled={!code.trim()}
            className="rounded-md bg-ink px-4 py-1.5 text-sm font-medium text-counter hover:opacity-90 disabled:opacity-40"
          >
            Scan
          </button>
        </div>
        <p className="text-xs text-muted">
          The code is typed into the focused field of every page running the Hardless shim with the
          keyboard wedge on. Synthetic key events have <code>isTrusted</code> set to false.
        </p>
      </form>

      <Presets
        presets={presets}
        onScan={(p) => void scan(scanner.id, p.code)}
        onRemove={(i) => {
          updatePresets(presets.filter((_, j) => j !== i));
        }}
        save={
          <form
            className="flex flex-wrap gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              if (!code.trim()) return;
              updatePresets([
                ...presets,
                { label: label.trim() || code.trim(), code: code.trim() },
              ]);
              setLabel("");
            }}
          >
            <input
              aria-label="Preset name"
              value={label}
              onChange={(e) => {
                setLabel(e.target.value);
              }}
              placeholder="Name (optional)"
              className="min-w-0 flex-1 rounded-md border border-line bg-raised px-3 py-1.5 text-sm"
            />
            <button
              type="submit"
              disabled={!code.trim()}
              className="rounded-md border border-line bg-raised px-3 py-1.5 text-sm font-medium hover:bg-panel disabled:opacity-40"
            >
              Save the barcode above
            </button>
          </form>
        }
      />

      <Sequence scannerId={scanner.id} />
    </section>
  );
}

function Presets({
  presets,
  onScan,
  onRemove,
  save,
}: {
  presets: Preset[];
  onScan: (preset: Preset) => void;
  onRemove: (index: number) => void;
  save: React.ReactNode;
}) {
  return (
    <section aria-labelledby="presets-title" className="flex flex-col gap-3">
      <h2 id="presets-title" className="text-sm font-semibold">
        Presets
      </h2>
      {presets.length === 0 ? (
        <p className="text-sm text-muted">No presets. Type a barcode above and save it.</p>
      ) : (
        <ul className="flex flex-col divide-y divide-line rounded-md border border-line bg-raised">
          {presets.map((preset, i) => (
            <li key={`${preset.code}-${String(i)}`} className="flex items-center gap-3 px-3 py-2">
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm">{preset.label}</span>
                <span className="block truncate font-mono text-xs text-muted">{preset.code}</span>
              </span>
              <button
                type="button"
                onClick={() => {
                  onScan(preset);
                }}
                className="rounded-md bg-ink px-3 py-1 text-sm font-medium text-counter hover:opacity-90"
              >
                Scan
              </button>
              <button
                type="button"
                aria-label={`Remove preset ${preset.label}`}
                onClick={() => {
                  onRemove(i);
                }}
                className="rounded-md px-2 py-1 text-sm text-muted hover:text-danger"
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
      {save}
    </section>
  );
}

/** Scans a list of codes in order, waiting `delay` ms after each scan. */
function Sequence({ scannerId }: { scannerId: string }) {
  const [text, setText] = useState("");
  const [delay, setDelay] = useState(500);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const stop = useRef(false);
  // Read through a function: the flag changes while `run` is awaiting.
  const stopped = () => stop.current;
  const codes = sequenceCodes(text);

  useEffect(
    () => () => {
      stop.current = true;
    },
    [],
  );

  const run = async () => {
    stop.current = false;
    setBusy(true);
    setProgress({ done: 0, total: codes.length });
    for (const [i, code] of codes.entries()) {
      if (stopped() || !(await scan(scannerId, code))) break;
      setProgress({ done: i + 1, total: codes.length });
      if (i < codes.length - 1) await sleep(delay);
    }
    setBusy(false);
  };

  return (
    <section aria-labelledby="sequence-title" className="flex flex-col gap-3">
      <h2 id="sequence-title" className="text-sm font-semibold">
        Sequence
      </h2>
      <label htmlFor="sequence-codes" className="sr-only">
        Codes, one per line
      </label>
      <textarea
        id="sequence-codes"
        value={text}
        onChange={(e) => {
          setText(e.target.value);
        }}
        rows={5}
        spellCheck={false}
        placeholder={"One code per line\n4006381333931\n8964000123456"}
        className="rounded-md border border-line bg-raised px-3 py-2 font-mono text-sm"
      />
      <div className="flex flex-wrap items-center gap-3 text-sm">
        <label className="flex items-center gap-2 text-muted">
          Delay between scans
          <input
            type="number"
            min={0}
            step={50}
            value={delay}
            onChange={(e) => {
              setDelay(Math.max(0, Number(e.target.value) || 0));
            }}
            className="w-24 rounded-md border border-line bg-raised px-2 py-1 text-ink"
          />
          ms
        </label>
        {busy ? (
          <button
            type="button"
            onClick={() => {
              stop.current = true;
            }}
            className="rounded-md border border-line bg-raised px-3 py-1.5 font-medium hover:bg-panel"
          >
            Stop
          </button>
        ) : (
          <button
            type="button"
            disabled={codes.length === 0}
            onClick={() => void run()}
            className="rounded-md bg-ink px-4 py-1.5 font-medium text-counter hover:opacity-90 disabled:opacity-40"
          >
            Run {codes.length > 0 ? `${String(codes.length)} scans` : "sequence"}
          </button>
        )}
        {progress && (
          <span className="text-muted" role="status">
            Scanned {progress.done} of {progress.total}
          </span>
        )}
      </div>
    </section>
  );
}
