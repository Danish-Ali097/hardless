# @hardless/types

TypeScript types for every Hardless wire message: event bus events, device summaries, event
log records, print jobs, control commands and the fault catalogue.

**Generated, never edited by hand.** The source of truth is the Rust types in
`crates/hardless-core`. After changing them, run:

```sh
cargo xtask gen-types
```

CI runs `cargo xtask gen-types --check` and fails if this package is stale.

## Usage

```ts
import type { BusEvent } from "@hardless/types";

function onBusEvent(event: BusEvent) {
  if (event.type === "device:output") {
    console.log(event.deviceId, event.job.text);
  }
}
```
