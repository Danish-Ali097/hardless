#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod docs;
mod rest;
mod server;

use std::sync::Arc;

use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::ServiceExt;

pub use rest::{BoxFuture, Method, Rest, RestResponse};
pub use server::{tool_list, HardlessMcp, MUTATING_TOOLS};

/// Serves MCP over stdin and stdout until the client disconnects (FR-43, `hardless mcp`).
/// Nothing else may write to stdout meanwhile.
pub async fn serve_stdio(rest: Arc<dyn Rest>, read_only: bool) -> Result<(), String> {
    let service = HardlessMcp::new(rest, read_only)
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| e.to_string())?;
    service.waiting().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// The Streamable HTTP endpoint (FR-43, `/mcp` on the server): a tower service to mount in a
/// router. `allowed_hosts` are the `Host` names it answers, which blocks DNS rebinding against
/// a server on localhost; `None` answers any `Host`, for a server already exposed to the
/// network (where its token protects it).
pub fn http_service(
    rest: Arc<dyn Rest>,
    read_only: bool,
    allowed_hosts: Option<Vec<String>>,
) -> StreamableHttpService<HardlessMcp, LocalSessionManager> {
    let config = match allowed_hosts {
        Some(hosts) => StreamableHttpServerConfig::default().with_allowed_hosts(hosts),
        None => StreamableHttpServerConfig::default().disable_allowed_hosts(),
    };
    StreamableHttpService::new(
        move || Ok(HardlessMcp::new(rest.clone(), read_only)),
        Arc::new(LocalSessionManager::default()),
        config,
    )
}
