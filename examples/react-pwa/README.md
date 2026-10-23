# Corner Café: example POS PWA

A small point-of-sale PWA that scans barcodes, prints receipts, opens a cash drawer and reads
loyalty cards over NFC, the way a real one does. In development it uses Hardless instead of hardware. The app's hardware code
knows nothing about Hardless:

- [src/printer.ts](src/printer.ts): a receipt printer on Web Serial (Chrome and Edge).
- [src/epos.ts](src/epos.ts): a network printer that speaks ePOS-Print XML over HTTP, the
  only way to print from Safari on iPhone and iPad.
- The barcode field: a USB scanner is a keyboard, so a scan is typed into it and Enter adds
  the item.
- [src/loyalty.ts](src/loyalty.ts): loyalty cards with Web NFC (`NDEFReader`): read the
  visit count from a card, and stamp a visit by writing to it.

Only [src/main.tsx](src/main.tsx) installs the Hardless shim, in development builds.

## Try it

From the repository root:

```sh
pnpm install
pnpm --filter @hardless/dashboard build
cargo run -p hardless-cli -- start          # dashboard at http://localhost:8080
pnpm --filter @hardless/example-react-pwa dev   # app at http://localhost:5173
```

In the app:

1. **Connect printer**, choose `printer-1` in the port picker, then **Pay cash**: the receipt
   appears in the dashboard, rendered, and the drawer opens.
2. Choose **Network (ePOS-Print)** and pay again: the receipt goes to `local_printer` over HTTP.
3. Click the barcode field, then in the dashboard open `scanner-1` and scan the
   `4006381333931` preset: a flat white is added to the order. Barcodes the till knows:
   `4006381333931` (flat white), `8964000123456` (croissant), `5012345678900` (orange juice).
4. In the dashboard, turn on **Paper out** for the printer you use, then **Pay card**: the app
   reports that the receipt did not print.
5. **Scan loyalty card**, then in the dashboard open `nfc-1`, make a card with a text record
   `Visits: 2` and tap it: the app shows the visits. **Stamp card** within two seconds (while
   the card is on the reader) writes `Visits: 3` to it; the dashboard shows the write.

## On an iPhone or iPad (HTTPS)

Safari has no Web Serial, so it prints over the network, and a PWA is normally on HTTPS:

1. Start Hardless for the LAN: `cargo run -p hardless-cli -- start --host 0.0.0.0`.
2. On the device, open `http://<computer's LAN address>:8080/hardless-ca.crt` and trust the
   Hardless certificate (the dashboard's HTTP printer panel lists the steps).
3. Serve the app over HTTPS with the same certificate:
   `HTTPS=1 pnpm --filter @hardless/example-react-pwa dev --host`.
4. On the device, open `https://<LAN address>:5173`, choose **Network (ePOS-Print)** and pay:
   the app uses `https://<LAN address>:8443/...` and the receipt appears in the dashboard.
5. To see mixed content, change the printer address to the `http://...:8080/...` one and pay
   again: Safari blocks the call and the app cannot reach the printer, as with a real one.

CI runs this check without a device: the [Safari workflow](../../.github/workflows/safari.yml)
opens [safari-check.html](safari-check.html) (which prints with the app's own `src/epos.ts`)
in Mobile Safari on an iPhone and an iPad simulator and in desktop Safari, on a GitHub Mac,
over HTTPS at the Mac's LAN address. Each must print over HTTPS and have plain HTTP blocked.
It runs weekly; start it any time from Actions → Safari → Run workflow. Screenshots are kept
with the run. A real device is still needed to check real Wi-Fi and installing the
certificate profile by hand.

## Tests

- `pnpm --filter @hardless/example-react-pwa test`: unit tests for the receipt builders and
  loyalty cards.
- `pnpm --filter @hardless/example-react-pwa test:acceptance`: the PRD M1, M2 and M3
  acceptance tests, in Chromium and WebKit (needs `cargo build -p hardless-cli` with the
  dashboard built, `pnpm --filter @hardless/shim-web build` and `cargo xtask build-wasm`).
  M3 ([e2e/m3.spec.ts](e2e/m3.spec.ts)) tests a production build, which contains no shim,
  with [`@hardless/playwright`](../../packages/playwright): once with the devices in the page
  (project `m3-standalone`) and once against a server (`m3-server`; set `HARDLESS_SERVER` to
  use one already running, as CI does with the Docker image). The iPhone and iPad check above
  runs in the Safari workflow.
