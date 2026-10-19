//! One test per printer and drawer fault (PRD section 10) proving its observable effect on the
//! device.
//! `slow-write` and `picker-cancel` are applied by the transport and the shim; here they are
//! accepted and reported in state, and their effects are tested in the server and shim.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

mod common;

use common::{id, Rig, PRINTER};
use hardless_core::{DeviceEvent, FaultId, FaultOptions, LogLevel, RegistryError};
use serde_json::json;

fn has_warning(events: &[DeviceEvent], text: &str) -> bool {
    events.iter().any(|e| {
        matches!(e, DeviceEvent::Log(entry) if entry.level == LogLevel::Warn && entry.message.contains(text))
    })
}

#[test]
fn fault_paper_out_rejects_prints_and_reports_paper_end() {
    let mut rig = Rig::new();
    rig.fault(PRINTER, FaultId::PAPER_OUT, true);
    let events = rig.print(b"Total: 1,250\n\x1dV\x00");
    assert!(has_warning(&events, "paper out"));
    rig.advance(1_000);
    assert!(rig.jobs().is_empty(), "nothing printed");

    assert_eq!(rig.status(1) & 0x08, 0x08, "DLE EOT 1: offline");
    assert_eq!(
        rig.status(2) & 0x20,
        0x20,
        "DLE EOT 2: stopped by paper end"
    );
    assert_eq!(rig.status(4) & 0x60, 0x60, "DLE EOT 4: paper end");
    assert_eq!(rig.state(PRINTER)["online"], false);

    // A buffered drawer kick is not executed while stopped; a real-time one is.
    rig.print(b"\x1bp\x00\x19\xfa");
    assert_eq!(rig.state(common::DRAWER)["kicks"], 0);
    rig.print(b"\x10\x14\x01\x00\x01");
    assert_eq!(rig.state(common::DRAWER)["kicks"], 1);

    rig.fault(PRINTER, FaultId::PAPER_OUT, false);
    assert_eq!(rig.status(4), 0x12);
    rig.print(b"again\n\x1dV\x00");
    assert_eq!(rig.jobs(), ["again\n"]);
}

#[test]
fn fault_cover_open_rejects_prints_and_reports_cover_open() {
    let mut rig = Rig::new();
    rig.fault(PRINTER, FaultId::COVER_OPEN, true);
    let events = rig.print(b"x\n\x1dV\x00");
    assert!(has_warning(&events, "cover open"));
    assert!(rig.jobs().is_empty());
    assert_eq!(rig.status(1) & 0x08, 0x08, "DLE EOT 1: offline");
    assert_eq!(rig.status(2) & 0x04, 0x04, "DLE EOT 2: cover open");
    assert_eq!(rig.status(4) & 0x60, 0, "paper is fine");
}

#[test]
fn fault_offline_disconnects_the_printer() {
    let mut rig = Rig::new();
    let events = rig.fault(PRINTER, FaultId::OFFLINE, true);
    assert!(events.contains(&DeviceEvent::Disconnected));
    let events = rig.print(b"lost\n");
    assert!(events.iter().all(|e| !matches!(e, DeviceEvent::ToHost(_))));
    assert!(has_warning(&events, "disconnected"));
    assert_eq!(rig.state(PRINTER)["connected"], false);

    let events = rig.fault(PRINTER, FaultId::OFFLINE, false);
    assert!(events.contains(&DeviceEvent::Connected));
}

#[test]
fn fault_disconnect_mid_write_disconnects_after_n_bytes_of_the_next_job() {
    let mut rig = Rig::new();
    let mut opts = FaultOptions::new();
    opts.insert("afterBytes".into(), json!(10));
    rig.registry
        .set_fault(&id(PRINTER), FaultId::DISCONNECT_MID_WRITE, true, opts)
        .unwrap();

    assert!(rig
        .print(b"12345")
        .iter()
        .all(|e| *e != DeviceEvent::Disconnected));
    let events = rig.print(b"6789012345\n\x1dV\x00");
    assert!(events.contains(&DeviceEvent::Disconnected));
    assert!(rig.jobs().is_empty(), "the interrupted job is lost");

    // It stays disconnected until the fault is cleared.
    rig.print(b"more");
    assert_eq!(rig.state(PRINTER)["connected"], false);
    let events = rig.fault(PRINTER, FaultId::DISCONNECT_MID_WRITE, false);
    assert!(events.contains(&DeviceEvent::Connected));
}

#[test]
fn fault_disconnect_mid_write_defaults_to_64_bytes() {
    let mut rig = Rig::new();
    rig.fault(PRINTER, FaultId::DISCONNECT_MID_WRITE, true);
    assert!(!rig.print(&[b'x'; 63]).contains(&DeviceEvent::Disconnected));
    assert!(rig.print(b"y").contains(&DeviceEvent::Disconnected));
}

#[test]
fn fault_slow_write_is_reported_for_the_transport() {
    let mut rig = Rig::new();
    rig.fault(PRINTER, FaultId::SLOW_WRITE, true);
    assert_eq!(rig.state(PRINTER)["slowWrite"], true);
    let summary = rig.registry.summary(&id(PRINTER)).unwrap();
    assert!(summary.faults.iter().any(|f| f.id == FaultId::SLOW_WRITE));
}

#[test]
fn fault_picker_cancel_is_reported_for_the_shim() {
    let mut rig = Rig::new();
    rig.fault(PRINTER, FaultId::PICKER_CANCEL, true);
    assert_eq!(rig.state(PRINTER)["pickerCancel"], true);
}

