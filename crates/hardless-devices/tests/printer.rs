//! The ESC/POS printer: jobs, status replies and config.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

mod common;

use common::{id, Rig, PRINTER};
use hardless_core::{DeviceEvent, DevicePlugin, FaultId, LogLevel};
use hardless_devices::printer::PrinterPlugin;
use serde_json::json;

#[test]
fn printer_closes_a_job_on_cut_with_ast_text_and_bytes() {
    let mut rig = Rig::new();
    let events = rig.print(b"\x1b@Total: 1,250\n\x1dV\x00");
    assert!(events
        .iter()
        .any(|e| matches!(e, DeviceEvent::JobCompleted(_))));
    let job = rig
        .registry
        .last_job(&id(PRINTER))
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(job.text, "Total: 1,250\n");
    assert_eq!(job.ast[0]["type"], "init");
    assert_eq!(job.ast.as_array().unwrap().last().unwrap()["type"], "cut");
    assert_eq!(job.bytes.as_ref(), b"\x1b@Total: 1,250\n\x1dV\x00");
    assert_eq!(
        rig.registry.next_deadline(),
        None,
        "nothing pending after a cut"
    );
}

#[test]
fn printer_closes_a_job_after_300ms_of_silence_by_default() {
    let mut rig = Rig::new();
    rig.print(b"Hello");
    rig.advance(200);
    rig.print(b" world");
    assert!(
        rig.advance(299).is_empty(),
        "the idle timer restarts with each write"
    );
    rig.advance(1);
    assert_eq!(rig.jobs(), ["Hello world\n"]);
}

#[test]
fn printer_idle_time_is_configurable() {
    let mut rig = Rig::with(json!({ "idleMs": 50 }), json!({ "attachedTo": PRINTER }));
    rig.print(b"x\n");
    rig.advance(50);
    assert_eq!(rig.jobs().len(), 1);
}

#[test]
fn printer_numbers_jobs_and_keeps_them_newest_first() {
    let mut rig = Rig::new();
    rig.print(b"one\n\x1dV\x00");
    rig.print(b"two\n\x1dV\x00");
    let jobs: Vec<u64> = rig
        .registry
        .jobs(&id(PRINTER))
        .unwrap()
        .map(|j| j.id.0)
        .collect();
    assert_eq!(jobs, [2, 1]);
}

#[test]
fn printer_settings_without_printing_carry_into_the_next_job() {
    let mut rig = Rig::new();
    rig.print(b"\x1ba\x01");
    rig.advance(300);
    assert!(rig.jobs().is_empty());
    rig.print(b"centered\n\x1dV\x00");
    let job = rig
        .registry
        .last_job(&id(PRINTER))
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(job.ast[0]["type"], "align");
    assert!(job.text.starts_with("                    centered"));
}

#[test]
fn printer_status_polls_do_not_create_jobs_or_timers() {
    let mut rig = Rig::new();
    assert_eq!(rig.status(1), 0x12);
    assert_eq!(rig.registry.next_deadline(), None);
    rig.advance(1_000);
    assert!(rig.jobs().is_empty());
}

#[test]
fn printer_answers_each_status_request() {
    let mut rig = Rig::new();
    assert_eq!(rig.status(1), 0x12, "online, drawer closed");
    assert_eq!(rig.status(2), 0x12, "no offline cause");
    assert_eq!(rig.status(3), 0x12, "no error");
    assert_eq!(rig.status(4), 0x12, "paper present");
}

#[test]
fn printer_logs_unknown_commands_as_warnings() {
    let mut rig = Rig::new();
    let events = rig.print(b"\x1b3\x18x\n");
    assert!(events.iter().any(|e| matches!(
        e,
        DeviceEvent::Log(entry) if entry.level == LogLevel::Warn && entry.message.contains("ESC 3")
    )));
}

#[test]
fn printer_job_text_uses_the_configured_paper_width() {
    let mut rig = Rig::with(
        json!({ "paperWidth": 58 }),
        json!({ "attachedTo": PRINTER }),
    );
    rig.print(b"\x1ba\x02R\n\x1dV\x00");
    assert_eq!(rig.jobs(), [format!("{}R\n", " ".repeat(31))]);
}

#[test]
fn printer_config_is_validated() {
    let plugin = PrinterPlugin;
    assert!(plugin.create(id("p"), json!({ "idleMs": 0 })).is_err());
    assert!(plugin.create(id("p"), json!({ "paperWidth": 72 })).is_err());
    assert!(plugin
        .create(id("p"), json!({ "transport": "bluetooth" }))
        .is_err());
    assert!(plugin.create(id("p"), json!({ "nope": 1 })).is_err());
    assert!(plugin
        .create(id("p"), json!({ "drawerOpenLevel": "sideways" }))
        .is_err());
    let schema = serde_json::to_value(plugin.config_schema()).unwrap();
    assert!(schema["properties"]["usbVendorId"].is_object());
    assert!(schema["properties"]["drawerOpenLevel"].is_object());
}

#[test]
fn printer_state_reports_usb_ids_and_auto_select_for_the_shim() {
    let rig = Rig::with(
        json!({ "usbVendorId": 4617, "usbProductId": 1, "autoSelect": true }),
        json!({ "attachedTo": PRINTER }),
    );
    let state = rig.state(PRINTER);
    assert_eq!(state["usbVendorId"], 4617);
    assert_eq!(state["usbProductId"], 1);
    assert_eq!(state["autoSelect"], true);
    assert_eq!(state["transport"], "serial");
}

