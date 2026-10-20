//! FR-20: the REST control API (PRD section 11.1).
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

mod common;

use common::{get_json, http, send_json, server};
use hardless_core::FaultId;
use serde_json::json;

#[tokio::test]
async fn fr_20_health() {
    let server = server().await;
    let (status, body) = get_json(&server, "/api/v1/health").await;
    assert_eq!(status, 200);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn fr_20_default_devices_are_two_printers_a_drawer_a_scanner_and_an_nfc_reader() {
    let server = server().await;
    let (status, body) = get_json(&server, "/api/v1/devices").await;
    assert_eq!(status, 200);
    let ids: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "drawer-1",
            "local_printer",
            "nfc-1",
            "printer-1",
            "scanner-1"
        ]
    );
    assert_eq!(body[0]["attachedTo"], "printer-1");
    assert!(body[1]["supportedFaults"]
        .as_array()
        .unwrap()
        .contains(&json!("paper-out")));
}

#[tokio::test]
async fn fr_20_create_get_and_delete_a_device() {
    let server = server().await;
    let (status, created) = send_json(
        &server,
        "POST",
        "/api/v1/devices",
        json!({ "kind": "printer-escpos", "id": "kitchen", "config": { "paperWidth": 58 } }),
    )
    .await;
    assert_eq!(status, 201);
    assert_eq!(created["id"], "kitchen");
    assert_eq!(created["state"]["paperWidth"], 58);

    let (status, again) = send_json(
        &server,
        "POST",
        "/api/v1/devices",
        json!({ "kind": "printer-escpos", "id": "kitchen" }),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(again["error"]["code"], "duplicate_id");

    let (status, _) = get_json(&server, "/api/v1/devices/kitchen").await;
    assert_eq!(status, 200);
    let (status, _, _) = http(&server, "DELETE", "/api/v1/devices/kitchen", None, &[]).await;
    assert_eq!(status, 204);
    let (status, missing) = get_json(&server, "/api/v1/devices/kitchen").await;
    assert_eq!(status, 404);
    assert_eq!(missing["error"]["code"], "not_found");
}

#[tokio::test]
async fn fr_20_errors_use_the_error_envelope() {
    let server = server().await;
    let (status, body) = send_json(
        &server,
        "POST",
        "/api/v1/devices",
        json!({ "kind": "laser" }),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "unknown_kind");
    assert!(body["error"]["message"].as_str().unwrap().contains("laser"));

    let (status, body) = send_json(
        &server,
        "POST",
        "/api/v1/devices",
        json!({ "kind": "printer-escpos", "config": { "idleMs": 0 } }),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "invalid_config");

    let (status, body) = send_json(&server, "POST", "/api/v1/devices", json!({ "nope": 1 })).await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "bad_request");
}

