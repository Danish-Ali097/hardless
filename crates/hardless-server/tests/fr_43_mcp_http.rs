//! FR-43: the MCP server over Streamable HTTP at `/mcp`, behind the token (NFR-02).
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

mod common;

use common::{http_text, server, server_with};
use hardless_server::ServerConfig;
use serde_json::json;

fn initialize() -> String {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0" }
        }
    })
    .to_string()
}

const ACCEPT: (&str, &str) = ("Accept", "application/json, text/event-stream");

#[tokio::test]
async fn fr_43_mcp_answers_initialize_over_http() {
    let server = server().await;
    let (status, _, body) = http_text(
        &server,
        "POST",
        "/mcp",
        &initialize(),
        Some("application/json"),
        &[ACCEPT],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"name\":\"hardless\""), "{body}");
    assert!(body.contains("\"tools\""), "{body}");
}

#[tokio::test]
async fn nfr_02_mcp_needs_the_token_when_one_is_set() {
    let server = server_with(ServerConfig {
        port: 0,
        https: false,
        token: Some("secret".into()),
        ..ServerConfig::default()
    })
    .await;
    let (status, _, _) = http_text(
        &server,
        "POST",
        "/mcp",
        &initialize(),
        Some("application/json"),
        &[ACCEPT],
    )
    .await;
    assert_eq!(status, 401);
    let (status, _, body) = http_text(
        &server,
        "POST",
        "/mcp",
        &initialize(),
        Some("application/json"),
        &[ACCEPT, ("Authorization", "Bearer secret")],
    )
    .await;
    assert_eq!(status, 200, "{body}");
}

#[tokio::test]
async fn fr_43_mcp_on_localhost_refuses_other_host_names() {
    let server = server().await;
    let (status, _, _) = http_text(
        &server,
        "POST",
        "/mcp",
        &initialize(),
        Some("application/json"),
        &[ACCEPT, ("Host", "attacker.example:8080")],
    )
    .await;
    assert_eq!(status, 403);
}
