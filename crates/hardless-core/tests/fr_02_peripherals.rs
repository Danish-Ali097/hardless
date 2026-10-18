//! FR-01/FR-02: peripheral signals between attached devices (printer <-> cash drawer).
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use hardless_core::{
    BusEvent, ConfigError, ControlCommand, Device, DeviceEvent, DeviceId, DevicePlugin, EventKind,
    FakeClock, FaultId, FaultOptions, Millis, PeripheralSignal, Registry,
};
use serde_json::{json, Value};

fn id(s: &str) -> DeviceId {
    s.parse().unwrap()
}

/// A host that kicks on `Custom { name: "kick" }` and remembers the sensor it last saw.
#[derive(Default)]
struct Host {
    drawer_open: bool,
}

/// A peripheral that opens on a pin-2 pulse and reports its sensor; `Custom "close"` closes it.
struct Drawer {
    host: Option<DeviceId>,
    open: bool,
}

/// Two devices attached to each other that answer every signal with another one.
struct PingPong {
    peer: DeviceId,
}

impl Device for Host {
    fn kind(&self) -> &'static str {
        "host"
    }
    fn state(&self) -> Value {
        json!({ "drawerOpen": self.drawer_open })
    }
    fn on_host_bytes(&mut self, _: &[u8], _: Millis) -> Vec<DeviceEvent> {
        vec![]
    }
    fn on_control(&mut self, cmd: ControlCommand, _: Millis) -> Vec<DeviceEvent> {
        match cmd {
            ControlCommand::Custom { name, .. } if name == "kick" => {
                vec![DeviceEvent::Peripheral(PeripheralSignal::Pulse {
                    pin: 2,
                    on_ms: 100,
                    off_ms: 200,
                })]
            }
            _ => vec![],
        }
    }
    fn on_tick(&mut self, _: Millis) -> Vec<DeviceEvent> {
        vec![]
    }
    fn next_deadline(&self) -> Option<Millis> {
        None
    }
    fn faults(&self) -> Vec<FaultId> {
        vec![]
    }
    fn set_fault(&mut self, _: FaultId, _: bool, _: FaultOptions, _: Millis) -> Vec<DeviceEvent> {
        vec![]
    }
    fn reset(&mut self, _: Millis) -> Vec<DeviceEvent> {
        vec![]
    }
    fn on_peripheral(
        &mut self,
        _: &DeviceId,
        signal: PeripheralSignal,
        _: Millis,
    ) -> Vec<DeviceEvent> {
        match signal {
            PeripheralSignal::Sensor { open } => {
                self.drawer_open = open;
                vec![DeviceEvent::StateChanged]
            }
            PeripheralSignal::Pulse { .. } => vec![],
        }
    }
}

impl Device for Drawer {
    fn kind(&self) -> &'static str {
        "drawer"
    }
    fn state(&self) -> Value {
        json!({ "open": self.open })
    }
    fn on_host_bytes(&mut self, _: &[u8], _: Millis) -> Vec<DeviceEvent> {
        vec![]
    }
    fn on_control(&mut self, cmd: ControlCommand, _: Millis) -> Vec<DeviceEvent> {
        match cmd {
            ControlCommand::CloseDrawer if self.open => {
                self.open = false;
                vec![
                    DeviceEvent::StateChanged,
                    DeviceEvent::Peripheral(PeripheralSignal::Sensor { open: false }),
                ]
            }
            _ => vec![],
        }
    }
    fn on_tick(&mut self, _: Millis) -> Vec<DeviceEvent> {
        vec![]
    }
    fn next_deadline(&self) -> Option<Millis> {
        None
    }
    fn faults(&self) -> Vec<FaultId> {
        vec![]
    }
    fn set_fault(&mut self, _: FaultId, _: bool, _: FaultOptions, _: Millis) -> Vec<DeviceEvent> {
        vec![]
    }
    fn reset(&mut self, _: Millis) -> Vec<DeviceEvent> {
        vec![]
    }
    fn attached_to(&self) -> Option<DeviceId> {
        self.host.clone()
    }
    fn on_peripheral(
        &mut self,
        _: &DeviceId,
        signal: PeripheralSignal,
        _: Millis,
    ) -> Vec<DeviceEvent> {
        match signal {
            PeripheralSignal::Pulse { pin: 2, .. } if !self.open => {
                self.open = true;
                vec![
                    DeviceEvent::StateChanged,
                    DeviceEvent::Peripheral(PeripheralSignal::Sensor { open: true }),
                ]
            }
            _ => vec![],
        }
    }
}

