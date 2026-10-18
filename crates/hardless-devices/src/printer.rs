//! ESC/POS receipt printer (`printer-escpos`).
//!
//! Status bytes follow the `DLE EOT` section of the public ESC/POS command reference.

use hardless_core::{
    Bytes, ConfigError, ControlCommand, Device, DeviceEvent, DeviceId, DevicePlugin, FaultId,
    FaultOptions, Job, JobId, LogEntry, LogLevel, Millis, PeripheralSignal,
};
use hardless_escpos::{Node, Parser, StatusKind};
use hardless_render::{to_text_with, PaperWidth, RenderOptions};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The device kind.
pub const KIND: &str = "printer-escpos";

/// Default `afterBytes` for `disconnect-mid-write` (PRD section 10).
const DEFAULT_AFTER_BYTES: u64 = 64;

/// How an app reaches the printer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// Web Serial (and, from phase 2, virtual serial ports).
    #[default]
    Serial,
}

/// Printer configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrinterConfig {
    /// How the app reaches the printer. Only `serial` in milestone M1.
    #[serde(default)]
    pub transport: Transport,
    /// Paper width in millimetres: 58 or 80.
    #[serde(default)]
    #[schemars(with = "u16")]
    pub paper_width: PaperWidth,
    /// Silence in milliseconds after which an open job closes.
    #[serde(default = "default_idle_ms")]
    pub idle_ms: u64,
    /// USB vendor ID reported by `SerialPort.getInfo()`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usb_vendor_id: Option<u16>,
    /// USB product ID reported by `SerialPort.getInfo()`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usb_product_id: Option<u16>,
    /// Let the shim's port picker choose this device without asking.
    #[serde(default)]
    pub auto_select: bool,
}

fn default_idle_ms() -> u64 {
    300
}

impl Default for PrinterConfig {
    fn default() -> Self {
        Self {
            transport: Transport::Serial,
            paper_width: PaperWidth::Mm80,
            idle_ms: default_idle_ms(),
            usb_vendor_id: None,
            usb_product_id: None,
            auto_select: false,
        }
    }
}

/// Faults the printer supports in milestone M1.
const FAULTS: [FaultId; 6] = [
    FaultId::PAPER_OUT,
    FaultId::COVER_OPEN,
    FaultId::OFFLINE,
    FaultId::DISCONNECT_MID_WRITE,
    FaultId::SLOW_WRITE,
    FaultId::PICKER_CANCEL,
];

/// Creates [`Printer`]s.
#[derive(Debug, Default)]
pub struct PrinterPlugin;

impl DevicePlugin for PrinterPlugin {
    fn kind(&self) -> &'static str {
        KIND
    }

    fn config_schema(&self) -> schemars::Schema {
        schemars::schema_for!(PrinterConfig)
    }

    fn faults(&self) -> Vec<FaultId> {
        FAULTS.to_vec()
    }

    fn create(&self, id: DeviceId, config: Value) -> Result<Box<dyn Device>, ConfigError> {
        let config = if config.is_null() {
            PrinterConfig::default()
        } else {
            serde_json::from_value::<PrinterConfig>(config)
                .map_err(|e| ConfigError::new(e.to_string()))?
        };
        if config.idle_ms == 0 {
            return Err(ConfigError::new("idleMs must be at least 1").at("/idleMs"));
        }
        Ok(Box::new(Printer::new(id, config)))
    }
}

/// An emulated ESC/POS receipt printer.
#[derive(Debug)]
pub struct Printer {
    id: DeviceId,
    config: PrinterConfig,
    parser: Parser,
    /// Commands of the job being received.
    job_nodes: Vec<Node>,
    /// Raw bytes of the job being received.
    job_bytes: Vec<u8>,
    /// When the open job closes if nothing else arrives.
    idle_deadline: Option<Millis>,
    jobs: u64,
    rejected: u64,
    connected: bool,
    paper_out: bool,
    cover_open: bool,
    drawer_open: bool,
    /// Whether the rejection of the current print has been logged.
    rejection_logged: bool,
    /// The time of the latest call, used to stamp jobs.
    last_now: Millis,
    /// `disconnect-mid-write`: bytes left before the disconnect, while armed.
    disconnect_after: Option<u64>,
    /// Whether the printer is disconnected because `disconnect-mid-write` fired.
    disconnected_mid_write: bool,
    offline_fault: bool,
    slow_write: bool,
    picker_cancel: bool,
}

impl Printer {
    /// A printer in its power-on state.
    pub fn new(id: DeviceId, config: PrinterConfig) -> Self {
        Self {
            id,
            config,
            parser: Parser::new(),
            job_nodes: Vec::new(),
            job_bytes: Vec::new(),
            idle_deadline: None,
            jobs: 0,
            rejected: 0,
            connected: true,
            paper_out: false,
            cover_open: false,
            drawer_open: false,
            rejection_logged: false,
            last_now: Millis::ZERO,
            disconnect_after: None,
            disconnected_mid_write: false,
            offline_fault: false,
            slow_write: false,
            picker_cancel: false,
        }
    }

