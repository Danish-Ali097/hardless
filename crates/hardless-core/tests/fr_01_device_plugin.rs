//! FR-01: device plugin interface.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

mod common;

use common::{id, SampleDevice, SamplePlugin, KIND, STATUS_OK, STATUS_PAPER_OUT};
use hardless_core::{
    ControlCommand, Device, DeviceEvent, DevicePlugin, FaultId, FaultOptions, Millis,
};
use serde_json::json;

#[test]
fn fr_01_plugin_reports_kind_and_creates_device_of_that_kind() {
    let plugin = SamplePlugin;
    assert_eq!(plugin.kind(), KIND);
    let device = plugin.create(id("p1"), json!({})).unwrap();
    assert_eq!(device.kind(), KIND);
}

#[test]
fn fr_01_plugin_rejects_invalid_config_with_config_error() {
    let err = SamplePlugin
        .create(id("p1"), json!({ "idleMs": 0 }))
        .err()
        .expect("idleMs = 0 must be rejected");
    assert_eq!(err.pointer(), Some("/idleMs"));

    let err = SamplePlugin
        .create(id("p1"), json!({ "nope": true }))
        .err()
        .unwrap();
    assert!(err.message().contains("nope"), "{err}");
}

#[test]
fn fr_01_config_schema_is_a_json_schema_object() {
    let schema = SamplePlugin.config_schema();
    let value = serde_json::to_value(&schema).unwrap();
    assert_eq!(value["type"], "object");
    assert!(value["properties"]["idleMs"].is_object());
}

#[test]
fn fr_01_device_is_a_pure_state_machine_fed_bytes_and_time() {
    let mut device = SampleDevice::new(id("p1"), 300);

    // Host -> device bytes; `DLE EOT 1` gets a status reply.
    let events = device.on_host_bytes(b"Hello\n\x10\x04\x01", Millis(0));
    assert!(events
        .iter()
        .any(|e| matches!(e, DeviceEvent::ToHost(b) if b.as_ref() == [STATUS_OK])));
    assert_eq!(device.next_deadline(), Some(Millis(300)));

    // Nothing happens before the deadline; the job closes at it.
    assert!(device.on_tick(Millis(299)).is_empty());
    let events = device.on_tick(Millis(300));
    let job = events
        .iter()
        .find_map(|e| match e {
            DeviceEvent::JobCompleted(job) => Some(job),
            _ => None,
        })
        .expect("job completed");
    assert_eq!(job.text, "Hello\n");
    assert_eq!(job.at, Millis(300));
    assert_eq!(device.next_deadline(), None);
}

#[test]
fn fr_01_split_command_across_chunks_is_handled_by_the_device() {
    let mut device = SampleDevice::new(id("p1"), 300);
    assert!(!device
        .on_host_bytes(b"\x10", Millis(0))
        .iter()
        .any(|e| matches!(e, DeviceEvent::ToHost(_))));
    assert!(!device
        .on_host_bytes(b"\x04", Millis(1))
        .iter()
        .any(|e| matches!(e, DeviceEvent::ToHost(_))));
    assert!(device
        .on_host_bytes(b"\x01", Millis(2))
        .iter()
        .any(|e| matches!(e, DeviceEvent::ToHost(_))));
}

#[test]
fn fr_01_set_fault_changes_device_behaviour_and_reset_clears_it() {
    let mut device = SampleDevice::new(id("p1"), 300);
    let events = device.set_fault(FaultId::PAPER_OUT, true, FaultOptions::new(), Millis(0));
    assert!(matches!(events[..], [DeviceEvent::StateChanged]));
    let events = device.on_host_bytes(b"\x10\x04\x01", Millis(0));
    assert!(events
        .iter()
        .any(|e| matches!(e, DeviceEvent::ToHost(b) if b.as_ref() == [STATUS_PAPER_OUT])));
    assert_eq!(device.state()["paperOut"], true);

    device.reset(Millis(1));
    assert_eq!(device.state()["paperOut"], false);
}

#[test]
fn fr_01_set_fault_and_reset_report_their_events_directly() {
    let mut device = SampleDevice::new(id("p1"), 300);
    let events = device.set_fault(FaultId::OFFLINE, true, FaultOptions::new(), Millis(5));
    assert!(matches!(events[..], [DeviceEvent::Disconnected]));
    assert_eq!(
        device.next_deadline(),
        None,
        "no alarm needed to report the disconnect"
    );
    let events = device.reset(Millis(6));
    assert!(matches!(events[..], [DeviceEvent::Connected]));
}

#[test]
fn fr_01_control_commands_produce_connection_and_log_events() {
    let mut device = SampleDevice::new(id("p1"), 300);
    let unplug = ControlCommand::Custom {
        name: "unplug".into(),
        payload: json!(null),
    };
    assert!(matches!(
        device.on_control(unplug, Millis(0))[..],
        [DeviceEvent::Disconnected]
    ));
    let plug = ControlCommand::Custom {
        name: "plug".into(),
        payload: json!(null),
    };
    assert!(matches!(
        device.on_control(plug, Millis(0))[..],
        [DeviceEvent::Connected]
    ));
    assert!(matches!(
        device.on_control(ControlCommand::CloseDrawer, Millis(0))[..],
        [DeviceEvent::Log(_)]
    ));
}

#[test]
fn fr_01_plugins_and_devices_are_object_safe_and_thread_safe() {
    fn assert_send<T: Send + ?Sized>() {}
    fn assert_send_sync<T: Send + Sync + ?Sized>() {}
    assert_send::<dyn Device>();
    assert_send_sync::<dyn DevicePlugin>();
    let _boxed: Box<dyn DevicePlugin> = Box::new(SamplePlugin);
}

#[test]
fn fr_01_control_command_wire_format() {
    let scan: ControlCommand =
        serde_json::from_value(json!({ "type": "scan", "code": "8964000123456", "delayMs": 8 }))
            .unwrap();
    assert_eq!(
        scan,
        ControlCommand::Scan {
            code: "8964000123456".into(),
            terminator: None,
            delay_ms: Some(8)
        }
    );
    let close: ControlCommand = serde_json::from_value(json!({ "type": "close-drawer" })).unwrap();
    assert_eq!(close, ControlCommand::CloseDrawer);
}

#[test]
fn fr_01_plugin_and_device_declare_their_supported_faults() {
    let plugin_faults = SamplePlugin.faults();
    assert_eq!(plugin_faults, common::SUPPORTED_FAULTS);
    let device = SamplePlugin.create(id("p1"), json!({})).unwrap();
    // An instance supports a subset of what its kind can support.
    assert!(device.faults().iter().all(|f| plugin_faults.contains(f)));
    assert!(device.faults().contains(&FaultId::PAPER_OUT));
    assert!(!device.faults().contains(&FaultId::STALL));
}