#[tokio::test]
async fn fr_20_jobs_last_job_and_render() {
    let server = server().await;
    let printer = "printer-1".parse().unwrap();
    server
        .engine
        .drive(&printer, |r| {
            r.host_bytes(&printer, b"\x1ba\x01Total: 1,250\n\x1dV\x00")
        })
        .unwrap();

    let (status, jobs) = get_json(&server, "/api/v1/devices/printer-1/jobs?limit=10").await;
    assert_eq!(status, 200);
    assert_eq!(jobs.as_array().unwrap().len(), 1);
    let job = &jobs[0];
    for field in ["id", "at", "ast", "text", "bytesHex"] {
        assert!(!job[field].is_null(), "job has {field}");
    }

    let (status, last) = get_json(&server, "/api/v1/devices/printer-1/jobs/last").await;
    assert_eq!(status, 200);
    assert!(last["text"].as_str().unwrap().contains("Total: 1,250"));

    let (status, headers, html) = http(
        &server,
        "GET",
        "/api/v1/devices/printer-1/jobs/1/render?format=html&width=58",
        None,
        &[],
    )
    .await;
    assert_eq!(status, 200);
    assert!(headers
        .iter()
        .any(|(k, v)| k == "content-type" && v.starts_with("text/html")));
    assert!(html.contains("hl-paper-58") && html.contains("Total: 1,250"));

    let content_type = |headers: &[(String, String)]| {
        headers
            .iter()
            .find(|(k, _)| k == "content-type")
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let (status, headers, svg) = http(
        &server,
        "GET",
        "/api/v1/devices/printer-1/jobs/1/render?format=svg",
        None,
        &[],
    )
    .await;
    assert_eq!(status, 200, "FR-11 SVG: {svg}");
    assert!(content_type(&headers).starts_with("image/svg+xml"));
    assert!(svg.starts_with("<svg") && svg.contains("Total: 1,250"));

    let (status, headers, png) = http(
        &server,
        "GET",
        "/api/v1/devices/printer-1/jobs/1/render?format=png&width=58",
        None,
        &[],
    )
    .await;
    assert_eq!(status, 200, "FR-11 PNG");
    assert_eq!(content_type(&headers), "image/png");
    assert!(png.contains("PNG"), "PNG signature");

    let (status, body) = get_json(
        &server,
        "/api/v1/devices/printer-1/jobs/1/render?format=pdf",
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error"]["code"], "bad_format");
    let (status, _) = get_json(&server, "/api/v1/devices/printer-1/jobs/99/render").await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn fr_20_last_job_is_404_before_anything_prints() {
    let server = server().await;
    let (status, body) = get_json(&server, "/api/v1/devices/printer-1/jobs/last").await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "no_jobs");
}

#[tokio::test]
async fn fr_20_faults_toggle_and_reject_what_does_not_apply() {
    let server = server().await;
    let (status, body) = send_json(
        &server,
        "PUT",
        "/api/v1/devices/printer-1/faults/paper-out",
        json!({ "active": true }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["state"]["paperOut"], true);
    assert_eq!(body["faults"][0]["id"], "paper-out");

    let (status, body) = send_json(
        &server,
        "PUT",
        "/api/v1/devices/printer-1/faults/disconnect-mid-write",
        json!({ "active": true, "options": { "afterBytes": 8 } }),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body["faults"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["options"]["afterBytes"] == 8));

    let (status, body) = send_json(
        &server,
        "PUT",
        "/api/v1/devices/printer-1/faults/jammed",
        json!({ "active": true }),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "unsupported_fault");

    let (status, body) = send_json(
        &server,
        "PUT",
        "/api/v1/devices/printer-1/faults/Not_Valid",
        json!({ "active": true }),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "invalid_fault");
    let _ = FaultId::PAPER_OUT;
}

#[tokio::test]
async fn fr_20_reset_clears_faults() {
    let server = server().await;
    send_json(
        &server,
        "PUT",
        "/api/v1/devices/printer-1/faults/cover-open",
        json!({ "active": true }),
    )
    .await;
    let (status, body) = send_json(
        &server,
        "POST",
        "/api/v1/devices/printer-1/reset",
        json!({}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["faults"], json!([]));
    assert_eq!(body["state"]["coverOpen"], false);
}

#[tokio::test]
async fn fr_20_close_drawer_endpoint() {
    let server = server().await;
    let printer = "printer-1".parse().unwrap();
    server
        .engine
        .drive(&printer, |r| r.host_bytes(&printer, b"\x1bp\x00\x19\xfa"))
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let (_, drawer) = get_json(&server, "/api/v1/devices/drawer-1").await;
    assert_eq!(drawer["state"]["state"], "open", "the timer task opened it");

    let (status, drawer) =
        send_json(&server, "POST", "/api/v1/devices/drawer-1/close", json!({})).await;
    assert_eq!(status, 200);
    assert_eq!(drawer["state"]["state"], "closed");
}

#[tokio::test]
async fn fr_20_events_since() {
    let server = server().await;
    let printer = "printer-1".parse().unwrap();
    server
        .engine
        .drive(&printer, |r| r.host_bytes(&printer, b"\x10\x04\x01"))
        .unwrap();
    let (status, events) = get_json(&server, "/api/v1/devices/printer-1/events").await;
    assert_eq!(status, 200);
    let events = events.as_array().unwrap();
    assert!(events
        .iter()
        .any(|e| e["type"] == "bytes-in" && e["dataHex"] == "100401"));
    assert!(events
        .iter()
        .any(|e| e["type"] == "bytes-out" && e["dataHex"] == "12"));
    let last = events.last().unwrap()["seq"].as_u64().unwrap();
    let (_, newer) = get_json(
        &server,
        &format!("/api/v1/devices/printer-1/events?since={last}"),
    )
    .await;
    assert_eq!(newer, json!([]));
}

#[tokio::test]
async fn fr_20_cors_allows_local_origins_only_without_a_token() {
    let server = server().await;
    let (status, headers, _) = http(
        &server,
        "OPTIONS",
        "/api/v1/devices/printer-1/faults/picker-cancel",
        None,
        &[
            ("Origin", "http://localhost:5173"),
            ("Access-Control-Request-Method", "PUT"),
        ],
    )
    .await;
    assert_eq!(status, 204);
    assert!(headers
        .iter()
        .any(|(k, v)| k == "access-control-allow-origin" && v == "http://localhost:5173"));

    let (_, headers, _) = http(
        &server,
        "GET",
        "/api/v1/devices",
        None,
        &[("Origin", "https://evil.example")],
    )
    .await;
    assert!(!headers
        .iter()
        .any(|(k, _)| k == "access-control-allow-origin"));
}

#[tokio::test]
async fn fr_20_render_can_hide_unknown_command_badges() {
    let server = server().await;
    let printer = "printer-1".parse().unwrap();
    server
        .engine
        .drive(&printer, |r| {
            r.host_bytes(&printer, b"a\x1b3\x18b\n\x1dV\x00")
        })
        .unwrap();
    let (_, _, with) = http(
        &server,
        "GET",
        "/api/v1/devices/printer-1/jobs/1/render",
        None,
        &[],
    )
    .await;
    assert!(with.contains(r#"class="hl-unknown""#));
    let (_, _, without) = http(
        &server,
        "GET",
        "/api/v1/devices/printer-1/jobs/1/render?unknown=0",
        None,
        &[],
    )
    .await;
    assert!(!without.contains(r#"class="hl-unknown""#));
}

#[tokio::test]
async fn fr_26_scan_endpoint_scans_and_validates_its_body() {
    let server = server().await;
    let (status, body) = send_json(
        &server,
        "POST",
        "/api/v1/devices/scanner-1/scan",
        json!({ "code": "ABC-1", "terminator": "tab", "delayMs": 0 }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["state"]["scans"], 1);
    assert_eq!(body["state"]["lastCode"], "ABC-1");

    let (status, events) = get_json(&server, "/api/v1/devices/scanner-1/events").await;
    assert_eq!(status, 200);
    let wedge = events
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "wedge")
        .expect("the scan is in the event log");
    assert_eq!(
        wedge["scan"],
        json!({ "code": "ABC-1", "delayMs": 0, "terminator": "tab" })
    );

    let (status, body) = send_json(
        &server,
        "POST",
        "/api/v1/devices/scanner-1/scan",
        json!({ "code": "1", "terminator": "space" }),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error"]["code"], "bad_request");
    let (status, _) = send_json(
        &server,
        "POST",
        "/api/v1/devices/nope/scan",
        json!({ "code": "1" }),
    )
    .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn fr_20_tap_rejects_a_bad_uid_or_bad_records() {
    let server = server().await;
    let (status, body) = send_json(
        &server,
        "POST",
        "/api/v1/devices/nfc-1/tap",
        json!({ "uid": "nope" }),
    )
    .await;
    assert_eq!(
        (status, body["error"]["code"].as_str()),
        (400, Some("invalid_uid"))
    );
    let (status, body) = send_json(
        &server,
        "POST",
        "/api/v1/devices/nfc-1/tap",
        json!({ "uid": "04a2b3c4", "records": [{ "recordType": "url" }] }),
    )
    .await;
    assert_eq!(
        (status, body["error"]["code"].as_str()),
        (400, Some("invalid_records"))
    );
    let (status, body) = send_json(
        &server,
        "POST",
        "/api/v1/devices/nfc-1/tap",
        json!({ "uid": "04a2b3c4", "records": [{ "recordType": "text", "data": "ok" }] }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["state"]["taps"], 1);
}
