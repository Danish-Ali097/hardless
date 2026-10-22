# hardless-wasm

The Hardless engine compiled to WebAssembly with `wasm-bindgen`, lazy-loaded by `@hardless/shim-web` in standalone mode (FR-15).

It runs the same device emulation as the server inside the page, so tests need no server. It
is not published to crates.io; it ships inside `@hardless/shim-web`.

Part of [Hardless](https://github.com/hardlessdev/hardless), an open-source POS hardware
emulator. Licensed under Apache-2.0.
