//! One test per M1 fault (PRD section 10) proving its observable effect on the device.
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
