# hardless-server

The Hardless server (tokio + axum). It owns all I/O and time for the sans-IO engine.

| Path             | What                                                                  |
| ---------------- | --------------------------------------------------------------------- |
| `/`              | The dashboard (embedded from `packages/dashboard/dist` at build time) |
| `/api/v1/...`    | REST control API (PRD section 11.1)                                   |
| `/api/v1/stream` | WebSocket: the event bus, for the dashboard                           |
| `/bridge`        | WebSocket bridge for the browser shim (PRD section 11.2)              |

Configuration (FR-21) comes from `hardless.config.json`, then `HARDLESS_*` environment
variables, then CLI flags. Without a config, it creates `printer-1` (ESC/POS) and `drawer-1`
attached to it. It listens on `127.0.0.1:8080` unless told otherwise (NFR-01); with a token
(NFR-02), the API needs `Authorization: Bearer <token>` and WebSockets need `?token=<token>`.

## Usage

```rust,no_run
use hardless_server::{spawn, ServerConfig};

# async fn demo() -> Result<(), hardless_server::StartError> {
let server = spawn(ServerConfig { port: 0, ..ServerConfig::default() }).await?;
println!("listening on {}", server.addr);
server.stop();
# Ok(()) }
```

Most people run it through the CLI: `hardless start`.

## License

Apache-2.0