#[test]
fn fr_35_http_printers_have_http_faults() {
    let rig = Rig::with(
        json!({ "transport": "http" }),
        json!({ "attachedTo": PRINTER }),
    );
    let summary = rig.registry.summary(&id(PRINTER)).unwrap();
    let faults: Vec<&str> = summary
        .supported_faults
        .iter()
        .map(|f| f.as_str())
        .collect();
    assert!(faults.contains(&"timeout") && faults.contains(&"cors-blocked"));
    assert!(!faults.contains(&"baud-mismatch") && !faults.contains(&"slow-write"));
    assert_eq!(summary.state["transport"], "http");

    let serial = Rig::new();
    let faults = serial
        .registry
        .summary(&id(PRINTER))
        .unwrap()
        .supported_faults;
    assert!(!faults.iter().any(|f| f.as_str() == "timeout"));
}

#[test]
fn fr_31_close_job_ends_an_http_print_without_waiting() {
    let mut rig = Rig::with(
        json!({ "transport": "http" }),
        json!({ "attachedTo": PRINTER }),
    );
    rig.print(b"\x1b@Hello\n");
    let events = rig
        .registry
        .control(
            &id(PRINTER),
            hardless_core::ControlCommand::Custom {
                name: hardless_devices::printer::CLOSE_JOB.into(),
                payload: json!(null),
            },
        )
        .unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e, DeviceEvent::JobCompleted(_))));
    assert_eq!(rig.jobs(), ["Hello\n"]);
    assert_eq!(rig.registry.next_deadline(), None);
}

#[test]
fn fr_34_state_carries_the_dle_eot_status_bytes() {
    let mut rig = Rig::new();
    assert_eq!(
        rig.state(PRINTER)["statusBytes"],
        json!([0x12, 0x12, 0x12, 0x12])
    );
    rig.fault(PRINTER, hardless_core::FaultId::PAPER_OUT, true);
    assert_eq!(
        rig.state(PRINTER)["statusBytes"],
        json!([0x1A, 0x32, 0x12, 0x7E])
    );
}

#[test]
fn spec_8_2_usb_printers_report_descriptors_device_id_and_port_status() {
    let mut rig = Rig::with(
        json!({ "transport": "usb", "productName": "TM-Test" }),
        json!({ "attachedTo": PRINTER }),
    );
    let state = rig.state(PRINTER);
    assert_eq!(state["transport"], "usb");
    assert_eq!(state["usbVendorId"], 0x1209);
    assert_eq!(state["usbProductId"], 1);
    assert_eq!(state["usb"]["manufacturerName"], "Hardless");
    assert_eq!(state["usb"]["productName"], "TM-Test");
    assert_eq!(state["usb"]["serialNumber"], PRINTER);
    assert_eq!(
        state["usb"]["ieee1284Id"],
        "MFG:Hardless;MDL:TM-Test;CMD:ESC/POS;CLS:PRINTER;"
    );
    // Not error and selected.
    assert_eq!(state["usb"]["portStatus"], 0x18);
    rig.registry
        .set_fault(&id(PRINTER), FaultId::PAPER_OUT, true, Default::default())
        .unwrap();
    // Paper empty, offline, error.
    assert_eq!(rig.state(PRINTER)["usb"]["portStatus"], 0x20);

    // Printing works the same as over serial.
    rig.registry
        .set_fault(&id(PRINTER), FaultId::PAPER_OUT, false, Default::default())
        .unwrap();
    rig.print(b"USB\n\x1dV\x00");
    assert_eq!(rig.jobs(), ["USB\n"]);
}

#[test]
fn spec_8_2_usb_printers_have_usb_faults() {
    let rig = Rig::with(
        json!({ "transport": "usb" }),
        json!({ "attachedTo": PRINTER }),
    );
    let summary = rig.registry.summary(&id(PRINTER)).unwrap();
    let faults: Vec<&str> = summary
        .supported_faults
        .iter()
        .map(|f| f.as_str())
        .collect();
    for fault in [
        "stall",
        "interface-busy",
        "slow-write",
        "disconnect-mid-write",
        "picker-cancel",
    ] {
        assert!(faults.contains(&fault), "{fault}");
    }
    assert!(!faults.contains(&"baud-mismatch") && !faults.contains(&"timeout"));
    let serial = Rig::new();
    let serial_faults = serial
        .registry
        .summary(&id(PRINTER))
        .unwrap()
        .supported_faults;
    assert!(!serial_faults.contains(&FaultId::STALL));
    assert!(PrinterPlugin
        .create(id("p"), json!({ "transport": "usb", "usbProductId": 5 }))
        .is_err());
}

#[test]
fn faults_stall_and_interface_busy_show_in_state_for_the_shim() {
    let mut rig = Rig::with(
        json!({ "transport": "usb" }),
        json!({ "attachedTo": PRINTER }),
    );
    for fault in [FaultId::STALL, FaultId::INTERFACE_BUSY] {
        rig.registry
            .set_fault(&id(PRINTER), fault, true, Default::default())
            .unwrap();
    }
    let state = rig.state(PRINTER);
    assert_eq!(
        (state["stall"].as_bool(), state["interfaceBusy"].as_bool()),
        (Some(true), Some(true))
    );
}
