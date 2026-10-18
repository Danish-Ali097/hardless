//! Cash drawer attached to a printer's drawer-kick connector (`cash-drawer`, FR-36 to FR-39).

use hardless_core::{
    ConfigError, ControlCommand, Device, DeviceEvent, DeviceId, DevicePlugin, FaultId,
    FaultOptions, LogEntry, LogLevel, Millis, PeripheralSignal,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The device kind.
pub const KIND: &str = "cash-drawer";

/// Shortest time a drawer takes to open, whatever the kick's on-time (FR-37).
pub const MIN_OPENING_MS: u64 = 50;

/// Drawer configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrawerConfig {
    /// The printer this drawer is plugged into.
    #[schemars(with = "String")]
    pub attached_to: DeviceId,
    /// The connector pin that kicks this drawer: 2 or 5.
    #[serde(default = "default_pin")]
    pub pin: u8,
    /// Close automatically this many milliseconds after opening. Off by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_close_ms: Option<u64>,
}

fn default_pin() -> u8 {
    2
}

/// Faults the drawer supports.
const FAULTS: [FaultId; 2] = [FaultId::JAMMED, FaultId::NOT_CONNECTED];

/// Creates [`Drawer`]s.
#[derive(Debug, Default)]
pub struct DrawerPlugin;

impl DevicePlugin for DrawerPlugin {
    fn kind(&self) -> &'static str {
        KIND
    }

    fn config_schema(&self) -> schemars::Schema {
        schemars::schema_for!(DrawerConfig)
    }

    fn faults(&self) -> Vec<FaultId> {
        FAULTS.to_vec()
    }

    fn create(&self, id: DeviceId, config: Value) -> Result<Box<dyn Device>, ConfigError> {
        let config: DrawerConfig =
            serde_json::from_value(config).map_err(|e| ConfigError::new(e.to_string()))?;
        if !matches!(config.pin, 2 | 5) {
            return Err(ConfigError::new("pin must be 2 or 5").at("/pin"));
        }
        if config.attached_to == id {
            return Err(ConfigError::new("a drawer cannot be attached to itself").at("/attachedTo"));
        }
        Ok(Box::new(Drawer::new(config)))
    }
}

/// Where the drawer is in its cycle (FR-37).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DrawerState {
    /// Closed.
    Closed,
    /// Kicked; opens when the pulse ends.
    Opening,
    /// Open.
    Open,
}

/// An emulated cash drawer.
#[derive(Debug)]
pub struct Drawer {
    config: DrawerConfig,
    state: DrawerState,
    kicks: u64,
    opened: u64,
    opening_until: Option<Millis>,
    close_at: Option<Millis>,
    jammed: bool,
    not_connected: bool,
}

impl Drawer {
    /// A closed drawer.
    pub fn new(config: DrawerConfig) -> Self {
        Self {
            config,
            state: DrawerState::Closed,
            kicks: 0,
            opened: 0,
            opening_until: None,
            close_at: None,
            jammed: false,
            not_connected: false,
        }
    }

    /// The current state.
    pub fn drawer_state(&self) -> DrawerState {
        self.state
    }

    /// What the printer's drawer-open sensor (pin 3) reads.
    fn sensor(&self) -> bool {
        self.state == DrawerState::Open && !self.not_connected
    }

    fn close(&mut self, events: &mut Vec<DeviceEvent>) {
        let was_sensing = self.sensor();
        self.state = DrawerState::Closed;
        self.opening_until = None;
        self.close_at = None;
        events.push(DeviceEvent::StateChanged);
        if was_sensing {
            events.push(DeviceEvent::Peripheral(PeripheralSignal::Sensor {
                open: false,
            }));
        }
    }
}

fn log(level: LogLevel, message: impl Into<String>) -> DeviceEvent {
    DeviceEvent::Log(LogEntry::new(level, message))
}

