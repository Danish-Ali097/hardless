//! FR-18: the WebSocket bridge (PRD section 11.2), plus the dashboard stream.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

mod common;

use std::time::{Duration, Instant};

use common::{b64, bridge, recv, recv_type, send, send_json, server, ws};
use serde_json::json;

#[tokio::test]
async fn fr_18_hello_returns_the_device_list() {
    let server = server().await;
    let mut ws = ws(&server, "/bridge").await;
    send(&mut ws, json!({ "v": 1, "type": "hello", "id": "1", "origin": "http://localhost", "shimVersion": "0.0.0", "apis": ["serial"] })).await;
    let devices = recv(&mut ws).await;
    assert_eq!(devices["v"], 1);
    assert_eq!(devices["type"], "devices");
    assert_eq!(devices["devices"].as_array().unwrap().len(), 2);
    assert_eq!(
        recv(&mut ws).await,
        json!({ "v": 1, "type": "ack", "replyTo": "1" })
    );
}

#[tokio::test]
async fn fr_18_open_write_and_status_reply() {
    let server = server().await;
    let mut ws = bridge(&server).await;
    send(&mut ws, json!({ "v": 1, "type": "open", "id": "o", "deviceId": "printer-1", "options": { "baudRate": 9600 } })).await;
    assert_eq!(recv_type(&mut ws, "ack").await["replyTo"], "o");

    // DLE EOT 1 comes back as a `data` frame.
    send(&mut ws, json!({ "v": 1, "type": "write", "id": "w", "deviceId": "printer-1", "data": b64(b"\x10\x04\x01") })).await;
    let data = recv_type(&mut ws, "data").await;
    assert_eq!(data["deviceId"], "printer-1");
    assert_eq!(data["data"], b64(&[0x12]));
    assert_eq!(recv_type(&mut ws, "ack").await["replyTo"], "w");
}

#[tokio::test]
async fn fr_18_a_print_over_the_bridge_becomes_a_job() {
    let server = server().await;
    let mut ws = bridge(&server).await;
    send(
        &mut ws,
        json!({ "v": 1, "type": "open", "id": "o", "deviceId": "printer-1" }),
    )
    .await;
    recv_type(&mut ws, "ack").await;
    send(&mut ws, json!({ "v": 1, "type": "write", "id": "w", "deviceId": "printer-1", "data": b64(b"Total: 1,250\n\x1dV\x00") })).await;
    recv_type(&mut ws, "ack").await;
    let (status, job) = common::get_json(&server, "/api/v1/devices/printer-1/jobs/last").await;
    assert_eq!(status, 200);
    assert_eq!(job["text"], "Total: 1,250\n");
}

#[tokio::test]
async fn fr_18_errors_reply_to_the_failed_message() {
    let server = server().await;
    let mut ws = bridge(&server).await;
    send(
        &mut ws,
        json!({ "v": 1, "type": "write", "id": "w", "deviceId": "printer-1", "data": "" }),
    )
    .await;
    let err = recv_type(&mut ws, "error").await;
    assert_eq!(
        err["code"], "invalid_state",
        "writing to a port that is not open"
    );
    assert_eq!(err["replyTo"], "w");

    send(
        &mut ws,
        json!({ "v": 1, "type": "open", "id": "x", "deviceId": "nope" }),
    )
    .await;
    assert_eq!(recv_type(&mut ws, "error").await["code"], "not_found");

    send(&mut ws, json!({ "v": 2, "type": "hello", "id": "v" })).await;
    assert_eq!(recv_type(&mut ws, "error").await["code"], "bad_message");

    send(
        &mut ws,
        json!({ "v": 1, "type": "open", "id": "a", "deviceId": "printer-1" }),
    )
    .await;
    recv_type(&mut ws, "ack").await;
    send(
        &mut ws,
        json!({ "v": 1, "type": "open", "id": "b", "deviceId": "printer-1" }),
    )
    .await;
    assert_eq!(
        recv_type(&mut ws, "error").await["code"],
        "invalid_state",
        "already open"
    );
}

#[tokio::test]
async fn fr_18_a_port_is_open_in_one_connection_at_a_time() {
    let server = server().await;
    let mut a = bridge(&server).await;
    let mut b = bridge(&server).await;
    send(
        &mut a,
        json!({ "v": 1, "type": "open", "id": "1", "deviceId": "printer-1" }),
    )
    .await;
    recv_type(&mut a, "ack").await;
    send(
        &mut b,
        json!({ "v": 1, "type": "open", "id": "1", "deviceId": "printer-1" }),
    )
    .await;
    assert_eq!(recv_type(&mut b, "error").await["code"], "network");
    // Closing the first connection frees the port.
    drop(a);
    tokio::time::sleep(Duration::from_millis(100)).await;
    send(
        &mut b,
        json!({ "v": 1, "type": "open", "id": "2", "deviceId": "printer-1" }),
    )
    .await;
    assert_eq!(recv_type(&mut b, "ack").await["replyTo"], "2");
}

