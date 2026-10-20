# Decisions and deviations

Choices made while implementing the PRD that the PRD does not spell out, and the places where
the implementation differs from it. Newest milestone first. Items marked **open** need a
decision from the maintainer.

## M2 — Cross-browser paths

### Maintainer decisions taken before starting

- **Receipt font: Go Mono** (BSD-3-Clause, Bigelow & Holmes for the Go project). PNG output
  needs a bundled font because containers have none, and most free monospace fonts are SIL
  OFL-1.1, which is not on the NFR-14 list. Go Mono and Go Mono Bold (about 350 KB) live in
  `crates/hardless-render/fonts/` with their licence. Rejected: a hand-drawn pixel font (more
  work, pixelated) and adding OFL-1.1 to the policy.
- **New libraries approved:** `resvg` (Apache-2.0 OR MIT; PRD section 6 already names it) for
  PNG, `roxmltree` (MIT OR Apache-2.0) for the HTTP dialects' XML, and `rustls` with the
  `ring` crypto provider (`ring` is Apache-2.0 AND ISC) for HTTPS. rustls's default provider,
  `aws-lc-rs`, includes OpenSSL-licensed code, which is not on the NFR-14 list.
- **Pushing:** after each M2 piece passes the local checks, commit, push to `main` and watch
  CI, then carry on.
- **Docker image is built and tested in CI but not pushed** until the first release (v0.1.0);
  npm packages are likewise published only at release. The names are already reserved.

### Renderer (FR-11)

- **"Identical layout" means the same layout pass, not identical pixels.** HTML, SVG, PNG and
  `to_text` all come from one `layout()` step, so line breaks, columns, alignment, character
  sizes, codes and cuts match. Fonts differ between a browser and the bundled Go Mono, so
  pixels cannot.
- **SVG is in printer dots**: 576 printable dots across 80 mm and 384 across 58 mm, 18-dot
  side margins, 30-dot line pitch (the ESC/POS default line spacing). Each text run uses
  `textLength` with `lengthAdjust="spacingAndGlyphs"`, so runs fill exactly their columns in
  any monospace font. The SVG names Go Mono first but does not embed it (it would add 350 KB
  to every receipt).
- **PNG is behind the `png` feature** of `hardless-render`, so the WebAssembly engine stays
  small; the server enables it. One pixel per dot, white paper, Go Mono only (no system
  fonts, so PNGs are the same on every OS and in Docker).
- **PNG height limit: 20,000 dots** (about 2.5 m of paper). Longer receipts get a 400
  `receipt_too_long` from `/render?format=png` instead of a multi-hundred-megabyte bitmap.
  SVG and HTML have no limit.
- **`/render?format=png` rasterises on a blocking thread** (`spawn_blocking`) so a large
  receipt does not stall the async workers.
- **Notices:** `hardless_render::FONT_LICENSE` exports the font licence. The v0.1.0 notices
  step (see M0, Licences) must include it, alongside the Rust dependencies' notices.
- **Dashboard (FR-24):** "Download PNG" sits next to "Download HTML" on every receipt.

### HTTP dialects (FR-31, FR-34)

- **Sources:** the public "ePOS-Print XML User's Manual" (Epson), the public "Star webPRNT
  User's Manual" and the public "ASB Function Specifications Manual" (Star). Only element
  names, attribute values, codes and bit positions are used; no SDK code or manual text.
- **XML is translated to ESC/POS bytes, not straight to the AST.** The printer device parses
  them, so "the same command AST as ESC/POS" (FR-31) holds by construction, faults such as
  `garbled` apply to HTTP prints too, and the receipt's raw-bytes view shows equivalent
  ESC/POS. Tests check that a document gives exactly the AST of the hand-written ESC/POS.
- **Each request starts with `ESC @`.** An HTTP request is a self-contained document, so it
  starts from the power-on settings; formatting does not carry over between requests (it
  still does within a Web Serial stream, see "Between M1 and M2").
- **Text encoding:** XML text is Unicode; it is printed in CP437, switching to CP858
  (`ESC t 19`) for characters only CP858 has, such as `€`. Characters neither table has print
  as `?`. So `lang`, `codepage` and `international` need no translation.
