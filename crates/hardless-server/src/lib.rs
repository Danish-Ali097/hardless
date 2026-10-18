#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod config;
pub mod engine;
pub mod error;

mod api;
mod assets;
mod bridge;
mod security;
mod stream;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{State, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use axum::{middleware, Router};
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
}

/// Builds every route: the REST API and `/api/v1/stream`, the `/bridge` WebSocket and the
/// dashboard at `/` (FR-18, FR-20, FR-21).
pub fn router(state: AppState) -> Router {
    let api = api::routes()
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

async fn bridge_upgrade(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| bridge::run(socket, state.engine))
}

async fn stream_upgrade(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| stream::run(socket, state.engine))
}

/// A server started with [`spawn`].
#[derive(Debug)]
pub struct RunningServer {
    /// The address it listens on.
    pub addr: SocketAddr,
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

/// Starts the server in the background and returns once it is listening. Port 0 picks a
/// free port (see [`RunningServer::addr`]).
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
    let state = AppState {
        engine: engine.clone(),
        token: config.token.as_deref().map(Arc::from),
        loopback_only: config.is_loopback(),
    };
    let app = router(state);
    let timers = engine.clone();
    let task = tokio::spawn(async move {
        let timers = tokio::spawn(timers.run_timers());
        let _ = axum::serve(listener, app).await;
        timers.abort();
    });
    Ok(RunningServer { addr, engine, task })
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
    let addr = server.addr;
    println!("Hardless {} is running", env!("CARGO_PKG_VERSION"));
    println!("  dashboard  http://{addr}/");
    println!("  REST API   http://{addr}/api/v1");
    println!("  bridge     ws://{addr}/bridge");
    for device in server.engine.list() {
        println!("  device     {} ({})", device.id, device.kind);
    }
    let _ = tokio::signal::ctrl_c().await;
    println!("Stopping.");
    server.stop();
    Ok(())
}
