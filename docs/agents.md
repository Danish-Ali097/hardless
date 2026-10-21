# Using Hardless from an AI agent

Hardless ships an [MCP](https://modelcontextprotocol.io) server, so a coding agent can work on
your app's hardware code the way you would: trigger a print, look at the rendered receipt,
fix the layout, then turn on a fault and check that the app handles it.

## Set it up

Add Hardless to your agent's MCP configuration. Most agents (Claude Code, Cursor, VS Code and
others) accept this shape:

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

- Start Hardless first (`hardless start`), or add `"--embedded"` to the arguments to have
  `hardless mcp` start one when none is running.
- `"--read-only"` gives the agent only the tools that read: it can look at receipts and the
  event log but not change devices, scan, tap or set faults.
- Agents running in a container or on another machine can use Streamable HTTP instead:
  `http://<server>:8080/mcp`. When the server was started with `--token`, send
  `Authorization: Bearer <token>`.

## What the agent can do

| Tool                                             | Use it to                                          |
| ------------------------------------------------ | -------------------------------------------------- |
| `list_devices`                                   | learn the device IDs and their state               |
| `wait_for_job`                                   | wait for the next receipt after triggering a print |
| `get_last_receipt`                               | see the latest receipt as text and as a PNG image  |
| `list_jobs`, `get_job`                           | look at earlier receipts                           |
| `decode_escpos`                                  | check bytes the code builds, without a device      |
| `set_fault`, `list_faults`                       | turn on paper out, offline, read errors and more   |
| `scan_barcode`, `tap_nfc`                        | feed the app a barcode or an NFC card              |
| `get_events`                                     | read the bytes the app sent, decoded               |
| `run_scenario`                                   | run a timed sequence of scans, faults and checks   |
| `create_device`, `remove_device`, `reset_device` | change the device set                              |

The full list, with each tool's inputs, is in the [MCP tool reference](reference/mcp-tools.md).
Receipt tools return the plain text, the job's details and a PNG, so an agent that reads
images sees alignment, QR codes and cuts. The server also offers the ESC/POS command list, the
fault catalogue and the HTTP printer reference as resources.

## A typical loop

1. The agent edits the app's receipt code.
2. It triggers a print: runs a Playwright test, or calls `run_scenario`.
3. It calls `wait_for_job` and checks the text and the image.
4. It fixes the code and repeats until the receipt is right.
5. It calls `set_fault` with `paper-out`, prints again and checks that the app shows its
   error, then turns the fault off.

## Safety

The MCP server can do only what the dashboard and REST API can. Like them, it listens on
localhost unless the server is started with `--host`, and a server started with `--token`
requires the token for `/mcp` too.
