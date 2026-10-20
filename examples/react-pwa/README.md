# Corner Café: example POS PWA

A small point-of-sale PWA that scans barcodes, prints receipts and opens a cash drawer, the way
a real one does. In development it uses Hardless instead of hardware. The app's hardware code
knows nothing about Hardless:

- [src/printer.ts](src/printer.ts): a receipt printer on Web Serial (Chrome and Edge).
- [src/epos.ts](src/epos.ts): a network printer that speaks ePOS-Print XML over HTTP, the
  only way to print from Safari on iPhone and iPad.
- The barcode field: a USB scanner is a keyboard, so a scan is typed into it and Enter adds
  the item.

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

## Tests

- `pnpm --filter @hardless/example-react-pwa test`: unit tests for the receipt builders.
- `pnpm --filter @hardless/example-react-pwa test:acceptance`: the PRD M1 and M2 acceptance
  tests, in Chromium and WebKit (needs `cargo build -p hardless-cli` with the dashboard
  built). The iPhone and iPad check above is manual.
