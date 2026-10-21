/**
 * The Hardless Chrome extension (FR-17, NFR-03). It injects `@hardless/shim-web` at
 * `document_start` in the page's MAIN world, only on origins the user enables in its popup,
 * so an unmodified app reaches the emulated devices of a running Hardless server.
 *
 * Enabling an origin asks for access to it, registers the injection for it, and stores the
 * bridge settings in that origin's `localStorage` under {@link CONFIG_KEY}: the injected
 * script runs before the page's own code and must read them synchronously, which extension
 * storage cannot do.
 *
 * @packageDocumentation
 */

/** Where and when the shim is injected (FR-17). */
export const INJECTION = { world: "MAIN", runAt: "document_start" } as const;

/** The page `localStorage` key holding the bridge settings for the injected shim. */
export const CONFIG_KEY = "hardless:extension";

/** Bridge settings for an enabled origin. */
export interface ExtensionConfig {
  /** The bridge URL, e.g. `ws://localhost:8080/bridge`. */
  url: string;
  /** Token if the server was started with `--token` (NFR-02). */
  token?: string;
}

export const DEFAULT_CONFIG: ExtensionConfig = { url: "ws://localhost:8080/bridge" };

/**
 * The match pattern for an origin, e.g. `http://localhost:5173/*`, or `null` for pages the
 * extension cannot run on (`chrome://`, `file://`, the Web Store).
 */
export function originPattern(url: string): string | null {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return null;
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return null;
  if (parsed.hostname === "chromewebstore.google.com") return null;
  // Match patterns take no port; the registered script's `matches` is per host, and the
  // injected script checks the full origin (see `inject.ts`).
  return `${parsed.protocol}//${parsed.hostname}/*`;
}

/** The ID of the registered content script for an origin. */
export function scriptId(origin: string): string {
  return `hardless:${origin}`;
}

/** Checks a bridge URL typed into the popup; returns an error message, or `null`. */
export function bridgeUrlProblem(url: string): string | null {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return "Enter a full URL, e.g. ws://localhost:8080/bridge.";
  }
  if (parsed.protocol !== "ws:" && parsed.protocol !== "wss:") {
    return "The bridge URL starts with ws:// or wss://.";
  }
  return null;
}

/**
 * Reads the bridge settings the popup stored for this page, or `null` if they are missing or
 * broken (then the injected script does nothing).
 */
export function parseConfig(raw: string | null): ExtensionConfig | null {
  if (raw === null) return null;
  try {
    const value = JSON.parse(raw) as Partial<ExtensionConfig> | null;
    if (!value || typeof value.url !== "string" || bridgeUrlProblem(value.url) !== null) {
      return null;
    }
    return typeof value.token === "string" && value.token !== ""
      ? { url: value.url, token: value.token }
      : { url: value.url };
  } catch {
    return null;
  }
}
