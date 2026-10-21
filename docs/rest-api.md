# REST API

Base URL `http://localhost:8080/api/v1` (and `https://localhost:8443/api/v1`). JSON in and
out. Errors are `{ "error": { "code", "message" } }` with status 400, 401, 404, 409 or 500.
When the server was started with `--token`, send `Authorization: Bearer <token>`.

| Method | Path                                                                 | Does                                                                           |
| ------ | -------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| GET    | `/devices`                                                           | Every device: `{ id, kind, state, faults, supportedFaults, attachedTo }`       |
| POST   | `/devices`                                                           | Creates a device from `{ kind, id?, config }`                                  |
| GET    | `/devices/:id`                                                       | One device                                                                     |
| DELETE | `/devices/:id`                                                       | Removes a device                                                               |
| POST   | `/devices/:id/reset`                                                 | Clears state, jobs and faults                                                  |
| GET    | `/devices/:id/jobs?limit=`                                           | Print jobs, newest first: `{ id, at, ast, text, bytesHex }`                    |
| GET    | `/devices/:id/jobs/last`                                             | The latest job                                                                 |
| GET    | `/devices/:id/jobs/:jobId/render?format=html\|svg\|png&width=58\|80` | A rendered receipt                                                             |
| POST   | `/devices/:id/scan`                                                  | Scans `{ code, terminator?, delayMs? }` on a keyboard-wedge scanner            |
| POST   | `/devices/:id/tap`                                                   | Taps `{ uid, records? }` on an NFC reader                                      |
| POST   | `/devices/:id/close`                                                 | Closes an open cash drawer                                                     |
| PUT    | `/devices/:id/faults/:faultId`                                       | Turns a fault on or off: `{ active, options? }`                                |
| GET    | `/devices/:id/events?since=&limit=`                                  | The event log, oldest first                                                    |
| POST   | `/scenarios/run`                                                     | Runs a scenario and answers with its report                                    |
| GET    | `/health`                                                            | Liveness, for Docker and CI                                                    |
| GET    | `/info`                                                              | Version, ports, HTTPS certificate link and hosts                               |
| GET    | `/stream` (WebSocket)                                                | Live events: `device:added`, `device:removed`, `device:state`, `device:output` |

Other routes on the same server:

- `/` — the dashboard.
- `/bridge` (WebSocket) — the browser shim's connection to the devices.
- `/mcp` — the [MCP server](agents.md) for AI agents.
- `/cgi-bin/epos/service.cgi?devid=<id>` and `/StarWebPRNT/SendMessage` (or
  `/p/<id>/StarWebPRNT/SendMessage`) — the [HTTP printers](reference/http-dialects.md).
- `/hardless-ca.crt` — the certificate to trust for HTTPS.

## Examples

```sh
curl -X PUT localhost:8080/api/v1/devices/printer-1/faults/paper-out \
  -H 'Content-Type: application/json' -d '{"active":true}'
curl localhost:8080/api/v1/devices/printer-1/jobs/last
curl -X POST localhost:8080/api/v1/devices/nfc-1/tap \
  -H 'Content-Type: application/json' \
  -d '{"uid":"04a2b3c4","records":[{"recordType":"text","data":"Member 1234"}]}'
```

## Scenarios

A scenario is a list of timed steps run on the server's devices:

```json
{
  "name": "paper runs out",
  "steps": [
    { "type": "scan", "device": "scanner-1", "code": "4006381333931" },
    { "type": "wait", "ms": 200 },
    { "type": "setFault", "device": "printer-1", "fault": "paper-out" },
    { "type": "expect", "device": "printer-1", "state": { "paperOut": true }, "withinMs": 500 }
  ]
}
```

Step types: `scan`, `tap`, `wait`, `setFault`, `clearFault` and `expect`, which checks part of
a device's `state`, text the latest receipt must contain (`receiptContains`) or a number of
`jobs`, optionally waiting up to `withinMs`. The report lists each step as `passed`,
`failed` or `skipped`; the run stops at the first failure.
