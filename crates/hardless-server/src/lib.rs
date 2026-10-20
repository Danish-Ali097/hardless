#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod config;
pub mod engine;
pub mod error;

mod api;
mod assets;
mod bridge;
mod http_printers;
mod security;
mod stream;
pub mod tls;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{State, WebSocketUpgrade};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{middleware, Json, Router};
use serde::Serialize;
use tokio::net::TcpListener;

pub use config::{DeviceSpec, Overrides, ServerConfig};
pub use engine::Engine;

/// State shared by every route.
#[derive(Clone, Debug)]
pub struct AppState {
    /// The device engine.
    pub engine: Arc<Engine>,
    /// The bearer token, if one is required (NFR-02).
    pub token: Option<Arc<str>>,
    /// Whether the server listens only on loopback (NFR-01).
    pub loopback_only: bool,
    /// Where the server listens, for `GET /api/v1/info`.
    pub info: Arc<ServerInfo>,
}

/// What `GET /api/v1/info` reports: enough for the dashboard to explain HTTPS printing
/// (FR-33).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    /// The server version.
    pub version: String,
    /// The HTTP port.
    pub http_port: u16,
    /// HTTPS, if it is running.
    pub https: HttpsInfo,
}

/// The HTTPS part of [`ServerInfo`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpsInfo {
    /// Whether HTTPS is serving.
    pub enabled: bool,
    /// The HTTPS port, when enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Where to download the CA certificate to trust, when enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate: Option<String>,
    /// The names and addresses the server certificate covers.
    pub hosts: Vec<String>,
    /// Why HTTPS is not running, if it was asked for and failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Builds every route: the REST API and `/api/v1/stream`, the `/bridge` WebSocket, the HTTP
/// printer endpoints, the CA certificate download and the dashboard at `/` (FR-18 to FR-21,
/// FR-33). `ca_pem` is the CA certificate when HTTPS is set up.
pub fn router(state: AppState, ca_pem: Option<Arc<str>>) -> Router {
    let api = api::routes()
        .route("/info", get(info))
        .route("/stream", get(stream_upgrade))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            security::require_token,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            security::cors,
        ));
    Router::new()
        .nest("/api/v1", api)
        .merge(http_printers::routes())
        .route(
            tls::CA_DOWNLOAD_PATH,
            get(move || {
                let ca_pem = ca_pem.clone();
                async move { ca_certificate(ca_pem) }
            }),
        )
        .route(
            "/bridge",
            get(bridge_upgrade).layer(middleware::from_fn_with_state(
                state.clone(),
                security::require_token,
            )),
        )
        .fallback(get(assets::serve))
        .with_state(state)
}

async fn info(State(state): State<AppState>) -> Json<ServerInfo> {
    Json((*state.info).clone())
}

/// The CA certificate, for phones and computers to trust (FR-33). It is public; the key stays
/// on this machine.
fn ca_certificate(ca_pem: Option<Arc<str>>) -> Response {
    match ca_pem {
        Some(pem) => (
            [
                (header::CONTENT_TYPE, "application/x-x509-ca-cert"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"hardless-ca.crt\"",
                ),
            ],
            pem.to_string(),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "HTTPS is off, so there is no certificate",
        )
            .into_response(),
    }
}

async fn bridge_upgrade(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| bridge::run(socket, state.engine))
}

async fn stream_upgrade(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| stream::run(socket, state.engine))
}

/// A server started with [`spawn`].
#[derive(Debug)]
pub struct RunningServer {
    /// The HTTP address it listens on.
    pub addr: SocketAddr,
    /// The HTTPS address, if HTTPS is running.
    pub https_addr: Option<SocketAddr>,
    /// What `GET /api/v1/info` reports.
    pub info: Arc<ServerInfo>,
    /// The device engine, for driving devices directly.
    pub engine: Arc<Engine>,
    task: tokio::task::JoinHandle<()>,
}

impl RunningServer {
    /// Stops the server.
    pub fn stop(self) {
        self.task.abort();
    }
}

/// Why the server could not start.
#[derive(Debug)]
pub enum StartError {
    /// A configured device could not be created.
    Devices(hardless_core::RegistryError),
    /// The address could not be bound.
    Bind(std::io::Error),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartError::Devices(e) => write!(f, "cannot create the configured devices: {e}"),
            StartError::Bind(e) => write!(f, "cannot listen: {e}"),
        }
    }
}

impl std::error::Error for StartError {}

/// Sets up HTTPS and binds its port. A failure is reported, not fatal: HTTP keeps working.
async fn start_https(config: &ServerConfig) -> Result<(TcpListener, tls::Tls), String> {
    let dir = config
        .cert_dir
        .clone()
        .unwrap_or_else(|| tls::default_cert_dir(&|k| std::env::var(k).ok()));
    let hosts = tls::certificate_hosts(config.host, &config.cert_hosts);
    let tls = tls::setup(&dir, hosts).map_err(|e| e.to_string())?;
    let listener = TcpListener::bind((config.host, config.https_port))
        .await
        .map_err(|e| format!("cannot listen on port {}: {e}", config.https_port))?;
    Ok((listener, tls))
}

