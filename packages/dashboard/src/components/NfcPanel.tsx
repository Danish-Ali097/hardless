import type { DeviceSummary } from "@hardless/types";
import { useState } from "react";
import { tap } from "../actions.js";
import { stateOf } from "../devices.js";
import {
  blankDraft,
  cardsOf,
  describeRecord,
  type DraftRecord,
  draftProblem,
  fromDrafts,
  lastWrittenOf,
  toDrafts,
} from "../nfc.js";

const TYPES: Record<DraftRecord["type"], string> = {
  text: "Text",
  url: "URL",
  mime: "MIME data",
  empty: "Empty",
};

const field = "rounded-md border border-line bg-raised px-2 py-1.5 text-sm";

/**
 * FR-27: the NFC reader. Tap a remembered card, or make a card (UID and NDEF records) and tap
 * it. Pages using the `NDEFReader` shim get a `reading` event; what they write shows here.
 */
export function NfcPanel({ reader }: { reader: DeviceSummary }) {
  const [uid, setUid] = useState("04a2b3c4d5e6f7");
  const [drafts, setDrafts] = useState<DraftRecord[]>([{ ...blankDraft(), data: "Member 1234" }]);
  const cards = cardsOf(reader);
  const written = lastWrittenOf(reader);
  const onReader = stateOf.string(reader, "card");
  const taps = stateOf.number(reader, "taps");
  const problem = draftProblem(uid, drafts);

  const update = (i: number, change: Partial<DraftRecord>) => {
    setDrafts(drafts.map((d, j) => (j === i ? { ...d, ...change } : d)));
  };

  return (
    <section aria-labelledby="nfc-title" className="flex flex-col gap-8">
      <header className="flex flex-wrap items-end justify-between gap-x-6 gap-y-3 border-b border-line pb-4">
        <div>
          <h1 id="nfc-title" className="text-2xl font-semibold tracking-tight">
            {reader.id}
          </h1>
          <p className="text-sm text-muted">
            NFC reader, a tapped card stays on it for{" "}
            {(stateOf.number(reader, "presenceMs") / 1000).toLocaleString("en")} s
          </p>
        </div>
        <p className="text-sm text-muted" role="status">
          {taps} {taps === 1 ? "tap" : "taps"},{" "}
          {onReader ? (
            <>
              card <span className="font-mono text-ink">{onReader}</span> on the reader
            </>
          ) : (
            "no card on the reader"
          )}
        </p>
      </header>

      <section aria-labelledby="cards-title" className="flex flex-col gap-3">
        <h2 id="cards-title" className="text-sm font-semibold">
          Cards
        </h2>
        {cards.length === 0 ? (
          <p className="text-sm text-muted">No cards yet. Make one below and tap it.</p>
        ) : (
          <ul className="flex flex-col divide-y divide-line rounded-md border border-line bg-raised">
            {cards.map((card) => (
              <li key={card.uid} className="flex flex-wrap items-start gap-3 px-3 py-2">
                <span className="min-w-0 flex-1">
                  <span className="block font-mono text-sm">
                    {card.uid}
                    {card.readOnly && (
                      <span className="ml-2 rounded bg-panel px-1.5 py-0.5 font-sans text-xs text-muted">
                        read-only
                      </span>
                    )}
                  </span>
                  {card.records.length === 0 ? (
                    <span className="block text-xs text-muted">No records</span>
                  ) : (
                    card.records.map((r, i) => (
                      <span key={i} className="block truncate text-xs text-muted">
                        {describeRecord(r)}
                      </span>
                    ))
                  )}
                </span>
                <button
                  type="button"
                  onClick={() => void tap(reader.id, card.uid)}
                  className="rounded-md bg-ink px-3 py-1 text-sm font-medium text-counter hover:opacity-90"
                >
                  Tap
                </button>
                <button
                  type="button"
                  aria-label={`Edit card ${card.uid}`}
                  onClick={() => {
                    setUid(card.uid);
                    setDrafts(toDrafts(card.records));
                  }}
                  className="rounded-md px-2 py-1 text-sm text-muted hover:text-ink"
                >
                  Edit
                </button>
              </li>
            ))}
          </ul>
        )}
        {written && (
          <p className="text-sm text-muted">
            Last written by the app to <span className="font-mono text-ink">{written.uid}</span>:{" "}
            {written.records.map(describeRecord).join("; ") || "no records"}
          </p>
        )}
      </section>

      <form
        aria-labelledby="card-title"
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          if (!problem) void tap(reader.id, uid.trim(), fromDrafts(drafts));
        }}
      >
        <h2 id="card-title" className="text-sm font-semibold">
          Make a card
        </h2>
        <label className="flex flex-col gap-1 text-xs text-muted">
          UID (hex)
          <input
            value={uid}
            onChange={(e) => {
              setUid(e.target.value);
            }}
            spellCheck={false}
            autoComplete="off"
            className={`${field} font-mono text-ink`}
          />
        </label>
        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1 text-xs text-muted">NDEF records</legend>
          {drafts.map((draft, i) => (
            <div key={i} className="flex flex-wrap items-center gap-2">
              {draft.kept ? (
                <span className="min-w-0 flex-1 truncate text-sm text-muted">
                  {describeRecord(draft.kept)} (kept as is)
                </span>
              ) : (
                <>
                  <select
                    aria-label={`Record ${String(i + 1)} type`}
                    value={draft.type}
                    onChange={(e) => {
                      update(i, { type: e.target.value as DraftRecord["type"] });
                    }}
                    className={field}
                  >
                    {Object.entries(TYPES).map(([value, label]) => (
                      <option key={value} value={value}>
                        {label}
                      </option>
                    ))}
                  </select>
                  {draft.type === "mime" && (
                    <input
                      aria-label={`Record ${String(i + 1)} MIME type`}
                      value={draft.mediaType}
                      onChange={(e) => {
                        update(i, { mediaType: e.target.value });
                      }}
                      className={`${field} w-40`}
                    />
                  )}
                  {draft.type !== "empty" && (
                    <input
                      aria-label={`Record ${String(i + 1)} data`}
                      value={draft.data}
                      onChange={(e) => {
                        update(i, { data: e.target.value });
                      }}
                      placeholder={draft.type === "url" ? "https://example.com/" : "Member 1234"}
                      spellCheck={false}
                      className={`${field} min-w-0 flex-1`}
                    />
                  )}
                  {draft.type === "text" && (
                    <input
                      aria-label={`Record ${String(i + 1)} language`}
                      value={draft.lang}
                      onChange={(e) => {
                        update(i, { lang: e.target.value });
                      }}
                      className={`${field} w-16`}
                    />
                  )}
                </>
              )}
              <button
                type="button"
                aria-label={`Remove record ${String(i + 1)}`}
                onClick={() => {
                  setDrafts(drafts.filter((_, j) => j !== i));
                }}
                className="rounded-md px-2 py-1 text-sm text-muted hover:text-danger"
              >
                Remove
              </button>
            </div>
          ))}
          <button
            type="button"
            onClick={() => {
              setDrafts([...drafts, blankDraft()]);
            }}
            className="self-start rounded-md border border-line bg-raised px-3 py-1 text-sm hover:bg-panel"
          >
            Add record
          </button>
        </fieldset>
        <div className="flex flex-wrap items-center gap-3">
          <button
            type="submit"
            disabled={problem !== null}
            className="rounded-md bg-ink px-4 py-1.5 text-sm font-medium text-counter hover:opacity-90 disabled:opacity-40"
          >
            Tap this card
          </button>
          {problem && (
            <span className="text-sm text-muted" role="status">
              {problem}
            </span>
          )}
        </div>
        <p className="text-xs text-muted">
          Pages using the Hardless <code>NDEFReader</code> shim get a <code>reading</code> event. A
          card the app writes to keeps what was written; tap it from the list to read it back.
        </p>
      </form>
    </section>
  );
}
