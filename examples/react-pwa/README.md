# Corner Café: example POS PWA

A small point-of-sale PWA that prints receipts and opens a cash drawer over Web Serial, the
way a real one does. In development it uses Hardless instead of hardware: the app's printer
code ([src/printer.ts](src/printer.ts)) is plain Web Serial and does not know about Hardless.
Only [src/main.tsx](src/main.tsx) installs the shim, in development builds.

## Try it

From the repository root:

```sh
pnpm install
pnpm --filter @hardless/dashboard build
cargo run -p hardless-cli -- start          # dashboard at http://localhost:8080
pnpm --filter @hardless/example-react-pwa dev   # app at http://localhost:5173
```

In the app:

1. **Connect printer**, then choose `printer-1` in the port picker.
2. **Pay cash**: the receipt appears in the dashboard, rendered, and the drawer opens.
3. In the dashboard, turn on **Paper out**, then **Pay card**: the app reads the printer's
   status and reports that the receipt did not print.

## Tests

- `pnpm --filter @hardless/example-react-pwa test`: unit tests for the receipt builder.
- `pnpm --filter @hardless/example-react-pwa test:acceptance`: the PRD milestone M1
  acceptance test in Chromium (needs `cargo build -p hardless-cli` with the dashboard built).
