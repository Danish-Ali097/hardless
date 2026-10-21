/**
 * Loyalty cards over Web NFC, as a real app would write it: no Hardless code. With the shim
 * (or on Android Chrome), `NDEFReader` exists; elsewhere the app says NFC is unavailable.
 */

/** The parts of Web NFC this app uses (TypeScript's DOM types do not include Web NFC). */
interface NdefRecordLike {
  recordType: string;
  data: DataView | null;
  encoding: string | null;
}

interface ReadingEvent extends Event {
  serialNumber: string;
  message: { records: readonly NdefRecordLike[] };
}

interface NdefReaderLike extends EventTarget {
  scan(options?: { signal?: AbortSignal }): Promise<void>;
  write(message: { records: { recordType: string; data: string }[] }): Promise<void>;
}

type NdefReaderConstructor = new () => NdefReaderLike;

/** A card read: its UID and the text of its first text record, e.g. `Visits: 2`. */
export interface Card {
  serialNumber: string;
  text: string | null;
}

/** Whether this browser has Web NFC. */
export function nfcAvailable(): boolean {
  return "NDEFReader" in globalThis;
}

function reader(): NdefReaderLike {
  const NDEFReader = (globalThis as { NDEFReader?: NdefReaderConstructor }).NDEFReader;
  if (!NDEFReader) throw new Error("NFC is not available in this browser.");
  return new NDEFReader();
}

/** The text of the first text record, decoded with its encoding. */
export function cardText(records: readonly NdefRecordLike[]): string | null {
  const text = records.find((r) => r.recordType === "text" && r.data !== null);
  if (!text?.data) return null;
  return new TextDecoder(text.encoding ?? "utf-8").decode(text.data);
}

/** The visit count in a card's text (`Visits: 2`), 0 if there is none. */
export function visits(text: string | null): number {
  const match = /Visits:\s*(\d+)/.exec(text ?? "");
  return match ? Number(match[1]) : 0;
}

/** Starts listening for cards; `onCard` runs for each tap. Stop with the returned function. */
export async function listen(
  onCard: (card: Card) => void,
  onError: () => void,
): Promise<() => void> {
  const controller = new AbortController();
  const nfc = reader();
  nfc.addEventListener("reading", (event) => {
    const e = event as ReadingEvent;
    onCard({ serialNumber: e.serialNumber, text: cardText(e.message.records) });
  });
  nfc.addEventListener("readingerror", onError);
  await nfc.scan({ signal: controller.signal });
  return () => {
    controller.abort();
  };
}

/** Writes the visit count to the next card tapped (or the one on the reader). */
export async function stamp(count: number): Promise<void> {
  await reader().write({ records: [{ recordType: "text", data: `Visits: ${String(count)}` }] });
}