/// Serves `app` over TLS: the same routes as HTTP, including WebSocket upgrades.
async fn serve_tls(listener: TcpListener, config: Arc<rustls::ServerConfig>, app: Router) {
    let acceptor = tokio_rustls::TlsAcceptor::from(config);
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(_) => {
                // Out of file handles or similar: back off instead of spinning.
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let acceptor = acceptor.clone();
        let app = app.clone();
        tokio::spawn(async move {
            // A failed handshake is normal: a browser that does not trust the CA yet.
            let Ok(stream) = acceptor.accept(stream).await else {
                return;
            };
            let service = hyper_util::service::TowerToHyperService::new(app);
            let _ =
                hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new())
                    .serve_connection_with_upgrades(hyper_util::rt::TokioIo::new(stream), service)
                    .await;
        });
    }
}

/// Starts the server in the background and returns once it is listening. Port 0 picks a
/// free port (see [`RunningServer::addr`] and [`RunningServer::https_addr`]).
///
/// ```no_run
/// # async fn demo() -> Result<(), hardless_server::StartError> {
/// let server = hardless_server::spawn(hardless_server::ServerConfig::default()).await?;
/// println!("dashboard at http://{}", server.addr);
/// # Ok(()) }
/// ```
pub async fn spawn(config: ServerConfig) -> Result<RunningServer, StartError> {
    let engine = Engine::new(&config.devices).map_err(StartError::Devices)?;
    let listener = TcpListener::bind((config.host, config.port))
        .await
        .map_err(StartError::Bind)?;
    let addr = listener.local_addr().map_err(StartError::Bind)?;

    let mut https_info = HttpsInfo::default();
    let mut https = None;
    if config.https {
        match start_https(&config).await {
            Ok((https_listener, tls)) => {
                let https_addr = https_listener.local_addr().map_err(StartError::Bind)?;
                https_info = HttpsInfo {
                    enabled: true,
                    port: Some(https_addr.port()),
                    certificate: Some(tls::CA_DOWNLOAD_PATH.to_owned()),
                    hosts: tls.hosts.clone(),
                    error: None,
                };
                https = Some((https_listener, https_addr, tls));
            }
            Err(error) => https_info.error = Some(error),
        }
    }
    let info = Arc::new(ServerInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        http_port: addr.port(),
        https: https_info,
    });
    let state = AppState {
        engine: engine.clone(),
        token: config.token.as_deref().map(Arc::from),
        loopback_only: config.is_loopback(),
        info: info.clone(),
    };
    let ca_pem = https
        .as_ref()
        .map(|(_, _, tls)| Arc::from(tls.ca_pem.as_str()));
    let app = router(state, ca_pem);
    let https_addr = https.as_ref().map(|(_, addr, _)| *addr);
    let timers = engine.clone();
    let task = tokio::spawn(async move {
        let timers = tokio::spawn(timers.run_timers());
        let secure = https
            .map(|(listener, _, tls)| tokio::spawn(serve_tls(listener, tls.config, app.clone())));
        let _ = axum::serve(listener, app).await;
        timers.abort();
        if let Some(secure) = secure {
            secure.abort();
        }
    });
    Ok(RunningServer {
        addr,
        https_addr,
        info,
        engine,
        task,
    })
}

/// Runs the server until Ctrl+C, printing where it listens and any security warnings.
pub async fn serve(config: ServerConfig) -> Result<(), StartError> {
    if !config.is_loopback() {
        eprintln!(
            "warning: listening on {} makes Hardless reachable from other machines (NFR-01).",
            config.host
        );
        if config.token.is_none() {
            eprintln!("warning: no --token is set, so anyone on the network can control the devices (NFR-02).");
        }
    }
    let server = spawn(config).await?;
    // 0.0.0.0 is where it listens, not an address to open: show localhost instead.
    let shown = |addr: SocketAddr| {
        if addr.ip().is_unspecified() {
            format!("localhost:{}", addr.port())
        } else {
            addr.to_string()
        }
    };
    let addr = shown(server.addr);
    println!("Hardless {} is running", env!("CARGO_PKG_VERSION"));
    println!("  dashboard  http://{addr}/");
    println!("  REST API   http://{addr}/api/v1");
    println!("  bridge     ws://{addr}/bridge");
    println!(
        "  printers   http://{addr}/cgi-bin/epos/service.cgi  http://{addr}/StarWebPRNT/SendMessage"
    );
    match (&server.https_addr, &server.info.https.error) {
        (Some(https), _) => {
            let https = shown(*https);
            println!(
                "  HTTPS      https://{https}/  (trust http://{addr}{} once per device)",
                tls::CA_DOWNLOAD_PATH
            );
            println!("             covers {}", server.info.https.hosts.join(", "));
        }
        (None, Some(error)) => eprintln!("warning: HTTPS is off: {error}"),
        (None, None) => {}
    }
    for device in server.engine.list() {
        println!("  device     {} ({})", device.id, device.kind);
    }
    let _ = tokio::signal::ctrl_c().await;
    println!("Stopping.");
    server.stop();
    Ok(())
}
