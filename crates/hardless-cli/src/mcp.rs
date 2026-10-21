//! `hardless mcp`: the MCP server for coding agents over stdio (FR-43), using the REST API of a
//! running server, or of one started here with `--embedded` (FR-44). Stdout carries MCP
//! messages only; everything for people goes to stderr.

use std::process::ExitCode;
use std::sync::Arc;

use hardless_mcp::{BoxFuture, Method, Rest, RestResponse};
use hardless_server::ServerConfig;
use hyper::Uri;
use serde_json::Value;

use crate::client::{Client, ClientError};

/// The REST API through the CLI's HTTP client.
struct Http(Client);

impl Rest for Http {
    fn send<'a>(
        &'a self,
        method: Method,
        path: &'a str,
        body: Option<Value>,
    ) -> BoxFuture<'a, Result<RestResponse, String>> {
        Box::pin(async move {
            let method = match method {
                Method::Get => hyper::Method::GET,
                Method::Post => hyper::Method::POST,
                Method::Put => hyper::Method::PUT,
                Method::Delete => hyper::Method::DELETE,
            };
            match self.0.send(method, path, body.as_ref()).await {
                Ok((status, body)) => Ok(RestResponse {
                    status,
                    body: body.to_vec(),
                }),
                Err(e) => Err(e.to_string()),
            }
        })
    }
}

/// Starts a server on the URL's port when nothing answers there (FR-44).
async fn embedded_server(
    client: &Client,
    url: &str,
) -> Result<Option<hardless_server::RunningServer>, String> {
    if client
        .send(hyper::Method::GET, "/health", None)
        .await
        .is_ok()
    {
        return Ok(None);
    }
    let port = url
        .parse::<Uri>()
        .ok()
        .and_then(|uri| uri.port_u16())
        .unwrap_or(8080);
    let config = ServerConfig {
        port,
        ..ServerConfig::default()
    };
    let server = hardless_server::spawn(config)
        .await
        .map_err(|e| format!("cannot start an embedded server on port {port}: {e}"))?;
    eprintln!(
        "hardless mcp: started an embedded Hardless server; dashboard at http://{}/",
        server.addr
    );
    Ok(Some(server))
}

/// Runs `hardless mcp` until the agent disconnects.
pub fn run(url: &str, token: Option<String>, read_only: bool, embedded: bool) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("hardless mcp: cannot start the runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result: Result<(), String> = runtime.block_on(async {
        let client = Client::new(url, token).map_err(|e: ClientError| e.to_string())?;
        // Kept alive while serving.
        let _server = if embedded {
            embedded_server(&client, url).await?
        } else {
            None
        };
        hardless_mcp::serve_stdio(Arc::new(Http(client)), read_only).await
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hardless mcp: {e}");
            ExitCode::FAILURE
        }
    }
}
