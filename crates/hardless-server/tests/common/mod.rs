//! Helpers: a server on a free port, REST calls and WebSocket clients.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, missing_docs)]

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use hardless_server::{spawn, RunningServer, ServerConfig};
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub async fn server() -> RunningServer {
    server_with(ServerConfig {
        port: 0,
        ..ServerConfig::default()
    })
    .await
}

pub async fn server_with(config: ServerConfig) -> RunningServer {
    spawn(config).await.unwrap()
}

/// A minimal HTTP/1.1 client over a raw socket, so tests need no HTTP client crate.
pub async fn http(
    server: &RunningServer,
    method: &str,
    path: &str,
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> (u16, Vec<(String, String)>, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = TcpStream::connect(server.addr).await.unwrap();
    let body = body.map(|b| b.to_string()).unwrap_or_default();
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: {}\r\n",
        server.addr,
        body.len()
    );
    if !body.is_empty() {
        request.push_str("Content-Type: application/json\r\n");
    }
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(&body);
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.unwrap();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let mut lines = head.lines();
    let status: u16 = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(": "))
        .map(|(k, v)| (k.to_ascii_lowercase(), v.to_owned()))
        .collect();
    let chunked = headers
        .iter()
        .any(|(k, v)| k == "transfer-encoding" && v.contains("chunked"));
    let body = if chunked {
        dechunk(body)
    } else {
        body.to_owned()
    };
    (status, headers, body)
}

fn dechunk(mut body: &str) -> String {
    let mut out = String::new();
    while let Some((size, rest)) = body.split_once("\r\n") {
        let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
        if n == 0 {
            break;
        }
        out.push_str(&rest[..n]);
        body = &rest[n + 2..];
    }
    out
}

pub async fn get_json(server: &RunningServer, path: &str) -> (u16, Value) {
    let (status, _, body) = http(server, "GET", path, None, &[]).await;
    (status, serde_json::from_str(&body).unwrap_or(Value::Null))
}

pub async fn send_json(
    server: &RunningServer,
    method: &str,
    path: &str,
    body: Value,
) -> (u16, Value) {
    let (status, _, text) = http(server, method, path, Some(body), &[]).await;
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

pub async fn ws(server: &RunningServer, path: &str) -> Ws {
    let url = format!("ws://{}{path}", server.addr);
    tokio_tungstenite::connect_async(url).await.unwrap().0
}

pub async fn send(ws: &mut Ws, frame: Value) {
    ws.send(Message::text(frame.to_string())).await.unwrap();
}

/// The next JSON text frame, or panics after two seconds.
pub async fn recv(ws: &mut Ws) -> Value {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(2), ws.next())
            .await
            .expect("timed out waiting for a frame")
            .expect("connection closed")
            .unwrap();
        if let Message::Text(text) = message {
            return serde_json::from_str(text.as_str()).unwrap();
        }
    }
}

/// Receives frames until one has `type == wanted` (skipping device list updates etc.).
pub async fn recv_type(ws: &mut Ws, wanted: &str) -> Value {
    for _ in 0..200 {
        let frame = recv(ws).await;
        if frame["type"] == wanted {
            return frame;
        }
    }
    panic!("no `{wanted}` frame");
}

/// A bridge connection that has said hello.
pub async fn bridge(server: &RunningServer) -> Ws {
    let mut ws = ws(server, "/bridge").await;
    send(
        &mut ws,
        json!({ "v": 1, "type": "hello", "id": "h", "origin": "http://localhost:5173", "shimVersion": "0.0.0", "apis": ["serial"] }),
    )
    .await;
    recv_type(&mut ws, "ack").await;
    ws
}

pub fn b64(data: &[u8]) -> String {
    hardless_core::base64::encode(data)
}
