//! The ESC/POS printer: jobs, status replies and config.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

mod common;

use common::{id, Rig, PRINTER};
use hardless_core::{DeviceEvent, DevicePlugin, LogLevel};
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
