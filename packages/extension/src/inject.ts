/**
 * Runs in the page's MAIN world at `document_start` on origins the user enabled (FR-17): it
 * installs the shim before any page code, with the bridge settings the popup stored in this
 * origin's `localStorage`. Without settings (another port of an enabled host, or settings
 * removed) it does nothing.
 */
import { install } from "@hardless/shim-web";
import { CONFIG_KEY, parseConfig } from "./index.js";

function storedConfig(): string | null {
  try {
    return window.localStorage.getItem(CONFIG_KEY);
  } catch {
    // Storage can be blocked (sandboxed frames); then the shim stays out.
    return null;
  }
}

const config = parseConfig(storedConfig());
if (config) {
  install({
    mode: "server",
    url: config.url,
    ...(config.token === undefined ? {} : { token: config.token }),
    // The user enabled this origin in the extension, which is the consent NFR-03 asks for.
    allowAnyOrigin: true,
  });
}
