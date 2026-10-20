import type { DeviceSummary, Direction, EventRecord } from "@hardless/types";
import { useEffect, useState } from "react";
import { api } from "../api.js";
import {
  describe,
  directionOf,
  formatAt,
  type LogFilter,
  matches,
  mergeRecords,
} from "../events.js";

/** How often the open log asks the server for new records. */
const POLL_MS = 1000;
/** Records loaded per device when the log opens. */
const FIRST_LOAD = 300;

const DIRECTIONS: { value: Direction | ""; label: string }[] = [
  { value: "", label: "Everything" },
  { value: "host-to-device", label: "App to device" },
  { value: "device-to-host", label: "Device to app" },
];

const ARROW: Record<Direction, string> = { "host-to-device": "→", "device-to-host": "←" };

const field = "mt-1 rounded-md border border-line bg-raised px-2 py-1.5 text-sm text-ink";

/**
 * FR-29: every device's event log, newest first, filtered by device and direction. Polls
 * `GET /devices/:id/events?since=` while open.
 */
export function EventLog({ devices }: { devices: DeviceSummary[] }) {
  const [records, setRecords] = useState<EventRecord[]>([]);
  const [filter, setFilter] = useState<LogFilter>({ device: null, direction: null });
  const ids = devices.map((d) => d.id).join("\n");

  useEffect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const last = new Map<string, number>();
    const poll = async () => {
      const batches = await Promise.all(
        ids
          .split("\n")
          .filter(Boolean)
          .map((id) => {
            const since = last.get(id);
            return api
              .events(id, since, since === undefined ? FIRST_LOAD : undefined)
              .catch(() => [] as EventRecord[]);
          }),
      );
      if (stopped) return;
      const incoming = batches.flat();
      for (const record of incoming) {
        last.set(record.deviceId, Math.max(last.get(record.deviceId) ?? 0, record.seq));
      }
      setRecords((log) => mergeRecords(log, incoming));
      timer = setTimeout(() => void poll(), POLL_MS);
    };
    void poll();
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [ids]);

  return <EventLogView devices={devices} records={records} filter={filter} onFilter={setFilter} />;
}

/** The log for a list of records; separate from the polling so it can be rendered in tests. */
export function EventLogView({
  devices,
  records,
  filter,
  onFilter,
}: {
  devices: DeviceSummary[];
  records: EventRecord[];
  filter: LogFilter;
  onFilter: (filter: LogFilter) => void;
}) {
  const shown = records.filter((r) => matches(r, filter)).reverse();
  return (
    <section aria-labelledby="log-title" className="flex flex-col gap-4">
      <header className="flex flex-wrap items-end justify-between gap-x-6 gap-y-3 border-b border-line pb-4">
        <div>
          <h1 id="log-title" className="text-2xl font-semibold tracking-tight">
            Event log
          </h1>
          <p className="text-sm text-muted">
            Bytes, keystrokes, card reads, state changes and faults, newest first
          </p>
        </div>
        <div className="flex flex-wrap gap-3">
          <label className="flex flex-col text-xs text-muted">
            Device
            <select
              value={filter.device ?? ""}
              onChange={(e) => {
                onFilter({ ...filter, device: e.target.value || null });
              }}
              className={field}
            >
              <option value="">All devices</option>
              {devices.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.id}
                </option>
              ))}
            </select>
          </label>
          <label className="flex flex-col text-xs text-muted">
            Direction
            <select
              value={filter.direction ?? ""}
              onChange={(e) => {
                onFilter({ ...filter, direction: (e.target.value || null) as Direction | null });
              }}
              className={field}
            >
              {DIRECTIONS.map((d) => (
                <option key={d.value} value={d.value}>
                  {d.label}
                </option>
              ))}
            </select>
          </label>
        </div>
      </header>
      {shown.length === 0 ? (
        <p className="py-10 text-center text-muted">
          {records.length === 0 ? "Nothing logged yet." : "No records match the filter."}
        </p>
      ) : (
        <ol className="flex flex-col divide-y divide-line rounded-md border border-line bg-raised">
          {shown.map((record) => {
            const { title, detail } = describe(record);
            const direction = directionOf(record);
            return (
              <li
                key={record.seq}
                className="grid grid-cols-[5.5rem_1.25rem_minmax(0,1fr)] gap-2 px-3 py-1.5 text-sm"
              >
                <span className="font-mono text-xs leading-5 text-muted">
                  {formatAt(record.at)}
                </span>
                <span
                  aria-label={
                    direction === "host-to-device"
                      ? "app to device"
                      : direction === "device-to-host"
                        ? "device to app"
                        : undefined
                  }
                  className="text-center leading-5 text-muted"
                >
                  {direction ? ARROW[direction] : ""}
                </span>
                <span className="min-w-0">
                  <span className="block">
                    <span className="mr-2 text-xs text-muted">{record.deviceId}</span>
                    {title}
                  </span>
                  {detail && (
                    <span className="block truncate font-mono text-xs text-muted" title={detail}>
                      {detail}
                    </span>
                  )}
                </span>
              </li>
            );
          })}
        </ol>
      )}
    </section>
  );
}