    /// Paper out or cover open: the printer stops printing.
    fn stopped(&self) -> bool {
        self.paper_out || self.cover_open
    }

    /// The `DLE EOT n` status byte.
    pub fn status_byte(&self, kind: StatusKind) -> u8 {
        // Bits 1 and 4 are fixed to 1 in every status byte.
        let mut byte = 0x12;
        match kind {
            StatusKind::Printer => {
                if self.drawer_open {
                    byte |= 0x04;
                }
                if self.stopped() {
                    byte |= 0x08;
                }
            }
            StatusKind::OfflineCause => {
                if self.cover_open {
                    byte |= 0x04;
                }
                if self.paper_out {
                    byte |= 0x20;
                }
            }
            StatusKind::ErrorCause => {}
            StatusKind::PaperSensor => {
                if self.paper_out {
                    // An empty roll trips the near-end sensor as well as the end sensor.
                    byte |= 0x0c | 0x60;
                }
            }
        }
        byte
    }

    fn handle(&mut self, node: Node, events: &mut Vec<DeviceEvent>) {
        match node {
            Node::Status { kind } => {
                events.push(DeviceEvent::ToHost(Bytes::from(vec![
                    self.status_byte(kind)
                ])));
            }
            Node::Pulse {
                pin,
                on_ms,
                off_ms,
                realtime,
            } => {
                if self.stopped() && !realtime {
                    events.push(warn(
                        "drawer kick ignored: the printer is stopped (paper out or cover open)",
                    ));
                } else {
                    events.push(DeviceEvent::Peripheral(PeripheralSignal::Pulse {
                        pin,
                        on_ms,
                        off_ms,
                    }));
                }
                if !realtime {
                    self.job_nodes.push(Node::Pulse {
                        pin,
                        on_ms,
                        off_ms,
                        realtime,
                    });
                }
            }
            node if node.is_printing() && self.stopped() => {
                // Prints are rejected; the app finds out by asking for status.
                if !self.rejection_logged {
                    self.rejection_logged = true;
                    self.rejected += 1;
                    events.push(warn(if self.paper_out {
                        "print rejected: paper out"
                    } else {
                        "print rejected: cover open"
                    }));
                }
                self.job_nodes.clear();
            }
            Node::Unknown { ref warning, .. } => {
                events.push(warn(warning.clone()));
                self.job_nodes.push(node);
            }
            Node::Cut { .. } => {
                self.job_nodes.push(node);
                self.close_job(events);
            }
            node => self.job_nodes.push(node),
        }
    }

    /// Closes the open job; a job with nothing printable carries over into the next one.
    fn close_job(&mut self, events: &mut Vec<DeviceEvent>) {
        self.idle_deadline = None;
        self.rejection_logged = false;
        if !self.job_nodes.iter().any(Node::is_printing) {
            // Settings carry over into the next job; a drawer kick on its own does not.
            self.job_nodes
                .retain(|node| !matches!(node, Node::Pulse { .. }));
            return;
        }
        let nodes = std::mem::take(&mut self.job_nodes);
        let bytes = std::mem::take(&mut self.job_bytes);
        let options = RenderOptions {
            paper: self.config.paper_width,
            show_unknown: true,
        };
        self.jobs += 1;
        events.push(DeviceEvent::JobCompleted(Job {
            id: JobId(self.jobs),
            at: self.last_now,
            ast: serde_json::to_value(&nodes).unwrap_or(Value::Null),
            text: to_text_with(&nodes, &options),
            bytes: Bytes::from(bytes),
        }));
        events.push(DeviceEvent::StateChanged);
    }

    fn disconnect(&mut self, events: &mut Vec<DeviceEvent>) {
        if self.connected {
            self.connected = false;
            self.abandon_job();
            events.push(DeviceEvent::Disconnected);
        }
    }

    fn connect(&mut self, events: &mut Vec<DeviceEvent>) {
        if !self.connected && !self.offline_fault && !self.disconnected_mid_write {
            self.connected = true;
            events.push(DeviceEvent::Connected);
        }
    }

    /// Drops a half-received job, e.g. when the cable is pulled.
    fn abandon_job(&mut self) {
        self.parser = Parser::new();
        self.job_nodes.clear();
        self.job_bytes.clear();
        self.idle_deadline = None;
    }
}

fn warn(message: impl Into<String>) -> DeviceEvent {
    DeviceEvent::Log(LogEntry::new(LogLevel::Warn, message))
}

