# hardless-server

The Hardless server (tokio + axum). It owns all I/O and time for the sans-IO engine.

| Path                                        | What                                                                  |
| ------------------------------------------- | --------------------------------------------------------------------- |
| `/`                                         | The dashboard (embedded from `packages/dashboard/dist` at build time) |
| `/api/v1/...`                               | REST control API (PRD section 11.1), including `GET /api/v1/info`     |
| `/api/v1/stream`                            | WebSocket: the event bus, for the dashboard                           |
| `/bridge`                                   | WebSocket bridge for the browser shim (PRD section 11.2)              |
| `/cgi-bin/epos/service.cgi?devid=&timeout=` | ePOS-Print-compatible HTTP printer (PRD section 9.1)                  |
| `/StarWebPRNT/SendMessage`                  | WebPRNT-compatible HTTP printer: the first HTTP printer               |
| `/p/<deviceId>/StarWebPRNT/SendMessage`     | WebPRNT-compatible HTTP printer by device ID                          |
| `/hardless-ca.crt`                          | The local CA certificate to trust for HTTPS                           |

Every route is served on HTTP (port 8080) and HTTPS (port 8443).

Configuration (FR-21) comes from `hardless.config.json`, then `HARDLESS_*` environment
variables, then CLI flags. Without a config, it creates `printer-1` (ESC/POS on Web Serial)
with `drawer-1` attached, `scanner-1` (keyboard wedge) and `local_printer` (an HTTP printer;
`local_printer` is the ePOS-Print default device ID). It listens on `127.0.0.1` unless told
otherwise (NFR-01); with a token (NFR-02), the API needs `Authorization: Bearer <token>` and
WebSockets need `?token=<token>`. The HTTP printer endpoints need no token and accept any
origin, like a printer on the network.

## HTTP printers (FR-31 to FR-35)

Printers with `"transport": "http"` answer both dialects. A request is translated to ESC/POS
(`hardless-http-dialects`) and printed by the same printer device as Web Serial, then the
response reports the printer's status. CORS (FR-32) echoes the request's origin and allows
`Content-Type`, `SOAPAction` and `If-Modified-Since`, plus Chrome's Private Network Access
preflight. Faults: `paper-out`, `paper-near-end`, `cover-open`, `garbled` as on any printer;
`offline` drops the connection without an answer; `timeout` holds the answer until after the
client's timeout (the ePOS `timeout`, or 90 s for WebPRNT); `cors-blocked` leaves out the CORS
headers.

## HTTPS (FR-22, FR-33)

On first run the server makes a small private certificate authority, "Hardless Local CA", in
the user's data folder (`%APPDATA%\hardless\certs` on Windows, `~/Library/Application
Support/hardless/certs` on macOS, `~/.local/share/hardless/certs` on Linux). It is never
committed or shipped in an image (NFR-05). At each start the CA signs a server certificate,
valid for 397 days, for `localhost`, `127.0.0.1`, `::1`, the address given with `--host`, the
LAN address when listening on `0.0.0.0`, and any `certHosts`.

Trust the CA once per device, downloading it from `/hardless-ca.crt` (the dashboard shows the
steps for iPhone and iPad, Android, macOS and Windows); later server certificates are trusted
automatically. If HTTPS cannot start, HTTP keeps running and `GET /api/v1/info` says why.

| Setting (config file / environment)                   | Default             |
| ----------------------------------------------------- | ------------------- |
| `httpsPort` / `HARDLESS_HTTPS_PORT` (`--https-port`)  | `8443`              |
| `https` / `HARDLESS_HTTPS` (`0`, `false` to turn off) | on                  |
| `certDir` / `HARDLESS_CERT_DIR`                       | the per-user folder |
| `certHosts` / `HARDLESS_CERT_HOSTS` (comma-separated) | none                |

Note: curl on Windows checks certificate revocation, which local CAs (like mkcert's) do not
support; pass `--ssl-no-revoke`. Browsers do not need this.

## Usage

```rust,no_run
use hardless_server::{spawn, ServerConfig};

# async fn demo() -> Result<(), hardless_server::StartError> {
let server = spawn(ServerConfig { port: 0, https: false, ..ServerConfig::default() }).await?;
println!("listening on {}", server.addr);
server.stop();
# Ok(()) }
```

Most people run it through the CLI: `hardless start`.

## License

Apache-2.0
