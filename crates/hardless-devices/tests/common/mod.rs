//! A registry with the printer and drawer, driven by a fake clock.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, missing_docs)]

use hardless_core::{Bytes, DeviceEvent, DeviceId, FakeClock, FaultId, FaultOptions, Registry};
use serde_json::{json, Value};

pub const PRINTER: &str = "printer-1";
pub const DRAWER: &str = "drawer-1";

pub fn id(s: &str) -> DeviceId {
    s.parse().unwrap()
}

pub struct Rig {
    pub clock: FakeClock,
    pub registry: Registry,
}

impl Rig {
    /// A printer and a drawer on pin 2.
    pub fn new() -> Self {
        Self::with(json!({}), json!({ "attachedTo": PRINTER }))
    }

    pub fn with(printer: Value, drawer: Value) -> Self {
        let clock = FakeClock::new();
        let mut registry = Registry::new(clock.clone());
        hardless_devices::register_all(&mut registry).unwrap();
        registry
            .create("printer-escpos", Some(id(PRINTER)), printer)
            .unwrap();
        registry
            .create("cash-drawer", Some(id(DRAWER)), drawer)
            .unwrap();
        Self { clock, registry }
    }

    pub fn print(&mut self, bytes: &[u8]) -> Vec<DeviceEvent> {
        self.registry.host_bytes(&id(PRINTER), bytes).unwrap()
    }

    /// Sends `DLE EOT n` and returns the status byte.
    pub fn status(&mut self, n: u8) -> u8 {
        let events = self.print(&[0x10, 0x04, n]);
        let replies: Vec<&Bytes> = events
            .iter()
            .filter_map(|e| match e {
                DeviceEvent::ToHost(b) => Some(b),
                _ => None,
            })
            .collect();
        assert_eq!(replies.len(), 1, "one status byte expected: {events:?}");
        replies[0][0]
    }

    pub fn advance(&mut self, ms: u64) -> Vec<(DeviceId, DeviceEvent)> {
        self.clock.advance(ms);
        self.registry.tick()
    }

    pub fn fault(&mut self, device: &str, fault: FaultId, active: bool) -> Vec<DeviceEvent> {
        self.registry
            .set_fault(&id(device), fault, active, FaultOptions::new())
            .unwrap()
    }

    pub fn state(&self, device: &str) -> Value {
        self.registry.summary(&id(device)).unwrap().state
    }

    pub fn jobs(&self) -> Vec<String> {
        self.registry
            .jobs(&id(PRINTER))
            .unwrap()
            .map(|j| j.text.clone())
            .collect()
    }
}