impl Device for Printer {
    fn kind(&self) -> &'static str {
        KIND
    }

    fn state(&self) -> Value {
        json!({
            "transport": self.config.transport,
            "paperWidth": self.config.paper_width,
            "usbVendorId": self.config.usb_vendor_id,
            "usbProductId": self.config.usb_product_id,
            "autoSelect": self.config.auto_select,
            "connected": self.connected,
            "online": self.connected && !self.stopped(),
            "paperOut": self.paper_out,
            "coverOpen": self.cover_open,
            "drawerOpen": self.drawer_open,
            "jobs": self.jobs,
            "rejectedPrints": self.rejected,
            "receiving": !self.job_bytes.is_empty(),
            "slowWrite": self.slow_write,
            "pickerCancel": self.picker_cancel,
        })
    }

    fn on_host_bytes(&mut self, data: &[u8], now: Millis) -> Vec<DeviceEvent> {
        self.last_now = now;
        let mut events = Vec::new();
        if !self.connected {
            events.push(warn("bytes received while disconnected were dropped"));
            return events;
        }
        let mut data = data;
        let mut disconnect_now = false;
        if let Some(left) = self.disconnect_after {
            let left_usize = usize::try_from(left).unwrap_or(usize::MAX);
            if data.len() >= left_usize {
                data = &data[..left_usize];
                disconnect_now = true;
            } else {
                self.disconnect_after = Some(left - data.len() as u64);
            }
        }
        let jobs_before = self.jobs;
        self.job_bytes.extend_from_slice(data);
        for node in self.parser.push(data) {
            self.handle(node, &mut events);
        }
        if disconnect_now {
            self.disconnect_after = None;
            self.disconnected_mid_write = true;
            self.disconnect(&mut events);
            return events;
        }
        if self.job_nodes.is_empty() && !self.parser.has_pending() {
            // Nothing in progress: only status requests, a closed job or a rejected print.
            self.job_bytes.clear();
            self.idle_deadline = None;
            self.rejection_logged = false;
        } else {
            self.idle_deadline = Some(now + self.config.idle_ms);
            if self.jobs == jobs_before {
                events.push(DeviceEvent::StateChanged);
            }
        }
        events
    }

    fn on_control(&mut self, cmd: ControlCommand, _now: Millis) -> Vec<DeviceEvent> {
        vec![warn(format!(
            "{} does not support control command {:?}",
            KIND,
            serde_json::to_value(&cmd)
                .ok()
                .and_then(|v| v.get("type").and_then(Value::as_str).map(str::to_owned))
                .unwrap_or_default()
        ))]
    }

    fn on_tick(&mut self, now: Millis) -> Vec<DeviceEvent> {
        self.last_now = now;
        let mut events = Vec::new();
        if self.idle_deadline.is_some_and(|d| d <= now) {
            for node in self.parser.finish() {
                self.handle(node, &mut events);
            }
            let jobs_before = self.jobs;
            self.close_job(&mut events);
            if self.job_nodes.is_empty() {
                self.job_bytes.clear();
            }
            if self.jobs == jobs_before {
                events.push(DeviceEvent::StateChanged);
            }
        }
        events
    }

    fn next_deadline(&self) -> Option<Millis> {
        self.idle_deadline
    }

    fn faults(&self) -> Vec<FaultId> {
        FAULTS.to_vec()
    }

    fn set_fault(
        &mut self,
        fault: FaultId,
        active: bool,
        opts: FaultOptions,
        now: Millis,
    ) -> Vec<DeviceEvent> {
        self.last_now = now;
        let mut events = Vec::new();
        match fault.as_str() {
            "paper-out" => {
                self.paper_out = active;
                self.rejection_logged = false;
            }
            "cover-open" => {
                self.cover_open = active;
                self.rejection_logged = false;
            }
            "offline" => {
                self.offline_fault = active;
                if active {
                    self.disconnect(&mut events);
                } else {
                    self.connect(&mut events);
                }
            }
            "disconnect-mid-write" => {
                if active {
                    let after = opts
                        .get("afterBytes")
                        .and_then(Value::as_u64)
                        .unwrap_or(DEFAULT_AFTER_BYTES);
                    self.disconnect_after = Some(after);
                } else {
                    self.disconnect_after = None;
                    self.disconnected_mid_write = false;
                    self.connect(&mut events);
                }
            }
            "slow-write" => self.slow_write = active,
            "picker-cancel" => self.picker_cancel = active,
            _ => {}
        }
        events.push(DeviceEvent::StateChanged);
        events
    }

    fn reset(&mut self, now: Millis) -> Vec<DeviceEvent> {
        let was_connected = self.connected;
        let drawer_open = self.drawer_open;
        *self = Printer::new(self.id.clone(), self.config.clone());
        // The drawer is a separate device: its state survives a printer reset.
        self.drawer_open = drawer_open;
        self.last_now = now;
        let mut events = vec![DeviceEvent::StateChanged];
        if !was_connected {
            events.push(DeviceEvent::Connected);
        }
        events
    }

    fn on_peripheral(
        &mut self,
        _from: &DeviceId,
        signal: PeripheralSignal,
        _now: Millis,
    ) -> Vec<DeviceEvent> {
        match signal {
            PeripheralSignal::Sensor { open } if open != self.drawer_open => {
                self.drawer_open = open;
                vec![DeviceEvent::StateChanged]
            }
            _ => Vec::new(),
        }
    }
}