#[tokio::test]
async fn fr_18_offline_fault_sends_disconnect_and_connect() {
    let server = server().await;
    let mut ws = bridge(&server).await;
    send(
        &mut ws,
        json!({ "v": 1, "type": "open", "id": "o", "deviceId": "printer-1" }),
    )
    .await;
    recv_type(&mut ws, "ack").await;
    send_json(
        &server,
        "PUT",
        "/api/v1/devices/printer-1/faults/offline",
        json!({ "active": true }),
    )
    .await;
    assert_eq!(
        recv_type(&mut ws, "disconnect").await["deviceId"],
        "printer-1"
    );
    // The port closed with the device: writing now fails.
    send(
        &mut ws,
        json!({ "v": 1, "type": "write", "id": "w", "deviceId": "printer-1", "data": b64(b"x") }),
    )
    .await;
    assert_eq!(recv_type(&mut ws, "error").await["code"], "invalid_state");
    // And it cannot be reopened while offline.
    send(
        &mut ws,
        json!({ "v": 1, "type": "open", "id": "o2", "deviceId": "printer-1" }),
    )
    .await;
    assert_eq!(recv_type(&mut ws, "error").await["code"], "network");

    send_json(
        &server,
        "PUT",
        "/api/v1/devices/printer-1/faults/offline",
        json!({ "active": false }),
    )
    .await;
    assert_eq!(recv_type(&mut ws, "connect").await["deviceId"], "printer-1");
}

#[tokio::test]
async fn fr_18_disconnect_mid_write_fails_the_write() {
    let server = server().await;
    let mut ws = bridge(&server).await;
    send(
        &mut ws,
        json!({ "v": 1, "type": "open", "id": "o", "deviceId": "printer-1" }),
    )
    .await;
    recv_type(&mut ws, "ack").await;
    send_json(
        &server,
        "PUT",
        "/api/v1/devices/printer-1/faults/disconnect-mid-write",
        json!({ "active": true, "options": { "afterBytes": 4 } }),
    )
    .await;
    send(&mut ws, json!({ "v": 1, "type": "write", "id": "w", "deviceId": "printer-1", "data": b64(b"Total: 1,250\n") })).await;
    let err = recv_type(&mut ws, "error").await;
    assert_eq!(
        (err["code"].as_str(), err["replyTo"].as_str()),
        (Some("network"), Some("w"))
    );
}

#[tokio::test]
async fn fr_18_slow_write_delays_the_ack() {
    let server = server().await;
    let mut ws = bridge(&server).await;
    send(
        &mut ws,
        json!({ "v": 1, "type": "open", "id": "o", "deviceId": "printer-1" }),
    )
    .await;
    recv_type(&mut ws, "ack").await;
    send_json(
        &server,
        "PUT",
        "/api/v1/devices/printer-1/faults/slow-write",
        json!({ "active": true, "options": { "msPerChunk": 300 } }),
    )
    .await;
    let start = Instant::now();
    send(
        &mut ws,
        json!({ "v": 1, "type": "write", "id": "w", "deviceId": "printer-1", "data": b64(b"x") }),
    )
    .await;
    recv_type(&mut ws, "ack").await;
    assert!(
        start.elapsed() >= Duration::from_millis(300),
        "{:?}",
        start.elapsed()
    );
}

#[tokio::test]
async fn fr_18_device_changes_resend_the_device_list() {
    let server = server().await;
    let mut ws = bridge(&server).await;
    send_json(
        &server,
        "POST",
        "/api/v1/devices",
        json!({ "kind": "printer-escpos", "id": "bar" }),
    )
    .await;
    let devices = recv_type(&mut ws, "devices").await;
    assert!(devices["devices"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["id"] == "bar"));
}

#[tokio::test]
async fn stream_pushes_bus_events_to_the_dashboard() {
    let server = server().await;
    let mut stream = ws(&server, "/api/v1/stream").await;
    let printer = "printer-1".parse().unwrap();
    server
        .engine
        .drive(&printer, |r| r.host_bytes(&printer, b"Hello\n\x1dV\x00"))
        .unwrap();
    let output = recv_type(&mut stream, "device:output").await;
    assert_eq!(output["deviceId"], "printer-1");
    assert_eq!(output["job"]["text"], "Hello\n");
}

#[tokio::test]
async fn bridge_round_trip_is_fast() {
    // NFR-06 asks for p95 < 10 ms on localhost; this is a loose smoke check.
    let server = server().await;
    let mut ws = bridge(&server).await;
    send(
        &mut ws,
        json!({ "v": 1, "type": "open", "id": "o", "deviceId": "printer-1" }),
    )
    .await;
    recv_type(&mut ws, "ack").await;
    let mut times = Vec::new();
    for i in 0..50 {
        let start = Instant::now();
        send(&mut ws, json!({ "v": 1, "type": "write", "id": format!("w{i}"), "deviceId": "printer-1", "data": b64(b"\x10\x04\x01") })).await;
        recv_type(&mut ws, "ack").await;
        times.push(start.elapsed());
    }
    times.sort();
    assert!(
        times[47] < Duration::from_millis(100),
        "p95 {:?}",
        times[47]
    );
}

#[tokio::test]
async fn fr_18_signals_are_recorded_in_the_event_log() {
    let server = server().await;
    let mut ws = bridge(&server).await;
    send(&mut ws, json!({ "v": 1, "type": "signals", "id": "s0", "deviceId": "printer-1", "signals": { "dataTerminalReady": true } })).await;
    assert_eq!(
        recv_type(&mut ws, "error").await["code"],
        "invalid_state",
        "port not open"
    );
    send(
        &mut ws,
        json!({ "v": 1, "type": "open", "id": "o", "deviceId": "printer-1" }),
    )
    .await;
    recv_type(&mut ws, "ack").await;
    send(&mut ws, json!({ "v": 1, "type": "signals", "id": "s1", "deviceId": "printer-1", "signals": { "dataTerminalReady": true, "requestToSend": false } })).await;
    assert_eq!(recv_type(&mut ws, "ack").await["replyTo"], "s1");
    let (_, events) = common::get_json(&server, "/api/v1/devices/printer-1/events").await;
    let signals = events
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "signals")
        .unwrap();
    assert_eq!(
        signals["signals"],
        json!({ "dataTerminalReady": true, "requestToSend": false })
    );
}