#[test]
fn faults_that_do_not_apply_are_rejected() {
    let mut rig = Rig::new();
    let err = rig
        .registry
        .set_fault(&id(PRINTER), FaultId::JAMMED, true, FaultOptions::new())
        .unwrap_err();
    assert!(matches!(err, RegistryError::UnsupportedFault { .. }));
    let err = rig
        .registry
        .set_fault(
            &id(common::DRAWER),
            FaultId::PAPER_OUT,
            true,
            FaultOptions::new(),
        )
        .unwrap_err();
    assert!(matches!(err, RegistryError::UnsupportedFault { .. }));
}

#[test]
fn printer_reset_clears_faults_and_reconnects() {
    let mut rig = Rig::new();
    rig.fault(PRINTER, FaultId::OFFLINE, true);
    rig.fault(PRINTER, FaultId::PAPER_OUT, true);
    let events = rig.registry.reset(&id(PRINTER)).unwrap();
    assert!(events.contains(&DeviceEvent::Connected));
    assert_eq!(rig.status(1), 0x12);
}

fn fault_with(rig: &mut Rig, fault: FaultId, options: serde_json::Value) -> Vec<DeviceEvent> {
    let options: FaultOptions = serde_json::from_value(options).unwrap();
    rig.registry
        .set_fault(&id(PRINTER), fault, true, options)
        .unwrap()
}

/// The raw bytes of the latest job, after an idle close.
fn last_job_bytes(rig: &mut Rig) -> Vec<u8> {
    rig.advance(300);
    rig.registry
        .last_job(&id(PRINTER))
        .unwrap()
        .expect("a job")
        .bytes
        .to_vec()
}

#[test]
fn fault_paper_near_end_prints_and_reports_near_end() {
    let mut rig = Rig::new();
    rig.fault(PRINTER, FaultId::PAPER_NEAR_END, true);
    rig.print(b"Total: 1,250\n\x1dV\x00");
    assert_eq!(rig.jobs(), ["Total: 1,250\n"], "prints still succeed");
    assert_eq!(rig.status(4) & 0x0c, 0x0c, "DLE EOT 4: near end");
    assert_eq!(rig.status(4) & 0x60, 0, "DLE EOT 4: not paper end");
    assert_eq!(rig.status(1) & 0x08, 0, "DLE EOT 1: still online");
    assert_eq!(rig.state(PRINTER)["paperNearEnd"], true);
}

#[test]
fn fault_baud_mismatch_scrambles_received_bytes() {
    let mut rig = Rig::new();
    rig.fault(PRINTER, FaultId::BAUD_MISMATCH, true);
    let events = rig.print(&[0x10, 0x04, 0x01]);
    assert!(
        !events.iter().any(|e| matches!(e, DeviceEvent::ToHost(_))),
        "a scrambled status request gets no reply: {events:?}"
    );
    rig.print(b"Hello\n");
    let received = last_job_bytes(&mut rig);
    assert!(!received.windows(5).any(|w| w == b"Hello"), "{received:?}");
    assert_eq!(rig.state(PRINTER)["baudMismatch"], true);
    // The event log still shows what the app sent.
    let sent = rig.registry.events(&id(PRINTER), None).unwrap();
    assert!(sent.iter().any(|r| matches!(
        &r.kind,
        hardless_core::EventKind::BytesIn { data } if data.as_ref() == b"Hello\n"
    )));

    rig.fault(PRINTER, FaultId::BAUD_MISMATCH, false);
    assert_eq!(rig.status(1), 0x12, "back to normal");
}

#[test]
fn fault_garbled_flips_bits_in_about_rate_of_the_bytes() {
    let mut rig = Rig::new();
    fault_with(&mut rig, FaultId::GARBLED, json!({ "rate": 0.25 }));
    // Single-bit flips of `a` are never control codes, so this stays one job.
    rig.print(&[b'a'; 8000]);
    let received = last_job_bytes(&mut rig);
    assert_eq!(received.len(), 8000);
    let corrupted = received.iter().filter(|&&b| b != b'a').count();
    assert!((1700..=2300).contains(&corrupted), "{corrupted} of 8000");
    assert!(received
        .iter()
        .all(|&b| b == b'a' || (b ^ b'a').count_ones() == 1));
    assert_eq!(rig.state(PRINTER)["garbleRate"], 0.25);
}

#[test]
fn fault_garbled_defaults_to_5_percent_and_is_repeatable() {
    let run = || {
        let mut rig = Rig::new();
        fault_with(&mut rig, FaultId::GARBLED, json!({}));
        rig.print(&[b'a'; 4000]);
        last_job_bytes(&mut rig)
    };
    let first = run();
    let corrupted = first.iter().filter(|&&b| b != b'a').count();
    assert!((120..=280).contains(&corrupted), "{corrupted} of 4000");
    assert_eq!(first, run(), "same clock, same corruption");
}

#[test]
fn fault_garbled_rate_is_clamped_with_a_warning() {
    let mut rig = Rig::new();
    let events = fault_with(&mut rig, FaultId::GARBLED, json!({ "rate": 2 }));
    assert!(has_warning(&events, "garbled rate must be 0 to 1"));
    rig.print(&[b'a'; 100]);
    assert!(last_job_bytes(&mut rig).iter().all(|&b| b != b'a'));
}