impl Device for PingPong {
    fn kind(&self) -> &'static str {
        "ping-pong"
    }
    fn state(&self) -> Value {
        json!({})
    }
    fn on_host_bytes(&mut self, _: &[u8], _: Millis) -> Vec<DeviceEvent> {
        vec![DeviceEvent::Peripheral(PeripheralSignal::Sensor {
            open: true,
        })]
    }
    fn on_control(&mut self, _: ControlCommand, _: Millis) -> Vec<DeviceEvent> {
        vec![]
    }
    fn on_tick(&mut self, _: Millis) -> Vec<DeviceEvent> {
        vec![]
    }
    fn next_deadline(&self) -> Option<Millis> {
        None
    }
    fn faults(&self) -> Vec<FaultId> {
        vec![]
    }
    fn set_fault(&mut self, _: FaultId, _: bool, _: FaultOptions, _: Millis) -> Vec<DeviceEvent> {
        vec![]
    }
    fn reset(&mut self, _: Millis) -> Vec<DeviceEvent> {
        vec![]
    }
    fn attached_to(&self) -> Option<DeviceId> {
        Some(self.peer.clone())
    }
    fn on_peripheral(
        &mut self,
        _: &DeviceId,
        signal: PeripheralSignal,
        _: Millis,
    ) -> Vec<DeviceEvent> {
        vec![DeviceEvent::Peripheral(signal)]
    }
}

/// One plugin for all three test devices, chosen by kind.
struct Plugin(&'static str);

impl DevicePlugin for Plugin {
    fn kind(&self) -> &'static str {
        self.0
    }
    fn config_schema(&self) -> schemars::Schema {
        schemars::json_schema!({ "type": "object" })
    }
    fn faults(&self) -> Vec<FaultId> {
        vec![]
    }
    fn create(&self, _: DeviceId, config: Value) -> Result<Box<dyn Device>, ConfigError> {
        let attached = config["attachedTo"].as_str().map(id);
        Ok(match self.0 {
            "host" => Box::new(Host::default()),
            "drawer" => Box::new(Drawer {
                host: attached,
                open: false,
            }),
            _ => Box::new(PingPong {
                peer: attached.ok_or_else(|| ConfigError::new("peer"))?,
            }),
        })
    }
}

fn registry() -> (FakeClock, Registry) {
    let clock = FakeClock::new();
    let mut registry = Registry::new(clock.clone());
    for kind in ["host", "drawer", "ping-pong"] {
        registry.register(Plugin(kind)).unwrap();
    }
    (clock, registry)
}

#[test]
fn fr_02_signals_travel_from_a_device_to_its_peripherals_and_back() {
    let (_, mut registry) = registry();
    // The drawer can be created before the device it is attached to.
    registry
        .create(
            "drawer",
            Some(id("drawer-1")),
            json!({ "attachedTo": "printer-1" }),
        )
        .unwrap();
    registry
        .create("host", Some(id("printer-1")), json!({}))
        .unwrap();

    let kick = ControlCommand::Custom {
        name: "kick".into(),
        payload: json!(null),
    };
    let events = registry.control(&id("printer-1"), kick).unwrap();
    assert!(matches!(
        events[..],
        [DeviceEvent::Peripheral(PeripheralSignal::Pulse {
            pin: 2,
            ..
        })]
    ));

    // The pulse reached the drawer, and the drawer's sensor came back to the printer.
    assert_eq!(
        registry.summary(&id("drawer-1")).unwrap().state["open"],
        true
    );
    assert_eq!(
        registry.summary(&id("printer-1")).unwrap().state["drawerOpen"],
        true
    );

    // Closing the drawer by hand updates the printer too.
    registry
        .control(&id("drawer-1"), ControlCommand::CloseDrawer)
        .unwrap();
    assert_eq!(
        registry.summary(&id("printer-1")).unwrap().state["drawerOpen"],
        false
    );
}

