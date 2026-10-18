---
"@hardless/shim-web": minor
"@hardless/types": minor
---

Web Serial shim: `install({ mode: "server" | "standalone" })` replaces `navigator.serial` with an emulated port picker, ports, streams, events and errors that follow the Web Serial spec. Adds the `SerialSignals` type and the bridge `signals` message.
