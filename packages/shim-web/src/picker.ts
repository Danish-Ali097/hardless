import type { DeviceSummary } from "@hardless/types";

/**
 * The emulated port chooser (PRD section 8.1): a shadow-DOM dialog like the browser's own,
 * listing the emulated devices that match the request's filters. Resolves with the chosen
 * device ID, or `null` if the user cancels.
 */
export function showPicker(devices: DeviceSummary[], origin: string): Promise<string | null> {
  return new Promise((resolve) => {
    const host = document.createElement("hardless-port-picker");
    host.setAttribute("data-hardless-picker", "");
    const root = host.attachShadow({ mode: "open" });
    let selected: string | null = devices[0]?.id ?? null;

    root.innerHTML = `
      <style>
        :host { all: initial; }
        .backdrop { position: fixed; inset: 0; z-index: 2147483647; display: flex;
          align-items: flex-start; justify-content: center; padding-top: 12vh;
          background: rgb(0 0 0 / 0.25); font: 14px/1.4 system-ui, sans-serif; }
        .dialog { width: min(26rem, calc(100vw - 2rem)); background: #fff; color: #1f2328;
          border-radius: 8px; box-shadow: 0 12px 40px rgb(0 0 0 / 0.3); padding: 16px; }
        h2 { font-size: 15px; font-weight: 600; margin: 0 0 4px; }
        p { margin: 0 0 12px; color: #59636e; font-size: 13px; }
        ul { list-style: none; margin: 0 0 16px; padding: 0; border: 1px solid #d1d9e0;
          border-radius: 6px; max-height: 14rem; overflow: auto; }
        li button { display: block; width: 100%; text-align: left; border: 0;
          background: none; padding: 8px 10px; font: inherit; color: inherit; cursor: pointer; }
        li button[aria-selected="true"] { background: #ddf4ff; }
        li button:focus-visible, .actions button:focus-visible { outline: 2px solid #0969da; outline-offset: -2px; }
        .empty { padding: 12px 10px; color: #59636e; }
        .actions { display: flex; justify-content: flex-end; gap: 8px; }
        .actions button { font: inherit; padding: 6px 14px; border-radius: 6px;
          border: 1px solid #d1d9e0; background: #f6f8fa; cursor: pointer; }
        .actions button[data-action="connect"] { background: #0969da; border-color: #0969da; color: #fff; }
        .actions button:disabled { opacity: 0.5; cursor: default; }
        @media (prefers-color-scheme: dark) {
          .dialog { background: #212830; color: #f0f6fc; }
          p, .empty { color: #9198a1; }
          ul { border-color: #3d444d; }
          li button[aria-selected="true"] { background: #1f3a5f; }
          .actions button { background: #2a313c; border-color: #3d444d; color: inherit; }
        }
      </style>
      <div class="backdrop">
        <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="t">
          <h2 id="t"></h2>
          <p>Emulated by Hardless. Choose a port to connect to.</p>
          <ul role="listbox" aria-labelledby="t"></ul>
          <div class="actions">
            <button type="button" data-action="cancel">Cancel</button>
            <button type="button" data-action="connect">Connect</button>
          </div>
        </div>
      </div>`;

    const title = root.querySelector("h2");
    if (title) title.textContent = `${origin} wants to connect to a serial port`;
    const list = root.querySelector("ul");
    const connect = root.querySelector<HTMLButtonElement>('[data-action="connect"]');
    const cancel = root.querySelector<HTMLButtonElement>('[data-action="cancel"]');

    const finish = (choice: string | null) => {
      document.removeEventListener("keydown", onKey, true);
      host.remove();
      resolve(choice);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        finish(null);
      }
    };

    const render = () => {
      if (!list) return;
      list.replaceChildren();
      if (devices.length === 0) {
        const empty = document.createElement("li");
        empty.className = "empty";
        empty.textContent = "No compatible devices found.";
        list.append(empty);
      }
      for (const device of devices) {
        const item = document.createElement("li");
        const button = document.createElement("button");
        button.type = "button";
        button.setAttribute("role", "option");
        button.setAttribute("data-device-id", device.id);
        button.setAttribute("aria-selected", String(device.id === selected));
        button.textContent = `${device.id} (emulated receipt printer)`;
        button.addEventListener("click", () => {
          selected = device.id;
          render();
        });
        button.addEventListener("dblclick", () => {
          finish(device.id);
        });
        item.append(button);
        list.append(item);
      }
      if (connect) connect.disabled = selected === null;
    };

    connect?.addEventListener("click", () => {
      if (selected) finish(selected);
    });
    cancel?.addEventListener("click", () => {
      finish(null);
    });
    document.addEventListener("keydown", onKey, true);
    render();
    document.documentElement.append(host);
    root.querySelector<HTMLButtonElement>("li button, [data-action='cancel']")?.focus();
  });
}
