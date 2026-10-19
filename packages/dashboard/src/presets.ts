/** A saved barcode in the scanner panel (FR-26). */
export interface Preset {
  label: string;
  code: string;
}

const KEY = "hardless.scanner.presets";

/** Shown until the user saves their own. */
export const SAMPLE_PRESETS: Preset[] = [
  { label: "EAN-13 sample", code: "4006381333931" },
  { label: "EAN-13 (PRD example)", code: "8964000123456" },
  { label: "Order ticket", code: "ORDER-1042" },
];

/** Reads the saved presets. Storage can be blocked (private mode); then the samples show. */
export function loadPresets(): Preset[] {
  try {
    const raw = window.localStorage.getItem(KEY);
    if (raw === null) return SAMPLE_PRESETS;
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return SAMPLE_PRESETS;
    return parsed.filter(
      (p): p is Preset =>
        typeof p === "object" &&
        p !== null &&
        typeof (p as Preset).label === "string" &&
        typeof (p as Preset).code === "string",
    );
  } catch {
    return SAMPLE_PRESETS;
  }
}

/** Saves presets for next time; does nothing if storage is unavailable. */
export function savePresets(presets: Preset[]): void {
  try {
    window.localStorage.setItem(KEY, JSON.stringify(presets));
  } catch {
    // Kept for this session only.
  }
}

/** The codes of a sequence: one per line, blank lines and surrounding spaces ignored. */
export function sequenceCodes(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}