#[test]
fn fr_02_peripheral_signals_are_logged_on_both_devices_and_publish_state() {
    let (_, mut registry) = registry();
    registry
        .create("host", Some(id("printer-1")), json!({}))
        .unwrap();
    registry
        .create(
            "drawer",
            Some(id("drawer-1")),
            json!({ "attachedTo": "printer-1" }),
        )
        .unwrap();
    registry.drain_bus();

    let kick = ControlCommand::Custom {
        name: "kick".into(),
        payload: json!(null),
    };
    registry.control(&id("printer-1"), kick).unwrap();

    let printer_log = registry.events(&id("printer-1"), None).unwrap();
    let signals: Vec<_> = printer_log
        .iter()
        .filter_map(|r| match &r.kind {
            EventKind::Peripheral { from, to, signal } => {
                Some((from.as_str(), to.as_str(), signal))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        signals,
        [
            (
                "printer-1",
                "drawer-1",
                &PeripheralSignal::Pulse {
                    pin: 2,
                    on_ms: 100,
                    off_ms: 200
                }
            ),
            (
                "drawer-1",
                "printer-1",
                &PeripheralSignal::Sensor { open: true }
            ),
        ]
    );
    assert!(registry
        .events(&id("drawer-1"), None)
        .unwrap()
        .iter()
        .any(|r| r.kind.name() == "peripheral"));

    let states: Vec<_> = registry
        .drain_bus()
        .into_iter()
        .filter_map(|e| match e {
            BusEvent::State { device_id, .. } => Some(device_id),
            _ => None,
        })
        .collect();
    assert_eq!(states, [id("drawer-1"), id("printer-1")]);
}

#[test]
fn fr_02_summary_shows_what_a_device_is_attached_to() {
    let (_, mut registry) = registry();
    registry
        .create(
            "drawer",
            Some(id("drawer-1")),
            json!({ "attachedTo": "printer-1" }),
        )
        .unwrap();
    registry
        .create("host", Some(id("printer-1")), json!({}))
        .unwrap();
    assert_eq!(
        registry.summary(&id("drawer-1")).unwrap().attached_to,
        Some(id("printer-1"))
    );
    assert_eq!(
        registry.summary(&id("printer-1")).unwrap().attached_to,
        None
    );
    let wire = serde_json::to_value(registry.summary(&id("drawer-1")).unwrap()).unwrap();
    assert_eq!(wire["attachedTo"], "printer-1");
}

#[test]
fn fr_02_signals_to_a_missing_device_are_dropped() {
    let (_, mut registry) = registry();
    registry
        .create("host", Some(id("printer-1")), json!({}))
        .unwrap();
    let kick = ControlCommand::Custom {
        name: "kick".into(),
        payload: json!(null),
    };
    // No drawer attached: the pulse goes nowhere and nothing fails.
    registry.control(&id("printer-1"), kick).unwrap();
    assert!(!registry
        .events(&id("printer-1"), None)
        .unwrap()
        .iter()
        .any(|r| r.kind.name() == "peripheral"));
}

#[test]
fn fr_02_signal_loops_are_cut_off() {
    let (_, mut registry) = registry();
    registry
        .create("ping-pong", Some(id("a")), json!({ "attachedTo": "b" }))
        .unwrap();
    registry
        .create("ping-pong", Some(id("b")), json!({ "attachedTo": "a" }))
        .unwrap();
    registry.host_bytes(&id("a"), b"go").unwrap();
    let log = registry.events(&id("a"), None).unwrap();
    assert!(log
        .iter()
        .any(|r| matches!(&r.kind, EventKind::Log { message, .. } if message.contains("loop"))));
}