- **What the emulator does not reproduce** (buzzer, page mode, logos, reverse/rotated text,
  ruled lines, hold print) gives a warning for the event log. Logos and symbol types other
  than QR Code go out as their real commands and show as unknown-command badges, as on a
  printer without them.
- **Strictness:** elements and attribute values the manuals do not define are a schema error
  (`SchemaError` in ePOS). Known quirks are accepted: WebPRNT's `poistion` typo and both
  `bitimage` and `bitImage`.
- **Status comes from the printer itself:** after printing, the endpoint reads the printer's
  `DLE EOT 1` to `4` reply values (the printer state's new `statusBytes` field) and builds the
  response from them, so faults and the `drawerOpenLevel` setting behave the same over HTTP as
  over Web Serial. Reading them from the state rather than sending `DLE EOT` over the line keeps
  `garbled` from eating the status requests and keeps the event log free of extra traffic.
  Paper end and an open cover mean the print did not happen (`success="false"`).
- **WebPRNT status:** 11 bytes as hex, starting `23 86` as in the manual's example; drawer
  switch is status 1 bit 2, offline bit 3, cover bit 5; paper near end status 4 bit 2, paper
  end bit 3. `code` is `0`, `1100` (offline or error, also used for a bad document) or `2001`
  (busy, used for the `timeout` fault).
- **Open, deviation from the PRD:** section 9.1 says the WebPRNT response has a `TraceID`.
  Star's public response reference documents only `success`, `code` and `status`, so no
  `TraceID` is sent. Following the vendor reference, as section 8 does for browser specs.
- **Safety:** documents over 8 MB and documents with a DTD are refused. Both parsers have a
  `cargo-fuzz` target (`crates/hardless-http-dialects/fuzz`), run nightly next to the ESC/POS
  one.

### Example app and M2 acceptance

- **The example app gained the M2 routes** without knowing about Hardless: a network printer
  that speaks ePOS-Print XML over `fetch` (`src/epos.ts`, the same receipt as over Web
  Serial), chosen next to Web Serial with an editable address, and a barcode field that a
  wedge scan fills and submits with Enter. The editable address is how the mixed-content case
  is reproduced: from the app on https://, type the `http://` address and pay.
- **HTTPS for the app itself:** the server now also writes its current server certificate
  and key (`hardless-server.pem`, `hardless-server-key.pem`) next to the CA at each start, so
  another dev server on the same machine can serve HTTPS that devices already trust. The
  example's Vite config uses them with `HTTPS=1`. The key is a development key for local names
  only and is owner-readable only on Unix. The shim's bridge address follows the page:
  `wss://<host>:8443` from https://, `ws://<host>:8080` otherwise; the example passes
  `allowAnyOrigin` because on a tablet it is opened at a LAN address.
- **Automated acceptance** (`examples/react-pwa/e2e/m2.spec.ts`): printing over the
  ePOS-Print-compatible endpoint, paper-out over HTTP, a server scan typed into the app, and
  the 20-barcode sequence arriving in order with its delay. The HTTP and wedge tests also run in
  Playwright's WebKit, the engine of Safari, which is as close as CI gets to the PRD's desktop
  Safari check. Tests run one at a time, because the wedge types into every connected page.
- **Manual, for the maintainer** (PRD M2 acceptance, to record in the M2 pull request or
  report): the example app printing from desktop Safari and from iOS Safari over the HTTPS
  endpoint after trusting the certificate, and the mixed-content case on iOS. Steps are in
  `examples/react-pwa/README.md`.

### Docker image (NFR-09, NFR-12)

- **Built from prebuilt static binaries.** CI compiles `hardless` for
  `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` on native x86 and arm64 GitHub
  runners, then `docker buildx` packs each into `gcr.io/distroless/static-debian12:nonroot`.
  No emulation is needed, so the arm64 build takes minutes, not an hour under QEMU.
- **Built and tested in CI, not pushed** (maintainer decision): a separate `Docker`
  workflow builds both platforms on every push and pull request, checks the sizes (binary under
  25 MB, image under 40 MB), runs the amd64 image and checks the M2 acceptance: the dashboard,
  `/api/v1/info`, HTTPS with the container's CA, and an ePOS print. Pushing to Docker Hub as
  `hardless/hardless` waits for the v0.1.0 release. It is a separate workflow so the main CI
  does not get slower.
- **Measured locally (amd64):** a 9.3 MB binary, a 20 MB image.
- **Container defaults:** `HARDLESS_HOST=0.0.0.0` so `-p` works (the start-up warnings about
  0.0.0.0 and no token are expected); the CA is made on first run in `/data/certs`, owned by
  the image's non-root user and declared a volume, so it is never in the image (NFR-05) and
  survives restarts with `-v hardless-data:/data`. Inside a container the detected LAN address
  is the container's own, so for phones pass `-e HARDLESS_CERT_HOSTS=<host LAN address>`.
- **No `HEALTHCHECK`:** distroless has no shell or curl; use `GET /api/v1/health`.
- **Start-up output** shows `localhost` instead of `0.0.0.0` when listening on every interface.
- **Open for v0.1.0:** the image must carry the licence notices of the Rust dependencies and
  the Go Mono font (see M0, Licences, and Renderer above).

### CLI (FR-30)

- **The commands are REST clients.** `scan`, `tap`, `fault` and `receipts` call a running
  server's `/api/v1`, chosen with `--server` / `HARDLESS_SERVER` (default
  `http://127.0.0.1:8080`) and `--token` / `HARDLESS_TOKEN`. They take the server's `http://`
  address only; the HTTPS port is for browsers and phones.
