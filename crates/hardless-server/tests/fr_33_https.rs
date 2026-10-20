//! FR-22 and FR-33: HTTPS with a local certificate authority (NFR-05).
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use common::{get_json, http_text, server_with};
use hardless_server::{RunningServer, ServerConfig};
use rustls::pki_types::{pem::PemObject, CertificateDer, ServerName};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// A fresh certificate folder, so tests never touch the user's own CA.
fn cert_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "hardless-test-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn https_server(dir: &std::path::Path) -> RunningServer {
    server_with(ServerConfig {
        port: 0,
        https: true,
        https_port: 0,
        cert_dir: Some(dir.to_owned()),
        ..ServerConfig::default()
    })
    .await
}

/// Sends a raw HTTP/1.1 request over TLS, trusting only `ca_pem`, and returns the response.
async fn https_request(server: &RunningServer, ca_pem: &str, request: &str) -> String {
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(CertificateDer::from_pem_slice(ca_pem.as_bytes()).unwrap())
        .unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let tcp = TcpStream::connect(server.https_addr.unwrap())
        .await
        .unwrap();
    let mut tls = connector
        .connect(ServerName::try_from("localhost").unwrap(), tcp)
        .await
        .expect("the handshake succeeds with the Hardless CA");
    tls.write_all(request.as_bytes()).await.unwrap();
    // Read until the server closes, or, for an upgraded WebSocket that stays open, until the
    // response head is in and nothing more arrives.
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let wait = std::time::Duration::from_secs(if out.is_empty() { 5 } else { 1 });
        match tokio::time::timeout(wait, tls.read(&mut buf)).await {
            Ok(Ok(0)) | Ok(Err(_)) => break,
            Ok(Ok(n)) => out.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

async fn ca_pem(server: &RunningServer) -> String {
    let (status, headers, pem) = http_text(server, "GET", "/hardless-ca.crt", "", None, &[]).await;
    assert_eq!(status, 200);
    assert!(headers
        .iter()
        .any(|(k, v)| k == "content-type" && v == "application/x-x509-ca-cert"));
    assert!(pem.starts_with("-----BEGIN CERTIFICATE-----"), "{pem}");
    pem
}

#[tokio::test]
async fn fr_33_https_serves_the_same_routes_with_a_trusted_ca() {
    let dir = cert_dir("routes");
    let server = https_server(&dir).await;
    let pem = ca_pem(&server).await;

    let response = https_request(
        &server,
        &pem,
        "GET /api/v1/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");

    // An ePOS print over HTTPS, as an HTTPS PWA would send it.
    let doc = r#"<epos-print xmlns="http://www.epson-pos.com/schemas/2011/03/epos-print"><text>secure</text></epos-print>"#;
    let request = format!(
        "POST /cgi-bin/epos/service.cgi?devid=local_printer HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: text/xml\r\nContent-Length: {}\r\n\r\n{doc}",
        doc.len()
    );
    let response = https_request(&server, &pem, &request).await;
    assert!(response.contains(r#"success="true""#), "{response}");

    // WebSocket upgrades work over TLS, so the shim can use wss:// from an HTTPS page.
    let response = https_request(
        &server,
        &pem,
        "GET /bridge HTTP/1.1\r\nHost: localhost\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n",
    )
    .await;
    assert!(response.starts_with("HTTP/1.1 101"), "{response}");
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn fr_33_info_reports_https_for_the_dashboard() {
    let dir = cert_dir("info");
    let server = https_server(&dir).await;
    let (status, info) = get_json(&server, "/api/v1/info").await;
    assert_eq!(status, 200);
    assert_eq!(info["httpPort"], server.addr.port());
    assert_eq!(info["https"]["enabled"], true);
    assert_eq!(info["https"]["port"], server.https_addr.unwrap().port());
    assert_eq!(info["https"]["certificate"], "/hardless-ca.crt");
    let hosts = info["https"]["hosts"].as_array().unwrap();
    assert!(hosts.contains(&serde_json::json!("localhost")));
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn nfr_05_the_ca_is_made_once_and_kept() {
    let dir = cert_dir("keep");
    let first = https_server(&dir).await;
    let pem = ca_pem(&first).await;
    first.stop();
    let second = https_server(&dir).await;
    assert_eq!(ca_pem(&second).await, pem, "the same CA, trusted once");
    assert!(dir.join("hardless-ca-key.pem").exists());
    // The server certificate is shared with other local dev servers, e.g. Vite.
    let shared = std::fs::read_to_string(dir.join("hardless-server.pem")).unwrap();
    assert!(shared.ends_with(&pem), "the chain ends with the CA");
    assert!(dir.join("hardless-server-key.pem").exists());
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn fr_33_https_off_means_no_certificate() {
    let server = common::server().await;
    assert!(server.https_addr.is_none());
    let (status, _, _) = http_text(&server, "GET", "/hardless-ca.crt", "", None, &[]).await;
    assert_eq!(status, 404);
    let (_, info) = get_json(&server, "/api/v1/info").await;
    assert_eq!(info["https"]["enabled"], false);
}

#[tokio::test]
async fn fr_33_a_broken_certificate_folder_leaves_http_running() {
    // A file where the folder should be: HTTPS cannot start, HTTP still does.
    let file = cert_dir("broken").join("not-a-folder");
    std::fs::write(&file, "x").unwrap();
    let server = https_server(&file).await;
    assert!(server.https_addr.is_none());
    let (status, info) = get_json(&server, "/api/v1/info").await;
    assert_eq!(status, 200);
    assert!(info["https"]["error"].as_str().is_some(), "{info}");
}
