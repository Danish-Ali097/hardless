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
    assert_eq!(devices["devices"].as_array().unwrap().len(), 6);
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

/// Every frame `ws` receives within `ms`, without failing on silence.
async fn frames_within(ws: &mut common::Ws, ms: u64) -> Vec<serde_json::Value> {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
    let mut frames = Vec::new();
    while let Ok(Some(Ok(message))) = tokio::time::timeout_at(deadline, ws.next()).await {
        if let Message::Text(text) = message {
            frames.push(serde_json::from_str(text.as_str()).unwrap());
        }
    }
    frames
}

#[tokio::test]
async fn fr_26_wedge_scans_reach_only_shims_that_asked_for_the_wedge() {
    let server = server().await;
    let mut wedge = ws(&server, "/bridge").await;
    send(&mut wedge, json!({ "v": 1, "type": "hello", "id": "h", "origin": "http://localhost", "shimVersion": "0.0.0", "apis": ["serial", "wedge"] })).await;
    recv_type(&mut wedge, "ack").await;
    let mut serial_only = bridge(&server).await;

    let (status, body) = send_json(
        &server,
        "POST",
        "/api/v1/devices/scanner-1/scan",
        json!({ "code": "4006381333931" }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        recv_type(&mut wedge, "wedge-scan").await,
        json!({ "v": 1, "type": "wedge-scan", "deviceId": "scanner-1", "code": "4006381333931", "delayMs": 8, "terminator": "enter" })
    );
    let others = frames_within(&mut serial_only, 300).await;
    assert!(
        others.iter().all(|f| f["type"] != "wedge-scan"),
        "{others:?}"
    );
}

#[tokio::test]
async fn fault_double_scan_repeat_arrives_over_the_bridge_on_time() {
    let server = server().await;
    let mut ws = ws(&server, "/bridge").await;
    send(&mut ws, json!({ "v": 1, "type": "hello", "id": "h", "origin": "http://localhost", "shimVersion": "0.0.0", "apis": ["wedge"] })).await;
    recv_type(&mut ws, "ack").await;
    let (status, _) = send_json(
        &server,
        "PUT",
        "/api/v1/devices/scanner-1/faults/double-scan",
        json!({ "active": true, "options": { "gapMs": 100 } }),
    )
    .await;
    assert_eq!(status, 200);
    let start = Instant::now();
    send_json(
        &server,
        "POST",
        "/api/v1/devices/scanner-1/scan",
        json!({ "code": "1", "delayMs": 0 }),
    )
    .await;
    assert_eq!(recv_type(&mut ws, "wedge-scan").await["code"], "1");
    assert_eq!(recv_type(&mut ws, "wedge-scan").await["code"], "1");
    assert!(
        start.elapsed() >= Duration::from_millis(100),
        "the repeat waits for the gap"
    );
}

async fn nfc_shim(server: &hardless_server::RunningServer) -> common::Ws {
    let mut ws = ws(server, "/bridge").await;
    send(&mut ws, json!({ "v": 1, "type": "hello", "id": "h", "origin": "http://localhost", "shimVersion": "0.0.0", "apis": ["nfc"] })).await;
    recv_type(&mut ws, "ack").await;
    ws
}

#[tokio::test]
async fn fr_27_taps_reach_only_shims_that_asked_for_nfc() {
    let server = server().await;
    let mut nfc = nfc_shim(&server).await;
    let mut serial_only = bridge(&server).await;
    let (status, body) = send_json(
        &server,
        "POST",
        "/api/v1/devices/nfc-1/tap",
        json!({ "uid": "04a2b3c4", "records": [{ "recordType": "text", "data": "Hi", "lang": "en" }] }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        recv_type(&mut nfc, "nfc-reading").await,
        json!({ "v": 1, "type": "nfc-reading", "deviceId": "nfc-1", "serialNumber": "04:a2:b3:c4",
                "records": [{ "recordType": "text", "data": "Hi", "lang": "en" }] })
    );
    let others = frames_within(&mut serial_only, 300).await;
    assert!(
        others.iter().all(|f| f["type"] != "nfc-reading"),
        "{others:?}"
    );
}

#[tokio::test]
async fn spec_8_3_nfc_write_and_make_read_only_over_the_bridge() {
    let server = server().await;
    let mut ws = nfc_shim(&server).await;
    let write = |id: &str| json!({ "v": 1, "type": "nfc-write", "id": id, "deviceId": "nfc-1", "records": [{ "recordType": "url", "data": "https://example.com" }] });

    // No card on the reader: the shim waits for a tap.
    send(&mut ws, write("w1")).await;
    let err = recv_type(&mut ws, "error").await;
    assert_eq!(
        (err["code"].as_str(), err["replyTo"].as_str()),
        (Some("invalid_state"), Some("w1"))
    );

    send_json(
        &server,
        "POST",
        "/api/v1/devices/nfc-1/tap",
        json!({ "uid": "04a2b3c4" }),
    )
    .await;
    recv_type(&mut ws, "nfc-reading").await;
    send(&mut ws, write("w2")).await;
    assert_eq!(recv_type(&mut ws, "ack").await["replyTo"], "w2");
    let (_, device) = common::get_json(&server, "/api/v1/devices").await;
    let reader = device
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == "nfc-1")
        .unwrap()
        .clone();
    assert_eq!(
        reader["state"]["lastWritten"]["records"][0]["data"],
        "https://example.com"
    );

    send(
        &mut ws,
        json!({ "v": 1, "type": "nfc-make-read-only", "id": "l1", "deviceId": "nfc-1" }),
    )
    .await;
    assert_eq!(recv_type(&mut ws, "ack").await["replyTo"], "l1");
    send(&mut ws, write("w3")).await;
    let err = recv_type(&mut ws, "error").await;
    assert_eq!(
        (err["code"].as_str(), err["replyTo"].as_str()),
        (Some("not_allowed"), Some("w3"))
    );

    // A device that is not an NFC reader cannot be written to.
    send(
        &mut ws,
        json!({ "v": 1, "type": "nfc-write", "id": "w4", "deviceId": "printer-1", "records": [] }),
    )
    .await;
    let err = recv_type(&mut ws, "error").await;
    assert_eq!(
        (err["code"].as_str(), err["replyTo"].as_str()),
        (Some("bad_message"), Some("w4"))
    );
}

#[tokio::test]
async fn fault_read_error_sends_nfc_error_once() {
    let server = server().await;
    let mut ws = nfc_shim(&server).await;
    let (status, _) = send_json(
        &server,
        "PUT",
        "/api/v1/devices/nfc-1/faults/read-error",
        json!({ "active": true }),
    )
    .await;
    assert_eq!(status, 200);
    send_json(
        &server,
        "POST",
        "/api/v1/devices/nfc-1/tap",
        json!({ "uid": "04a2b3c4" }),
    )
    .await;
    assert_eq!(
        recv_type(&mut ws, "nfc-error").await,
        json!({ "v": 1, "type": "nfc-error", "deviceId": "nfc-1", "serialNumber": "04:a2:b3:c4", "records": [] })
    );
    send_json(
        &server,
        "POST",
        "/api/v1/devices/nfc-1/tap",
        json!({ "uid": "04a2b3c4" }),
    )
    .await;
    recv_type(&mut ws, "nfc-reading").await;
}

#[tokio::test]
async fn fault_interface_busy_refuses_the_claim_with_network() {
    let server = server().await;
    let mut ws = bridge(&server).await;
    let (status, _) = send_json(
        &server,
        "PUT",
        "/api/v1/devices/usb-printer-1/faults/interface-busy",
        json!({ "active": true }),
    )
    .await;
    assert_eq!(status, 200);
    send(
        &mut ws,
        json!({ "v": 1, "type": "open", "id": "o1", "deviceId": "usb-printer-1" }),
    )
    .await;
    let err = recv_type(&mut ws, "error").await;
    assert_eq!(
        (err["code"].as_str(), err["replyTo"].as_str()),
        (Some("network"), Some("o1"))
    );
    send_json(
        &server,
        "PUT",
        "/api/v1/devices/usb-printer-1/faults/interface-busy",
        json!({ "active": false }),
    )
    .await;
    send(
        &mut ws,
        json!({ "v": 1, "type": "open", "id": "o2", "deviceId": "usb-printer-1" }),
    )
    .await;
    assert_eq!(recv_type(&mut ws, "ack").await["replyTo"], "o2");
}
