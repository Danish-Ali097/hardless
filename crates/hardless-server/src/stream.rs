//! `GET /api/v1/stream`: the event bus over a WebSocket, for the dashboard.
//!
//! Each frame is one bus event as JSON (`device:added`, `device:removed`, `device:state`,
//! `device:output`). If the connection falls behind, it gets `{ "type": "lagged" }` and
//! should reload the device list.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast::error::RecvError;

use crate::engine::{Engine, Event};

/// Runs one stream connection until it closes.
pub async fn run(socket: WebSocket, engine: Arc<Engine>) {
    let mut events = engine.subscribe();
    let (mut tx, mut rx) = socket.split();
    loop {
        let text = tokio::select! {
            incoming = rx.next() => match incoming {
                Some(Ok(_)) => continue,
                Some(Err(_)) | None => break,
            },
            event = events.recv() => match event {
                Ok(event) => match event.as_ref() {
                    Event::Bus(bus) => serde_json::to_string(bus).unwrap_or_default(),
                    Event::Device { .. } => continue,
                },
                Err(RecvError::Lagged(_)) => r#"{"type":"lagged"}"#.to_owned(),
                Err(RecvError::Closed) => break,
            },
        };
        if tx.send(Message::Text(text.into())).await.is_err() {
            break;
        }
    }
}
