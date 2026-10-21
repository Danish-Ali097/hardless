# hardless-mcp

An [MCP](https://modelcontextprotocol.io) server so AI coding agents can drive Hardless's
emulated POS devices: list devices, read printed receipts as text and as an image, scan
barcodes, tap NFC cards and toggle faults. It is a thin, typed layer over the REST API
(PRD section 11.4): every agent action is one a person could take in the dashboard, and no
device logic lives here. Built on [`rmcp`](https://crates.io/crates/rmcp), the official Rust
MCP SDK.

Part of [Hardless](https://github.com/hardlessdev/hardless), an open-source POS hardware
emulator. Licensed under Apache-2.0.

## Use it from an agent

The `hardless` binary serves it over stdio (FR-43):

```json
{
  "mcpServers": {
    "hardless": {
      "command": "hardless",
      "args": ["mcp", "--url", "http://localhost:8080"],
      "env": { "HARDLESS_TOKEN": "" }
    }
  }
}
```

`--embedded` starts a Hardless server in the same process when none answers at `--url`
(FR-44), so one command is enough. `--read-only` offers only the tools that read (FR-48).
Agents in containers or on other machines use Streamable HTTP at `/mcp` on the server
instead (with `Authorization: Bearer <token>` when the server has one).

## Tools (FR-45)

| Tool               | Does                                                       | Hint        |
| ------------------ | ---------------------------------------------------------- | ----------- |
| `list_devices`     | Devices with kind, state and active faults                 | read-only   |
| `create_device`    | Creates a device from `kind`, `id?`, `config`              |             |
| `remove_device`    | Removes a device                                           | destructive |
| `reset_device`     | Clears state, receipts and faults                          | destructive |
| `get_last_receipt` | Latest receipt: text, PNG image and job metadata           | read-only   |
| `list_jobs`        | Receipts, newest first                                     | read-only   |
| `get_job`          | One receipt, like `get_last_receipt`                       | read-only   |
| `wait_for_job`     | Waits for the next receipt (default 10 s)                  | read-only   |
| `decode_escpos`    | Decodes ESC/POS bytes (hex or base64) to text, PNG and AST | read-only   |
| `scan_barcode`     | Scans on a keyboard-wedge scanner                          |             |
| `tap_nfc`          | Taps an NFC card                                           |             |
| `set_fault`        | Turns a fault on or off                                    |             |
| `list_faults`      | The fault catalogue, optionally for one device kind        | read-only   |
| `get_events`       | The event log, with bytes as hex and decoded ESC/POS       | read-only   |
| `run_scenario`     | Runs a scenario and returns its step results               |             |

Every tool has input and output JSON Schemas. Receipt tools return a text block, structured
content and, unless `format: "text"`, a PNG image block (FR-47), so multimodal agents can
see the layout, QR codes and cuts. Failures set `isError` and name the REST error code and
a next step, e.g. "device `nope` not found (not_found). Call `list_devices` to see the device
IDs." (FR-49).

## Resources (FR-46)

`hardless://devices` (JSON), `hardless://devices/{id}/receipts/last` (plain text),
`hardless://docs/escpos-commands`, `hardless://docs/faults` and `hardless://docs/http-dialects`
(Markdown, from the same sources as the rest of the docs: the fault catalogue and the
`hardless-escpos` and `hardless-http-dialects` READMEs).

## In Rust

`serve_stdio` and `http_service` take any [`Rest`] implementation: the CLI's HTTP client, or
the server's routes called in-process.

## License

Apache-2.0
