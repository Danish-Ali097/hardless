import type { DeviceSummary, NdefRecord } from "@hardless/types";

/** A card an NFC reader remembers, from its state (FR-27). */
export interface Card {
  uid: string;
  records: NdefRecord[];
  readOnly: boolean;
}

/** One row of the records editor: the kinds of record people type by hand. */
export interface DraftRecord {
  type: "text" | "url" | "mime" | "empty";
  data: string;
  /** Language of a text record. */
  lang: string;
  /** MIME type of a mime record. */
  mediaType: string;
  /** A record the editor cannot show (binary data), kept as it is. */
  kept?: NdefRecord;
}

function isRecord(value: unknown): value is NdefRecord {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { recordType?: unknown }).recordType === "string"
  );
}

function stateField(device: DeviceSummary, key: string): unknown {
  const state = device.state;
  return state && typeof state === "object" && !Array.isArray(state)
    ? (state as Record<string, unknown>)[key]
    : undefined;
}

/** The cards a reader remembers, most recently seen first. */
export function cardsOf(device: DeviceSummary): Card[] {
  const cards = stateField(device, "cards");
  if (!Array.isArray(cards)) return [];
  /** A card as it arrives: state is JSON, so check every field. */
  type Raw = { uid: string; records?: unknown; readOnly?: unknown };
  return cards
    .filter((c): c is Raw => typeof (c as { uid?: unknown } | null)?.uid === "string")
    .map((c) => ({
      uid: c.uid,
      records: Array.isArray(c.records) ? c.records.filter(isRecord) : [],
      readOnly: c.readOnly === true,
    }))
    .reverse();
}

/** The latest write from the app: which card and what. */
export function lastWrittenOf(
  device: DeviceSummary,
): { uid: string; records: NdefRecord[] } | null {
  const written = stateField(device, "lastWritten") as {
    uid?: unknown;
    records?: unknown;
  } | null;
  if (!written || typeof written.uid !== "string" || !Array.isArray(written.records)) return null;
  return { uid: written.uid, records: written.records.filter(isRecord) };
}

/** Size of a base64 payload in bytes. */
function base64Bytes(text: string): number {
  const padding = text.endsWith("==") ? 2 : text.endsWith("=") ? 1 : 0;
  return Math.max(0, Math.floor((text.length * 3) / 4) - padding);
}

/** A record in a few words, e.g. `Text "Member 1234" (en)`. */
export function describeRecord(record: NdefRecord): string {
  const size =
    record.dataBase64 === undefined ? "" : `${String(base64Bytes(record.dataBase64))} bytes`;
  switch (record.recordType) {
    case "empty":
      return "Empty record";
    case "text":
      return record.data === undefined
        ? `Text, ${size}`
        : `Text "${record.data}"${record.lang ? ` (${record.lang})` : ""}`;
    case "url":
    case "absolute-url":
      return `URL ${record.data ?? size}`;
    case "mime":
      return `${record.mediaType ?? "application/octet-stream"}: ${record.data ?? size}`;
    default:
      return `${record.recordType}: ${record.data ?? size}`;
  }
}

/** An empty row for the editor. */
export function blankDraft(): DraftRecord {
  return { type: "text", data: "", lang: "en", mediaType: "application/json" };
}

/** Turns a card's records into editor rows; binary records are kept unchanged. */
export function toDrafts(records: NdefRecord[]): DraftRecord[] {
  return records.map((record) => {
    const draft = blankDraft();
    if (record.recordType === "empty") return { ...draft, type: "empty" };
    const editable =
      record.data !== undefined &&
      record.id === undefined &&
      (record.recordType === "text" || record.recordType === "url" || record.recordType === "mime");
    if (!editable) return { ...draft, kept: record };
    return {
      ...draft,
      type: record.recordType as DraftRecord["type"],
      data: record.data ?? "",
      lang: record.lang ?? draft.lang,
      mediaType: record.mediaType ?? draft.mediaType,
    };
  });
}

/** Turns editor rows into records for a tap. */
export function fromDrafts(drafts: DraftRecord[]): NdefRecord[] {
  return drafts.map((draft) => {
    if (draft.kept) return draft.kept;
    switch (draft.type) {
      case "empty":
        return { recordType: "empty" };
      case "text":
        return draft.lang.trim()
          ? { recordType: "text", data: draft.data, lang: draft.lang.trim() }
          : { recordType: "text", data: draft.data };
      case "url":
        return { recordType: "url", data: draft.data };
      case "mime":
        return {
          recordType: "mime",
          mediaType: draft.mediaType.trim() || "application/octet-stream",
          data: draft.data,
        };
    }
  });
}

/** What is wrong with a draft card, or null if it can be tapped. */
export function draftProblem(uid: string, drafts: DraftRecord[]): string | null {
  const hex = uid.replace(/[:\-\s]/g, "");
  if (!/^([0-9a-fA-F]{2}){1,10}$/.test(hex)) {
    return "The UID is 1 to 10 bytes of hex, e.g. 04a2b3c4d5e6f7.";
  }
  if (drafts.some((d) => !d.kept && d.type === "url" && !d.data.trim())) {
    return "A URL record needs a URL.";
  }
  return null;
}
