//! FR-21 (config precedence and the dashboard at `/`), FR-22 (ports), NFR-01, NFR-02.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

mod common;

use std::net::{IpAddr, Ipv4Addr};

use common::{get_json, http, server, server_with};
use hardless_server::{Overrides, ServerConfig};
use serde_json::json;

fn no_env(_: &str) -> Option<String> {
    None
}

#[test]
fn fr_22_default_ports_and_nfr_01_loopback_host() {
    let config = ServerConfig::load(&no_env, &Overrides::default()).unwrap();
    assert_eq!(config.port, 8080);
    assert_eq!(config.https_port, 8443);
    assert_eq!(config.host, IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert!(config.is_loopback());
    assert!(config.token.is_none());
}

#[test]
fn fr_21_file_then_env_then_flags() {
    let dir = std::env::temp_dir().join(format!("hardless-config-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("hardless.config.json");
    std::fs::write(
        &file,
        json!({
            "port": 7000,
            "token": "from-file",
            "devices": [{ "kind": "printer-escpos", "id": "only" }]
        })
        .to_string(),
    )
    .unwrap();
    let path = file.to_string_lossy().into_owned();

    let env = |k: &str| (k == "HARDLESS_CONFIG").then(|| path.clone());
    let from_file = ServerConfig::load(&env, &Overrides::default()).unwrap();
    assert_eq!(from_file.port, 7000);
    assert_eq!(from_file.token.as_deref(), Some("from-file"));
    assert_eq!(from_file.devices.len(), 1);

    let env = |k: &str| match k {
        "HARDLESS_CONFIG" => Some(path.clone()),
        "HARDLESS_PORT" => Some("7100".into()),
        "HARDLESS_TOKEN" => Some("from-env".into()),
        _ => None,
    };
    let from_env = ServerConfig::load(&env, &Overrides::default()).unwrap();
    assert_eq!(from_env.port, 7100);
    assert_eq!(from_env.token.as_deref(), Some("from-env"));

    let flags = Overrides {
        port: Some(7200),
        token: Some("from-flag".into()),
        ..Default::default()
    };
    let from_flags = ServerConfig::load(&env, &flags).unwrap();
    assert_eq!(from_flags.port, 7200);
    assert_eq!(from_flags.token.as_deref(), Some("from-flag"));
    assert_eq!(
        from_flags.devices.len(),
        1,
        "devices still come from the file"
    );
}

#[test]
fn fr_21_bad_config_is_a_clear_error() {
    let env = |k: &str| (k == "HARDLESS_PORT").then(|| "eighty".to_owned());
    let err = ServerConfig::load(&env, &Overrides::default()).unwrap_err();
    assert!(err.to_string().contains("HARDLESS_PORT"), "{err}");

    let flags = Overrides {
        config: Some("does-not-exist.json".into()),
        ..Default::default()
    };
    assert!(ServerConfig::load(&no_env, &flags).is_err());
}

#[tokio::test]
async fn fr_21_serves_the_dashboard_at_root() {
    let server = server().await;
    let (status, headers, body) = http(&server, "GET", "/", None, &[]).await;
    assert_eq!(status, 200);
    assert!(headers
        .iter()
        .any(|(k, v)| k == "content-type" && v.starts_with("text/html")));
    assert!(body.contains("Hardless"));
    // Client-side routes fall back to index.html.
    let (status, _, _) = http(&server, "GET", "/devices/printer-1", None, &[]).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn nfr_02_token_protects_the_api_and_bridge() {
    let server = server_with(ServerConfig {
        port: 0,
        https: false,
        token: Some("s3cret".into()),
        ..ServerConfig::default()
    })
    .await;
    let (status, body) = get_json(&server, "/api/v1/devices").await;
    assert_eq!(status, 401);
    assert_eq!(body["error"]["code"], "unauthorized");

    let (status, _, _) = http(
        &server,
        "GET",
        "/api/v1/devices",
        None,
        &[("Authorization", "Bearer s3cret")],
    )
    .await;
    assert_eq!(status, 200);
    let (status, _, _) = http(&server, "GET", "/api/v1/devices?token=s3cret", None, &[]).await;
    assert_eq!(status, 200);

    let url = format!("ws://{}/bridge", server.addr);
    assert!(
        tokio_tungstenite::connect_async(url).await.is_err(),
        "bridge without token"
    );
    let url = format!("ws://{}/bridge?token=s3cret", server.addr);
    assert!(tokio_tungstenite::connect_async(url).await.is_ok());
}
