# @hardless/types

[Hardless](https://github.com/hardlessdev/hardless) emulates POS hardware (receipt printers,
cash drawers, barcode scanners and NFC readers) so web apps and PWAs can be developed and
tested without the devices. This package has the TypeScript types for every Hardless wire
message: event bus events, device summaries, event log records, print jobs, control commands
and the fault catalogue. Use it when you talk to the Hardless REST API or event stream
yourself; [`@hardless/shim-web`](https://www.npmjs.com/package/@hardless/shim-web) and
[`@hardless/playwright`](https://www.npmjs.com/package/@hardless/playwright) already include
it.

```sh
npm install --save-dev @hardless/types
```

## Usage

```ts
import type { BusEvent } from "@hardless/types";

function onBusEvent(event: BusEvent) {
  if (event.type === "device:output") {
    console.log(event.deviceId, event.job.text);
  }
}
```

## Contributing

**Generated, never edited by hand.** The source of truth is the Rust types in
`crates/hardless-core` in the repository. After changing them, run:

```sh
cargo xtask gen-types
```

CI runs `cargo xtask gen-types --check` and fails if this package is stale.

## Links

- Documentation: <https://hardless-docs.netlify.app>
- The `hardless` server and dashboard: `npx hardless start` ([hardless](https://www.npmjs.com/package/hardless))
- Source and issues: <https://github.com/hardlessdev/hardless>

Apache-2.0.
