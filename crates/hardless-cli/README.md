# hardless-cli

The `hardless` binary (FR-30). `hardless start` runs the server; the other commands control a
running server through its REST API, so scripts and CI jobs can drive devices without a
browser.

Part of [Hardless](https://github.com/hardlessdev/hardless), an open-source POS hardware
emulator. Licensed under Apache-2.0.

## Commands

```sh
hardless start [--config FILE] [--host IP] [--port 8080] [--https-port 8443] [--token T]

hardless scan scanner-1 4006381333931 [--terminator enter|tab|none] [--delay-ms 8]
hardless tap nfc-1 --uid 04a2b3c4d5e6f7 [--records '[...]']
hardless fault printer-1 paper-out on
hardless fault printer-1 disconnect-mid-write on --option afterBytes=32
hardless fault printer-1 paper-out off
hardless receipts printer-1 --last                      # the latest receipt as text
hardless receipts printer-1 --limit 5                   # the five newest
hardless receipts printer-1 --format json
hardless receipts printer-1 --format png --out receipt.png [--width 58]
hardless mcp --url http://127.0.0.1:8080 [--read-only] [--embedded]   # MCP server on stdio
hardless licenses                                       # third-party notices for this binary
```

`scan`, `tap`, `fault` and `receipts` talk to `http://127.0.0.1:8080` by default; set another
server with `--server` or `HARDLESS_SERVER`, and its token with `--token` or `HARDLESS_TOKEN`.
They print one line on success, and on failure print the server's error and exit with status 1.
`--format html`, `svg` and `png` render the latest receipt; PNG needs `--out`.

`tap` puts a card on an NFC reader (`nfc-1` by default) and reads it: pages using the
`NDEFReader` shim get a `reading` event. Without `--records` the reader uses what it
remembers of the card, including anything the app wrote to it. Records use Web NFC's field
names, e.g. `'[{"recordType":"text","data":"Member 1234","lang":"en"}]'`. `hardless mcp`
is the MCP server for AI coding agents (see crates/hardless-mcp): it speaks MCP on stdin and
stdout, so only the agent should start it.
