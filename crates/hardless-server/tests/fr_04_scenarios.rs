//! FR-04: `POST /api/v1/scenarios/run`, and the M2 acceptance "a scripted 20-barcode
//! sequence arrives in order with the configured delay".
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

mod common;

use std::time::{Duration, Instant};

use common::{recv_type, send, send_json, server, ws};
use serde_json::{json, Value};

#[tokio::test]
async fn m2_a_scripted_20_barcode_sequence_arrives_in_order_with_the_delay() {
    let server = server().await;
    let mut shim = ws(&server, "/bridge").await;
    send(&mut shim, json!({ "v": 1, "type": "hello", "id": "h", "origin": "http://localhost", "shimVersion": "0.0.0", "apis": ["wedge"] })).await;
    recv_type(&mut shim, "ack").await;

    const DELAY_MS: u64 = 50;
    let mut steps = Vec::new();
    for i in 0..20 {
        if i > 0 {
            steps.push(json!({ "type": "wait", "ms": DELAY_MS }));
        }
        steps
            .push(json!({ "type": "scan", "device": "scanner-1", "code": format!("CODE-{i:02}") }));
    }
    steps.push(json!({ "type": "expect", "device": "scanner-1", "state": { "scans": 20, "lastCode": "CODE-19" } }));
    let body = json!({ "name": "20 scans", "steps": steps });
    let ((status, report), arrivals) = tokio::join!(
        send_json(&server, "POST", "/api/v1/scenarios/run", body),
        async {
            let mut arrivals = Vec::new();
            for _ in 0..20 {
                let frame = recv_type(&mut shim, "wedge-scan").await;
                arrivals.push((frame["code"].as_str().unwrap().to_owned(), Instant::now()));
            }
            arrivals
        }
    );
    let codes: Vec<&str> = arrivals.iter().map(|(c, _)| c.as_str()).collect();
    let expected: Vec<String> = (0..20).map(|i| format!("CODE-{i:02}")).collect();
    assert_eq!(codes, expected);
    for pair in arrivals.windows(2) {
        let gap = pair[1].1 - pair[0].1;
        // Allow for timer granularity on the receiving side; the server waits at least 50 ms.
        assert!(gap >= Duration::from_millis(DELAY_MS - 10), "gap {gap:?}");
    }

    assert_eq!(status, 200, "{report}");
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(report["name"], "20 scans");
    assert_eq!(report["steps"].as_array().unwrap().len(), 40);
    assert!(report["durationMs"].as_u64().unwrap() >= 19 * DELAY_MS);
}

#[tokio::test]
async fn fr_04_a_failing_scenario_reports_where_it_stopped() {
    let server = server().await;
    let (status, report) = send_json(
        &server,
        "POST",
        "/api/v1/scenarios/run",
        json!({ "steps": [
            { "type": "setFault", "device": "printer-1", "fault": "paper-out" },
            { "type": "expect", "device": "printer-1", "state": { "paperOut": false } },
            { "type": "clearFault", "device": "printer-1", "fault": "paper-out" }
        ] }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(report["ok"], false);
    let statuses: Vec<&Value> = report["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| &s["status"])
        .collect();
    assert_eq!(
        statuses,
        [&json!("passed"), &json!("failed"), &json!("skipped")]
    );
    assert!(report["steps"][1]["message"]
        .as_str()
        .unwrap()
        .contains("paperOut"));
}

#[tokio::test]
async fn fr_04_bad_and_oversized_scenarios_are_rejected() {
    let server = server().await;
    let (status, body) = send_json(
        &server,
        "POST",
        "/api/v1/scenarios/run",
        json!({ "steps": [{ "type": "moonwalk" }] }),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    let (status, body) = send_json(
        &server,
        "POST",
        "/api/v1/scenarios/run",
        json!({ "steps": [{ "type": "wait", "ms": 3_600_000 }] }),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "scenario_too_long");
}

#[tokio::test]
async fn fr_20_tap_reaches_the_device() {
    let server = server().await;
    let (status, body) = send_json(
        &server,
        "POST",
        "/api/v1/devices/scanner-1/tap",
        json!({ "uid": "04a2b3c4", "records": [] }),
    )
    .await;
    // A scanner is not an NFC reader: the tap is refused in its event log, not as an error.
    assert_eq!(status, 200, "{body}");
    let (status, _) = send_json(
        &server,
        "POST",
        "/api/v1/devices/scanner-1/tap",
        json!({ "records": [] }),
    )
    .await;
    assert_eq!(status, 400, "uid is required");
}
