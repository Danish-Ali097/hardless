# Hardless Chrome extension

A Chrome MV3 extension that injects `@hardless/shim-web` into pages, so an unmodified app (a
production build, or a site you do not control) can use the emulated devices of a running
Hardless server. It only runs on sites you turn it on for (NFR-03).

## Use it

1. Download `hardless-extension-<version>.zip` from the
   [latest release](https://github.com/hardlessdev/hardless/releases/latest) and unzip it. Or
   build it from the repository: `pnpm --filter @hardless/extension build` (writes `dist/`).
2. In Chrome, open `chrome://extensions`, turn on Developer mode, choose **Load unpacked** and
   pick the unzipped folder (or `packages/extension/dist`).
3. Start Hardless (`hardless start`), open your app, click the Hardless icon and choose
   **Use Hardless on this site**. Chrome asks for access to the site; the page reloads with
   the shim installed before any of its own code.
4. **Turn off** in the same popup restores the browser's own APIs.

The popup takes the bridge URL (default `ws://localhost:8080/bridge`; from an https:// page
use `wss://<host>:8443/bridge`) and the token if the server has one.

## How it works

- Turning a site on asks Chrome for access to it (`optional_host_permissions`; nothing is
  requested at install), then registers `inject.js` as a content script for it at
  `document_start` in the page's MAIN world (FR-17), so the shim exists before the app runs.
- The injected script must read its settings synchronously, which extension storage cannot
  do, so the popup also writes them to the site's `localStorage` (key `hardless:extension`)
  and reloads the site's open tabs. Clearing the site's storage turns the shim off for it
  until you save the settings again.
- Access is granted per host (Chrome match patterns ignore ports) but settings are per origin,
  so other ports of the same host stay untouched.
- The extension uses server mode only: the devices are the server's, shared with the
  dashboard, REST API and CLI.

## Tests

```sh
pnpm --filter @hardless/extension test       # unit tests
pnpm --filter @hardless/extension test:e2e   # loads the extension in Chromium (build it and
                                             # the server first: cargo build -p hardless-cli)
```
