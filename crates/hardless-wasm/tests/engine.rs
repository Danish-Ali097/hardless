//! FR-15: the standalone engine. These tests run natively (`cargo test`) and in WebAssembly
//! (`cargo test --target wasm32-unknown-unknown`, with wasm-bindgen-test-runner), proving the
//! devices behave the same in server and standalone modes (PRD section 15).
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use hardless_wasm::Engine;
use serde_json::{json, Value};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test as test;

fn engine() -> Engine {
    Engine::new(
        &json!([
            { "kind": "printer-escpos", "id": "printer-1" },
            { "kind": "cash-drawer", "id": "drawer-1", "config": { "attachedTo": "printer-1" } }
        ])
        .to_string(),
    )
    .unwrap()
}

fn parse(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

#[test]
fn fr_15_engine_creates_the_configured_devices() {
    let engine = engine();
    let devices = parse(&engine.devices().unwrap());
    let ids: Vec<&str> = devices
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["drawer-1", "printer-1"]);
}

#[test]
fn fr_15_status_requests_are_answered_like_on_the_server() {
    let mut engine = engine();
    let events = parse(
        &engine
            .host_bytes(0.0, "printer-1", b"\x10\x04\x01")
            .unwrap(),
    );
    assert_eq!(
        events,
        json!([{ "type": "data", "deviceId": "printer-1", "data": "Eg==" }])
    );
}

#[test]
fn fr_15_jobs_close_on_cut_and_on_idle_time() {
    let mut engine = engine();
    engine
        .host_bytes(0.0, "printer-1", b"one\n\x1dV\x00")
        .unwrap();
    engine.host_bytes(10.0, "printer-1", b"two\n").unwrap();
    assert_eq!(engine.next_deadline(), Some(310.0));
    engine.tick(309.0).unwrap();
    assert_eq!(
        parse(&engine.jobs("printer-1").unwrap())
            .as_array()
            .unwrap()
            .len(),
        1
    );
    engine.tick(310.0).unwrap();
    let jobs = parse(&engine.jobs("printer-1").unwrap());
    assert_eq!(jobs[0]["text"], "two\n");
    assert_eq!(jobs[1]["text"], "one\n");
}

#[test]
fn fr_15_drawer_opens_and_the_printer_reports_it() {
    let mut engine = engine();
    engine
        .host_bytes(0.0, "printer-1", b"\x1bp\x00\x19\xfa")
        .unwrap();
    engine.tick(50.0).unwrap();
    let events = parse(
        &engine
            .host_bytes(60.0, "printer-1", b"\x10\x04\x01")
            .unwrap(),
    );
    assert_eq!(events[0]["data"], "Fg==", "0x16: drawer open bit set");
    engine
        .control(70.0, "drawer-1", r#"{ "type": "close-drawer" }"#)
        .unwrap();
    let events = parse(
        &engine
            .host_bytes(80.0, "printer-1", b"\x10\x04\x01")
            .unwrap(),
    );
    assert_eq!(events[0]["data"], "Eg==");
}

#[test]
fn fr_15_faults_and_signals() {
    let mut engine = engine();
    let events = parse(
        &engine
            .set_fault(0.0, "printer-1", "offline", true, "")
            .unwrap(),
    );
    assert_eq!(
        events,
        json!([{ "type": "disconnect", "deviceId": "printer-1" }])
    );
    let events = parse(
        &engine
            .set_fault(1.0, "printer-1", "offline", false, "")
            .unwrap(),
    );
    assert_eq!(
        events,
        json!([{ "type": "connect", "deviceId": "printer-1" }])
    );

    engine
        .set_fault(
            2.0,
            "printer-1",
            "slow-write",
            true,
            r#"{ "msPerChunk": 50 }"#,
        )
        .unwrap();
    let devices = parse(&engine.devices().unwrap());
    assert_eq!(devices[1]["faults"][0]["options"]["msPerChunk"], 50);

    engine
        .host_signals(3.0, "printer-1", r#"{ "dataTerminalReady": true }"#)
        .unwrap();
    let log = parse(&engine.events("printer-1").unwrap());
    assert!(log
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["type"] == "signals"));
}

#[test]
fn fr_15_bus_events_are_available_for_the_shim() {
    let mut engine = engine();
    engine
        .host_bytes(0.0, "printer-1", b"hi\n\x1dV\x00")
        .unwrap();
    let bus = parse(&engine.drain_bus().unwrap());
    assert!(bus
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["type"] == "device:output"));
}

#[test]
fn fr_26_scanner_scans_reach_the_shim_like_on_the_server() {
    let mut engine =
        Engine::new(&json!([{ "kind": "scanner-wedge", "id": "scanner-1" }]).to_string()).unwrap();
    let scan = parse(
        &engine
            .control(0.0, "scanner-1", r#"{ "type": "scan", "code": "123" }"#)
            .unwrap(),
    );
    let expected = json!({
        "type": "wedge", "deviceId": "scanner-1", "code": "123", "delayMs": 8, "terminator": "enter"
    });
    assert_eq!(scan, json!([expected.clone()]));

    // double-scan: the repeat comes due after typing (3 gaps of 8 ms) plus the 30 ms gap.
    engine
        .set_fault(0.0, "scanner-1", "double-scan", true, "{}")
        .unwrap();
    engine
        .control(100.0, "scanner-1", r#"{ "type": "scan", "code": "123" }"#)
        .unwrap();
    assert_eq!(engine.next_deadline(), Some(154.0));
    assert_eq!(parse(&engine.tick(154.0).unwrap()), json!([expected]));
}

#[test]
fn fr_27_nfc_taps_and_writes_reach_the_shim_like_on_the_server() {
    let mut engine =
        Engine::new(&json!([{ "kind": "nfc-reader", "id": "nfc-1" }]).to_string()).unwrap();
    let write = r#"{ "type": "nfc-write", "records": [{ "recordType": "text", "data": "new" }] }"#;
    assert_eq!(
        parse(&engine.control(0.0, "nfc-1", write).unwrap()),
        json!([{ "type": "nfc-rejected", "deviceId": "nfc-1", "reason": "no-card", "message": "no card is on the reader" }])
    );
    let tap = r#"{ "type": "tap", "uid": "04a2b3c4", "records": [{ "recordType": "text", "data": "old" }] }"#;
    assert_eq!(
        parse(&engine.control(10.0, "nfc-1", tap).unwrap()),
        json!([{ "type": "nfc-reading", "deviceId": "nfc-1", "serialNumber": "04:a2:b3:c4",
                 "records": [{ "recordType": "text", "data": "old" }] }])
    );
    assert_eq!(
        parse(&engine.control(20.0, "nfc-1", write).unwrap()),
        json!([{ "type": "nfc-done", "deviceId": "nfc-1" }])
    );
    // The card leaves the reader after presenceMs (2000 ms by default).
    assert_eq!(engine.next_deadline(), Some(2010.0));

    engine
        .set_fault(3000.0, "nfc-1", "read-error", true, "")
        .unwrap();
    assert_eq!(
        parse(
            &engine
                .control(3000.0, "nfc-1", r#"{ "type": "tap", "uid": "04a2b3c4" }"#)
                .unwrap()
        ),
        json!([{ "type": "nfc-error", "deviceId": "nfc-1", "serialNumber": "04:a2:b3:c4", "records": [] }])
    );
    assert_eq!(
        parse(
            &engine
                .control(3100.0, "nfc-1", r#"{ "type": "tap", "uid": "04a2b3c4" }"#)
                .unwrap()
        )[0]["records"][0]["data"],
        "new"
    );
}
