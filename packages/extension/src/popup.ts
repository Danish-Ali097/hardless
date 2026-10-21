/**
 * The extension popup: turns Hardless on or off for the site in the current tab (NFR-03).
 * `popup.html?origin=<origin>` targets another origin (used by the tests).
 */
import {
  bridgeUrlProblem,
  CONFIG_KEY,
  DEFAULT_CONFIG,
  type ExtensionConfig,
  INJECTION,
  originPattern,
  parseConfig,
  scriptId,
} from "./index.js";

function element(id: string): HTMLElement {
  const found = document.getElementById(id);
  if (!found) throw new Error(`popup.html is missing #${id}`);
  return found;
}

const ui = {
  origin: element("origin"),
  status: element("status"),
  form: element("form") as HTMLFormElement,
  url: element("url") as HTMLInputElement,
  token: element("token") as HTMLInputElement,
  enable: element("enable") as HTMLButtonElement,
  disable: element("disable") as HTMLButtonElement,
  message: element("message"),
};

/** The page the popup acts on: `?origin=` or the active tab. */
async function targetUrl(): Promise<string | null> {
  const fromQuery = new URLSearchParams(window.location.search).get("origin");
  if (fromQuery) return fromQuery;
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  return tab?.url ?? null;
}

/** Writes (or with `null`, removes) the settings in every open tab of `origin`, then reloads them. */
async function applyToTabs(origin: string, pattern: string, config: ExtensionConfig | null) {
  const tabs = await chrome.tabs.query({ url: pattern });
  const value = config === null ? null : JSON.stringify(config);
  for (const tab of tabs) {
    if (tab.id === undefined || !tab.url) continue;
    if (new URL(tab.url).origin !== origin) continue;
    await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      world: "MAIN",
      func: (key: string, json: string | null) => {
        if (json === null) localStorage.removeItem(key);
        else localStorage.setItem(key, json);
      },
      args: [CONFIG_KEY, value],
    });
    await chrome.tabs.reload(tab.id);
  }
}

async function isEnabled(origin: string): Promise<boolean> {
  const scripts = await chrome.scripting.getRegisteredContentScripts({ ids: [scriptId(origin)] });
  return scripts.length > 0;
}

function show(on: boolean, message = ""): void {
  ui.status.textContent = on
    ? "On: the page uses Hardless devices."
    : "Off: the page uses the browser's own APIs.";
  ui.status.dataset.on = String(on);
  ui.enable.textContent = on ? "Save and reload" : "Use Hardless on this site";
  ui.disable.hidden = !on;
  ui.message.textContent = message;
}

async function main(): Promise<void> {
  const url = await targetUrl();
  const pattern = url ? originPattern(url) : null;
  if (!url || !pattern) {
    ui.origin.textContent = url ?? "none";
    ui.status.textContent = "Hardless cannot run on this page.";
    ui.form.hidden = true;
    return;
  }
  const origin = new URL(url).origin;
  ui.origin.textContent = origin;
  const stored = await chrome.storage.local.get(origin);
  const config = parseConfig(typeof stored[origin] === "string" ? stored[origin] : null);
  ui.url.value = (config ?? DEFAULT_CONFIG).url;
  ui.token.value = config?.token ?? "";
  show(await isEnabled(origin));

  ui.form.addEventListener("submit", (event) => {
    event.preventDefault();
    void (async () => {
      const problem = bridgeUrlProblem(ui.url.value.trim());
      if (problem) {
        ui.message.textContent = problem;
        return;
      }
      const next: ExtensionConfig = ui.token.value
        ? { url: ui.url.value.trim(), token: ui.token.value }
        : { url: ui.url.value.trim() };
      // Must run in the click's user gesture: Chrome asks the user for access to the site.
      if (!(await chrome.permissions.request({ origins: [pattern] }))) {
        ui.message.textContent = "Hardless needs access to this site to run on it.";
        return;
      }
      if (!(await isEnabled(origin))) {
        await chrome.scripting.registerContentScripts([
          {
            id: scriptId(origin),
            matches: [pattern],
            js: ["inject.js"],
            runAt: INJECTION.runAt,
            world: INJECTION.world,
            allFrames: false,
            persistAcrossSessions: true,
          },
        ]);
      }
      await chrome.storage.local.set({ [origin]: JSON.stringify(next) });
      await applyToTabs(origin, pattern, next);
      show(true, "Turned on. The page has been reloaded.");
    })().catch((error: unknown) => {
      ui.message.textContent = error instanceof Error ? error.message : String(error);
    });
  });

  ui.disable.addEventListener("click", () => {
    void (async () => {
      await chrome.scripting.unregisterContentScripts({ ids: [scriptId(origin)] });
      await chrome.storage.local.remove(origin);
      await applyToTabs(origin, pattern, null);
      show(false, "Turned off. The page has been reloaded.");
      // Give up access to the host unless another enabled origin on it still needs it. Chrome
      // refuses to remove access the extension was installed with; that is harmless here.
      const others = await chrome.scripting.getRegisteredContentScripts();
      if (!others.some((s) => s.matches?.includes(pattern))) {
        await chrome.permissions.remove({ origins: [pattern] }).catch(() => false);
      }
    })().catch((error: unknown) => {
      ui.message.textContent = error instanceof Error ? error.message : String(error);
    });
  });
}

void main();