impl Device for Drawer {
    fn kind(&self) -> &'static str {
        KIND
    }

    fn state(&self) -> Value {
        json!({
            "state": self.state,
            "open": self.state == DrawerState::Open,
            "kicks": self.kicks,
            "opened": self.opened,
            "attachedTo": self.config.attached_to,
            "pin": self.config.pin,
            "autoCloseMs": self.config.auto_close_ms,
            "connected": !self.not_connected,
            "jammed": self.jammed,
        })
    }

    fn on_host_bytes(&mut self, _data: &[u8], _now: Millis) -> Vec<DeviceEvent> {
        vec![log(
            LogLevel::Warn,
            "a cash drawer takes no data; send the kick to its printer",
        )]
    }

    fn on_control(&mut self, cmd: ControlCommand, _now: Millis) -> Vec<DeviceEvent> {
        let mut events = Vec::new();
        match cmd {
            ControlCommand::CloseDrawer if self.state != DrawerState::Closed => {
                self.close(&mut events);
            }
            ControlCommand::CloseDrawer => events.push(log(LogLevel::Info, "already closed")),
            _ => events.push(log(LogLevel::Warn, "unsupported control command")),
        }
        events
    }

    fn on_tick(&mut self, now: Millis) -> Vec<DeviceEvent> {
        let mut events = Vec::new();
        if self.opening_until.is_some_and(|t| t <= now) {
            self.opening_until = None;
            self.state = DrawerState::Open;
            self.opened += 1;
            self.close_at = self.config.auto_close_ms.map(|ms| now + ms);
            events.push(DeviceEvent::StateChanged);
            if self.sensor() {
                events.push(DeviceEvent::Peripheral(PeripheralSignal::Sensor {
                    open: true,
                }));
            }
        }
        if self.state == DrawerState::Open && self.close_at.is_some_and(|t| t <= now) {
            self.close(&mut events);
        }
        events
    }

    fn next_deadline(&self) -> Option<Millis> {
        self.opening_until.or(self.close_at)
    }

    fn faults(&self) -> Vec<FaultId> {
        FAULTS.to_vec()
    }

    fn set_fault(
        &mut self,
        fault: FaultId,
        active: bool,
        _opts: FaultOptions,
        _now: Millis,
    ) -> Vec<DeviceEvent> {
        let mut events = vec![DeviceEvent::StateChanged];
        let sensed = self.sensor();
        if fault == FaultId::JAMMED {
            self.jammed = active;
        } else if fault == FaultId::NOT_CONNECTED {
            self.not_connected = active;
        }
        if self.sensor() != sensed {
            // Unplugging an open drawer reads as closed at the printer, and back.
            events.push(DeviceEvent::Peripheral(PeripheralSignal::Sensor {
                open: self.sensor(),
            }));
        }
        events
    }

    fn reset(&mut self, _now: Millis) -> Vec<DeviceEvent> {
        let mut events = Vec::new();
        let config = self.config.clone();
        let sensed = self.sensor();
        *self = Drawer::new(config);
        events.push(DeviceEvent::StateChanged);
        if sensed {
            events.push(DeviceEvent::Peripheral(PeripheralSignal::Sensor {
                open: false,
            }));
        }
        events
    }

    fn attached_to(&self) -> Option<DeviceId> {
        Some(self.config.attached_to.clone())
    }

    fn on_peripheral(
        &mut self,
        _from: &DeviceId,
        signal: PeripheralSignal,
        now: Millis,
    ) -> Vec<DeviceEvent> {
        let PeripheralSignal::Pulse { pin, on_ms, .. } = signal else {
            return Vec::new();
        };
        if pin != self.config.pin {
            return Vec::new();
        }
        if self.not_connected {
            return vec![log(LogLevel::Warn, "kick ignored: drawer not connected")];
        }
        self.kicks += 1;
        let mut events = vec![DeviceEvent::StateChanged];
        if self.jammed {
            events.push(log(
                LogLevel::Warn,
                "kick received but the drawer is jammed",
            ));
        } else if self.state == DrawerState::Closed {
            self.state = DrawerState::Opening;
            self.opening_until = Some(now + on_ms.max(MIN_OPENING_MS));
        }
        events
    }
}