- **No new HTTP client crate:** hyper and hyper-util (already used by the server) provide the
  client. Their client features add two small crates, `want` and `try-lock` (both MIT).
- **Beyond the PRD's list,** which these extend without changing: `scan --terminator` and
  `--delay-ms` (the REST body's fields), `tap --records`, `fault --option KEY=VALUE`, and
  `receipts --limit`, `--format text|json|html|svg|png`, `--width` and `--out`. PNG needs
  `--out` so binary data never goes to a terminal.
- **Output:** one plain line on success; on failure the server's own error message and exit
  status 1 (2 for usage errors, from clap). An unreachable server says how to fix it.

### Scenario runner (FR-04)

- **JSON shape:** `{ "name"?, "steps": [ { "type": "scan" | "tap" | "wait" | "setFault" |
"clearFault" | "expect", ... } ] }`, with the PRD's step names. Device steps name their
  device with `device`. Unknown step types and fields are rejected.
- **`expect`** was not specified beyond its name. It checks `state` (a subset match, so
  `{ "state": "open" }` checks a drawer without listing every field), `receiptContains` (the
  latest job's text) and `jobs` (how many jobs the device holds). `withinMs` lets it wait for
  something the app does in response, polling every 20 ms; without it, the check is immediate.
- **A run stops at the first failing step**, like a test; later steps are reported as
  `skipped`. A device that refuses a step (a scan with no code, a tap on a non-NFC device)
  fails it.
- **Sans-IO in the core** (`ScenarioRun::advance`), so the WebAssembly engine can run
  scenarios too (the M3 Playwright fixture). The server's `POST /api/v1/scenarios/run` runs
  in real time and answers with the report when the run ends; scans reach the shim during the
  run, as with `POST /scan`.
- **Limits:** at most 10,000 steps and 10 minutes of `wait` plus `withinMs` in total, checked
  before the run starts (`400 scenario_too_long`), so a request cannot hold the server forever.
- **`POST /devices/:id/tap`** (PRD section 11.1) is added now for the CLI's `tap`; NFC devices
  arrive in M3, until then a tap is refused in the device's event log.

### HTTP printer endpoints and HTTPS (FR-19, FR-22, FR-32 to FR-35)

- **Default `local_printer`** (maintainer decision): a fourth default device, an HTTP
  printer. `local_printer` is the ePOS-Print default device ID, so Epson-style apps work with
  no setup. With no device of that ID, `devid=local_printer` reaches the first HTTP printer.
  `/StarWebPRNT/SendMessage` without `/p/<id>` also reaches the first HTTP printer by ID.
- **Printers are on one route:** `printer-escpos` gets `"transport": "http"`. An HTTP printer
  is not offered by the Web Serial picker, and a Web Serial printer does not answer HTTP. HTTP
  printers support `paper-out`, `paper-near-end`, `cover-open`, `offline`, `garbled`,
  `timeout` and `cors-blocked` (section 10); the serial-only faults are rejected.
- **A request is one job:** after the translated bytes, the endpoint sends the printer a
  `close-job` control command (`ControlCommand::Custom`), so the receipt appears at once
  instead of after the 300 ms idle time.
- **CORS (FR-32):** any origin is echoed (a printer on the network does not know the app's
  origin), with methods `POST, OPTIONS` and headers `Content-Type`, `SOAPAction` and
  `If-Modified-Since`. The last is not in the PRD but the ePOS samples send it and it is not a
  safelisted header, so without it browsers would block those apps. Chrome's Private Network
  Access preflight is answered too. The printer endpoints need no token, like real printers.
- **`offline` over HTTP** closes the connection without an answer. "Connection refused" is not
  possible per device on a shared port; the browser sees the same network error.
- **`timeout`** prints, then holds the answer until 500 ms after the client's timeout: the
  ePOS `timeout` (default 10 s when the request gives none), or 90 s for WebPRNT, whose SDK
  waits 90 s by default. The answer then says `EX_TIMEOUT` (ePOS) or busy (`2001`, WebPRNT).
- **Rejected documents** are answered as schema errors and logged in the printer's event log,
  through a new `Registry::log` (core) for transports. Translation warnings (buzzer, page mode,
  logos) go to the event log the same way.
- **HTTPS: a local certificate authority** (maintainer decision), as mkcert does. iOS only
  offers full trust for a CA, and trusting a CA once covers every later server certificate.
  The CA (10 years) lives in the per-user data folder, never in the repo or the Docker image
  (NFR-05); the server certificate (397 days, Apple's limit) is made fresh at each start.
  The key file is readable by the owner only on Unix.
- **Certificate names:** `localhost`, `127.0.0.1`, `::1`, the `--host` address, and with
  `--host 0.0.0.0` the LAN address of the default route, found by asking the OS which source
  address a UDP socket would use. That sends no packets (NFR-04). Extra names come from
  `certHosts` / `HARDLESS_CERT_HOSTS`.
- **TLS stack:** rustls with the `ring` provider (approved before M2), `rcgen` for the
  certificates (PRD section 6) and `tokio-rustls` to serve them (MIT OR Apache-2.0). HTTP/1.1
  only, so WebSocket upgrades (the bridge over `wss://`) work as on port 8080.
- **HTTPS failing is not fatal:** HTTP keeps running, startup prints a warning, and
  `GET /api/v1/info` carries the error for the dashboard. Settings: `https`, `httpsPort`,
  `certDir`, `certHosts` and matching `HARDLESS_*` variables. Tests run with HTTPS off and
  their own certificate folders, so they never touch a user's CA.
- **New `GET /api/v1/info`** (maintainer decision): version, HTTP port, and HTTPS state, port,
  certificate link and hosts, for the dashboard's HTTPS help. `/health` stays minimal for
  Docker. The CA certificate is served at `/hardless-ca.crt`, outside the API, so a phone can
  fetch it without a token; it is public, the key never leaves the machine.
- **Dashboard (FR-33):** an HTTP printer shows its ePOS and WebPRNT addresses (HTTPS when
  running), explains mixed content and the untrusted-certificate case, offers the certificate
  and lists the trust steps for iPhone and iPad, Android, macOS and Windows. It warns when the
  address the dashboard is open on is not in the certificate. "Add device" can add an HTTP
  receipt printer.

### Printer faults (section 10)

- **`paper-near-end`** sets `DLE EOT 4` bits 2-3 only; prints and `DLE EOT 1` stay normal.
- **`baud-mismatch`** applies a fixed, reversible scramble to every received byte
  (`rotate_left(3) ^ 0x5A`). Real garbage depends on the two baud rates; a fixed scramble
  shows the app the same symptoms (nonsense output, unanswered status requests) and keeps
  tests repeatable.
- **`garbled`** flips one random bit per corrupted byte, with probability `rate`. Devices are
  sans-IO and read no randomness, so the generator (xorshift64*) is seeded from the time the
  fault is turned on: the same steps under a fake clock corrupt the same bytes. `rate` outside
  0 to 1 is clamped, with a warning in the event log.
- **Where corruption shows:** the event log records the bytes the app sent; the job's
  `bytesHex` holds what the printer received, so both views are available.
- These faults apply to serial printers now. When HTTP printers arrive, `garbled` applies to
  them too and `baud-mismatch` does not (PRD section 10).

### Keyboard-wedge scanner (FR-26, section 8.4)

- **New `DeviceEvent::Wedge(WedgeScan)`** (maintainer decision; changes FR-01, additive). A
  real wedge scanner is a USB keyboard: it types each code key by key, then Enter or Tab. The
  scanner emits one `Wedge { code, delayMs, terminator }` per scan and the shim presses the
  keys, playing the computer's keyboard driver. This matches the PRD's existing `wedge-scan`
  bridge message one-to-one. Rejected: reusing `ToHost` bytes, which is how a _serial-mode_
  scanner works and loses the per-scan delay; and one message per key press, whose network
  jitter would break "a 20-barcode sequence arrives in order with the configured delay".
- **Event log:** scans are recorded as `wedge` records (`{ scan: { code, delayMs, terminator }
}`) with direction device-to-host, so the log filter (FR-29) treats them like status bytes.
- **`scanner-wedge` config:** `terminator` (default `enter`), `delayMs` (default 8),
  `prefix`, `suffix`. A scan may override `terminator` and `delayMs`. Empty codes and codes
  over 4,096 characters are ignored with a warning rather than typed.
- **`double-scan` timing:** the repeat starts `gapMs` after the first scan's _last_ key, i.e.
  at `(keys - 1) × delayMs + gapMs`. "Each scan is sent twice" with a gap only makes sense
  measured from the end of the first one. The repeat is a fault, so it does not count as a
  new scan in `scans`. Turning the fault off drops repeats not yet sent.
- **Who receives scans:** every bridge connection whose `hello` listed `wedge`, the way a
  keyboard types into whichever window has focus. Connections without `wedge` never get
  `wedge-scan` frames.
- **Default devices** now include `scanner-1`, so the dashboard has a scanner without config
  (the PRD's own example config in section 11.3 uses that ID).
- **Shim injector, where the spec leaves room:** legacy `keyCode`/`which`/`charCode` follow a
  US keyboard layout (barcode listeners commonly read them); characters with no US key get
  `keyCode` 0. Enter follows HTML implicit submission (click the default button; with none,
  submit only a single-field form). Tab moves to the next focusable element in tab order
  (positive `tabindex` first, then document order); it does not cross into iframes or shadow
  roots. Text is inserted with `setRangeText` so React's controlled inputs see the change.
  `isTrusted` is `false`, as section 8.4 says, documented in the shim README.
- **`installation.scan(deviceId, code, options?)`** in the shim (new public API) triggers a
  scan from the page in either mode: standalone resolves once the keys are typed; server mode
  calls `POST /devices/:id/scan` and resolves once the server has taken it. It exists so
  standalone-mode tests (and the M3 Playwright fixture) can scan without a server.
- **Dashboard sequence runner** runs in the page: it sends each scan, waits for the server to
  accept it, then waits the configured delay. Presets are kept in the browser's local storage
  (sample presets until the user saves their own); they are a convenience, not shared state.

## Between M1 and M2

- **Names secured (PRD section 16).** On 19 Oct 2026 the maintainer registered the npm
  organization `hardless` (scope `@hardless`), the Docker Hub account `hardless` (image
  `hardless/hardless`) and the GitHub organization `hardlessdev`, then moved the repository to
  <https://github.com/hardlessdev/hardless>. The GitHub name `hardless` belongs to a dormant
  personal account from 2013, and GitHub no longer releases dormant names on request, so the
  GitHub org differs from the npm scope. `hardlessdev` was picked because it is free on GitHub,
  npm and Docker Hub alike. Docker Hub uses a free personal account rather than an organization,
  which needs a paid plan; the image name is the same. The PRD's names for the npm packages and
  the Docker image are unchanged. Every repository URL now points at `hardlessdev/hardless`.
  The trademark search is still **open**.
- **Drawer sensor polarity is a printer setting** (maintainer decision, 19 Oct 2026; extends
  FR-38). `DLE EOT 1` bit 2 reports the level of connector pin 3, and real drawers are wired
  either way, so the meaning of the bit cannot be fixed. New optional printer config field
  `drawerOpenLevel`: `"high"` (default, unchanged M1 behaviour: bit 2 set means open) or
  `"low"` (bit 2 set means closed). The printer's `drawerOpen` state stays the logical value.
  An unplugged drawer (`not-connected`) still reads as closed, so with `"low"` it sets bit 2.
  The HTTP dialects' drawer status in M2 should apply the same setting.
- **Formatting carries over between receipts, as on real printers** (maintainer decision,
  19 Oct 2026). Settings such as bold or alignment sent without any printable text stay in
  effect for the next job, until `ESC @` resets them. This keeps the emulator honest: an app
  that forgets to reset formatting misbehaves here just as it would on a real printer. No code
  change; `printer_settings_without_printing_carry_into_the_next_job` pins the behaviour.

## M1 — Printer and drawer over Web Serial

### Maintainer decisions taken before starting

- **Printer and drawer connect through a peripheral link in `hardless-core`** (changes FR-01
  and FR-02). A device can name the device it is attached to; a device can emit a
  `PeripheralSignal` (the printer's drawer-kick pulse, the drawer's open sensor), and the
  registry delivers it between attached devices. This mirrors the real drawer-kick connector
  and keeps `cash-drawer` a separate device kind as in the PRD.
- **No PNG download in M1.** FR-24's "download PNG" button arrives with PNG rendering (FR-11)
  in M2. M1 offers the raw hex view and an HTML download.
- **`hardless start` ships in M1** (`--config`, `--port`, `--host`, `--token`), because FR-21
  needs CLI flags and the M1 acceptance test needs a running server. The other FR-30
  subcommands stay in M2.
- **New endpoint `GET /api/v1/stream`** (WebSocket): pushes event bus events
  (`device:added`, `device:removed`, `device:state`, `device:output`) to the dashboard.
  Section 5 says the dashboard receives jobs over a WebSocket, but the PRD defines none for it.
- **New endpoint `POST /api/v1/devices/:id/close`**: closes an open cash drawer (FR-25, FR-37),
  in the style of `/scan` and `/tap`.
- **New bridge message `signals`** (client to server): `{ deviceId, signals }` from
  `SerialPort.setSignals()`, recorded in the event log and acknowledged. Section 8.1 asks for
  DTR/RTS in the event log, but section 11.2 had no message to carry them.

### Where the Web Serial shim follows the spec instead of the PRD (section 8.1)

The PRD says to follow the WICG specification where they disagree. Checked against the spec
source (`WICG/serial`, `index.bs`) and Chrome:

- **`close()` with a locked stream rejects with `TypeError`, not `InvalidStateError`.** The
  spec's close() cancels the readable and aborts the writable; on a locked stream that fails,
  and Chrome reports `TypeError: Cannot cancel a locked stream`. Apps are written against
  Chrome, so the shim matches it, and rejects before changing anything.
- **`requestPort()` validates filters**: an empty filter, a `usbProductId` without
  `usbVendorId`, or Bluetooth and USB IDs together reject with `TypeError` (spec steps; not in
  the PRD).
- **`open()` validates every option**, not only `baudRate`: `dataBits` 7 or 8, `stopBits` 1 or
  2, `parity` and `flowControl` enum values, `bufferSize` above 0 and at most 16 MiB (Chrome's
  limit), all with WebIDL `[EnforceRange]` semantics.
- **`connect` and `disconnect` fire on the port and bubble to `navigator.serial`** (spec), with
  `event.target` being the port. `port.connected` reflects the state.
- **`getPorts()` returns only granted ports whose device is connected** ("available" in the
  spec). `forget()` removes the grant.
- **`getSignals()`** reports the inputs of a ready printer: CTS and DSR set, DCD and RI clear.
- After a disconnect, `readable` and `writable` become `null` but the port stays open until the
  app calls `close()`, as in the spec; it can then reopen once the device is back.

### Other M1 choices

- **`picker-cancel` is one-shot**, as PRD section 10 says ("next request"): the shim turns it
  off after using it, through `PUT /api/v1/devices/:id/faults/picker-cancel` in server mode.
  The REST API therefore allows CORS from local origins (any origin when a token is set).
- **Default devices.** Without a config file the server creates `printer-1` and `drawer-1`, so
  `hardless start` is enough for a first receipt.
- **Job closing.** Jobs close on a cut or after `idleMs` (default 300 ms). Status requests do
  not start a job. Settings sent without anything printable carry over into the next job.
- **Paper out and cover open** stop printing like a real printer: prints are dropped, buffered
  `ESC p` kicks are not executed, and real-time `DLE DC4` kicks still are. The app learns the
  cause through `DLE EOT` status, which is how ESC/POS apps detect it.
- **Drawer sensor**: `DLE EOT 1` bit 2 set means the drawer is open. Real drawers differ in
  polarity; this can become a printer setting if needed.
- **Code pages** come from the Windows definitions of CP437 and CP858.
- **Parser extras** beyond the phase 1 list (`CR`, `HT`, `ESC !`, `ESC M`, `GS h/w/H`,
  `ESC i/m`, `DLE DC4 1`), because apps send them constantly. Other known commands are
  skipped by length so their parameters are not printed.
- **Render endpoint** takes `unknown=0` to hide unknown-command badges (FR-12's toggle).
- **New dependencies**: `tokio`, `axum`, `qrcode`, `barcoders` (all named in section 6),
  `clap` for the CLI and `futures-util` for WebSocket handling (both MIT/Apache-2.0, widely
  used); `zustand` for the dashboard (section 6).
- **Sizes (NFR-08)**: `dist/hardless-shim.js` is 7.6 KiB gzipped (limit 30 KB); the
  WebAssembly engine is 169 KiB gzipped (limit 500 KB) and loads only in standalone mode.

## M0 — Foundation

### Toolchain

- **Rust 1.99 pinned** in `rust-toolchain.toml`; MSRV 1.97 (stable minus 2, NFR-11) in
  `rust-version` and checked by a separate CI job. Pinning keeps local and CI builds
  identical; bump both together.
- **Edition 2021** as the PRD says, although edition 2024 is available.
- **TypeScript 5.9** (`~5.9.3`): the PRD says TypeScript 5.x, so newer majors are not used.
- **pnpm 12.14.0** via the `packageManager` field.

### `hardless-core`

- **`std`, not `no_std + alloc`.** The PRD says "no_std + alloc where possible". The crate
  depends on `serde_json` and `schemars` in its public API (FR-01), and nothing in phase 1
  needs `no_std` (WebAssembly has `std`). Revisit for the phase 5 USB gadget if needed.
- **`bytes` dependency.** FR-01 uses `Bytes` in `DeviceEvent::ToHost`; the type comes from the
  `bytes` crate (MIT, tokio-rs, widely used, and already a dependency of axum). Re-exported as
  `hardless_core::Bytes`.
- **Job AST is JSON.** `Job.ast` is `serde_json::Value` so the core does not depend on any
  command set (ESC/POS now, ZPL/TSPL in phase 2). Devices fill it from their own typed AST.
- **Jobs are stored by the registry**, newest first, capped at 1,000 per device
  (`RegistryConfig::job_capacity`), so `GET /devices/:id/jobs` works the same for every
  printer route. Devices number their own jobs from 1.
- **Event bus is a drained queue.** Being sans-IO, the registry cannot push to subscribers;
  callers call `drain_bus()` after each operation. Undrained events are capped at 10,000.
- **Bus events are exactly the four in FR-02.** Device -> host bytes and connection changes
  are returned to the caller from every registry method instead of adding bus event types.
  Connection changes also publish `device:state`.
- **FR-01 changed (maintainer decision, M0): `set_fault` and `reset` take `now` and return
  `Vec<DeviceEvent>`**, like every other `Device` method. The original signatures gave a
  device no way to report what a fault caused (e.g. `offline` must disconnect immediately),
  which forced a hidden workaround: set a deadline of "now" and wait for the registry to tick.
  Device authors who forgot it would get faults that silently did nothing. The PRD's FR-01
  code block was updated. The registry still publishes `device:state` after every fault
  toggle and reset, because the active faults are part of the device summary.
- **Faults are open strings.** `FaultId` is a validated kebab-case string so plugins can add
  faults (goal 5); the section 10 IDs are constants and `catalogue()`.
- **FR-01/FR-02 changed (maintainer decision, M0): devices declare their faults.**
  `DevicePlugin::faults()` lists every fault a kind can support (for listing faults by kind,
  e.g. the MCP `list_faults` tool). `Device::faults()` lists the faults that apply to one
  instance, because applicability can depend on config: the same `printer-escpos` kind gets
  `baud-mismatch` over serial but `stall` over USB. `Registry::set_fault` rejects any other
  fault with `RegistryError::UnsupportedFault` (code `unsupported_fault`, HTTP 400 in M1),
  whether turning it on or off, and changes and records nothing. `DeviceSummary` gains
  `supportedFaults` so the dashboard fault panel (FR-28) shows only toggles that apply. Both
  return `Vec<FaultId>` so plugins can return their own fault IDs.
- **Device IDs** are 1–64 ASCII letters, digits, `-`, `_` and `.`, because they appear in URL
  paths. Generated IDs are `<kind>-<n>`.
- **Event log `seq`** is unique across all devices, so `?since=` works for merged views.
- **Event log records carry `deviceId`** and are JSON `{ seq, at, deviceId, type, ... }` with
  byte payloads as lowercase hex (`dataHex`), matching `bytesHex` in the REST job shape.
- **Monotonic time.** If the injected clock goes backwards, the registry keeps the last
  timestamp it used (FR-03 "monotonic timestamp").

### Wire types

- `cargo xtask gen-types` flattens ts-rs output into `packages/types/src`, rewrites imports
  to ESM (`./X.js`) and writes an `index.ts`. `u64` maps to `number` (all timestamps and
  sequence numbers fit in 2^53).
- Zod schemas (PRD section 6) are not generated yet; they are needed when the shim validates
  bridge messages in M1.

### Licences (NFR-14)

**Decided (maintainer, M0):** NFR-14 applies its strict list to what ships; build tools get a
slightly wider list; strong copyleft is banned everywhere. NFR-14 in the PRD was reworded to
say so. Why:

- `lightningcss` (MPL-2.0) is required by Vite 8 and Tailwind 4 (both PRD section 6). MPL-2.0
  is file-level weak copyleft: it only obliges sharing changes to its own files when they are
  distributed. It runs at build time and none of its code is in any output.
- `minimatch` (BlueOak-1.0.0) is required by ESLint. BlueOak-1.0.0 is an OSI-approved
  permissive licence (MIT-like, with a patent grant), so it joins the shipped list.
- The alternative, older Vite/Tailwind/ESLint versions, would trade a non-risk for an
  outdated toolchain.

How it is enforced:

- **One policy file**, `license-policy.json`: `shipped`, `buildToolsAlsoAllow` (only
  `MPL-2.0`) and `neverAllow` (GPL family, AGPL, SSPL, BUSL, non-commercial CC). Any licence
  not listed fails, so each new licence needs a decision here.
- **Rust**: `cargo-deny` applies the shipped list to every dependency, dev ones included
  (stricter than needed). Unpublished workspace crates are exempt (`licenses.private.ignore`).
- **npm declared dependencies**: `scripts/check-npm-licenses.mjs` (`pnpm run
check:licenses`) checks runtime `dependencies` against `shipped` and everything else against
  `shipped` + `buildToolsAlsoAllow`. It uses `pnpm licenses list` instead of the unmaintained
  `license-checker` package.
- **What is actually bundled**: Vite bundles whatever is imported, whichever dependency list
  it is in. The dashboard build (`packages/dashboard/build/licenses.ts`) checks every bundled
  package against `shipped` and fails the build otherwise.
- **Notices**: MIT, BSD, ISC and Apache-2.0 require their notices to accompany distributed
  copies. The dashboard build writes `THIRD_PARTY_LICENSES.txt` (full licence texts) and
  `third-party-licenses.json` into `dist/`. Today that is React, react-dom, scheduler,
  Tailwind's generated base CSS, and small Vite and rolldown runtime helpers, all MIT.
- **Follow-up for v0.1.0**: the `hardless` binary must also ship notices for its Rust
  dependencies (e.g. generated with `cargo-about` in the release workflow), and npm packages
  that bundle code must include theirs.

### Not in M0

- `cargo-fuzz` targets and the nightly fuzz job arrive with the first parser (M1).
- The device test suite under `wasm-bindgen-test` arrives with the first real device (M1).
- `llms.txt` and its CI sync check are FR-51 (M3).
