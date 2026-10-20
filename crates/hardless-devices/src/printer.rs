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
    /// The HTTP printer endpoints: ePOS-Print-compatible and WebPRNT-compatible (FR-31 to
    /// FR-35).
    Http,
}

/// The control command that ends the job being received, as a printer does at the end of an
/// HTTP print request: `ControlCommand::Custom { name: "close-job", .. }`.
pub const CLOSE_JOB: &str = "close-job";

/// What the drawer-open sensor (connector pin 3) reads while the drawer is open.
///
/// `DLE EOT 1` bit 2 reports the pin level, not "open", and real drawers are wired either way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PinLevel {
    /// Pin 3 is high while the drawer is open: bit 2 set means open.
    #[default]
    High,
    /// Pin 3 is low while the drawer is open: bit 2 set means closed.
    Low,
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
    /// The drawer sensor level that means "open" in `DLE EOT 1` bit 2.
    #[serde(default)]
    pub drawer_open_level: PinLevel,
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
            drawer_open_level: PinLevel::High,
        }
    }
}

/// Default `rate` for `garbled` (PRD section 10).
const DEFAULT_GARBLE_RATE: f64 = 0.05;

/// Faults a serial printer supports (PRD section 10).
const SERIAL_FAULTS: [FaultId; 9] = [
    FaultId::PAPER_OUT,
    FaultId::PAPER_NEAR_END,
    FaultId::COVER_OPEN,
    FaultId::OFFLINE,
    FaultId::DISCONNECT_MID_WRITE,
    FaultId::SLOW_WRITE,
    FaultId::BAUD_MISMATCH,
    FaultId::GARBLED,
    FaultId::PICKER_CANCEL,
];

/// Faults an HTTP printer supports (PRD section 10).
const HTTP_FAULTS: [FaultId; 7] = [
    FaultId::PAPER_OUT,
    FaultId::PAPER_NEAR_END,
    FaultId::COVER_OPEN,
    FaultId::OFFLINE,
    FaultId::GARBLED,
    FaultId::TIMEOUT,
    FaultId::CORS_BLOCKED,
];

/// `garbled`: flips one random bit in a byte with probability `rate`. The generator is
/// seeded when the fault is turned on, so a run is repeatable under a fake clock.
#[derive(Clone, Copy, Debug)]
struct Garble {
    rate: f64,
    state: u64,
}

impl Garble {
    fn new(rate: f64, now: Millis) -> Self {
        let seed = 0x9E37_79B9_7F4A_7C15 ^ now.0;
        Self {
            rate,
            state: seed.max(1),
        }
    }

    /// xorshift64*: small, fast and deterministic; no cryptographic needs here.
    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn apply(&mut self, byte: u8) -> u8 {
        let r = self.next();
        // 53 high bits as a uniform number in [0, 1); 3 low bits pick the bit to flip.
        let p = (r >> 11) as f64 / (1u64 << 53) as f64;
        if p < self.rate {
            byte ^ (1 << (r & 7))
        } else {
            byte
        }
    }
}

/// `baud-mismatch`: what a byte looks like when read at the wrong baud rate. Real framing
/// errors depend on the two rates; any fixed, reversible scramble shows the app the same
/// symptom (garbage and no status replies) and keeps tests repeatable.
fn baud_scramble(byte: u8) -> u8 {
    byte.rotate_left(3) ^ 0x5A
}

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
        let mut all = SERIAL_FAULTS.to_vec();
        all.extend([FaultId::TIMEOUT, FaultId::CORS_BLOCKED]);
        all
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
    paper_near_end: bool,
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
    baud_mismatch: bool,
    garbled: Option<Garble>,
    picker_cancel: bool,
    /// HTTP: `timeout` (applied by the endpoint).
    http_timeout: bool,
    /// HTTP: `cors-blocked` (applied by the endpoint).
    cors_blocked: bool,
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
            paper_near_end: false,
            cover_open: false,
            drawer_open: false,
            rejection_logged: false,
            last_now: Millis::ZERO,
            disconnect_after: None,
            disconnected_mid_write: false,
            offline_fault: false,
            slow_write: false,
            baud_mismatch: false,
            garbled: None,
            picker_cancel: false,
            http_timeout: false,
            cors_blocked: false,
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
                let pin3_high =
                    self.drawer_open == (self.config.drawer_open_level == PinLevel::High);
                if pin3_high {
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
                } else if self.paper_near_end {
                    byte |= 0x0c;
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
            "paperNearEnd": self.paper_near_end,
            "coverOpen": self.cover_open,
            "drawerOpen": self.drawer_open,
            "jobs": self.jobs,
            "rejectedPrints": self.rejected,
            "receiving": !self.job_bytes.is_empty(),
            "slowWrite": self.slow_write,
            "baudMismatch": self.baud_mismatch,
            "garbleRate": self.garbled.map(|g| g.rate),
            "pickerCancel": self.picker_cancel,
            "timeout": self.http_timeout,
            "corsBlocked": self.cors_blocked,
            "drawerOpenLevel": self.config.drawer_open_level,
            // Replies to DLE EOT 1 to 4, for the HTTP endpoints' status reports.
            "statusBytes": [
                self.status_byte(StatusKind::Printer),
                self.status_byte(StatusKind::OfflineCause),
                self.status_byte(StatusKind::ErrorCause),
                self.status_byte(StatusKind::PaperSensor),
            ],
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
        // What the printer actually receives once line faults have had their way.
        let received: Vec<u8>;
        let data = if self.baud_mismatch || self.garbled.is_some() {
            received = data
                .iter()
                .map(|&b| {
                    let b = if self.baud_mismatch {
                        baud_scramble(b)
                    } else {
                        b
                    };
                    self.garbled.as_mut().map_or(b, |g| g.apply(b))
                })
                .collect();
            &received[..]
        } else {
            data
        };
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

    fn on_control(&mut self, cmd: ControlCommand, now: Millis) -> Vec<DeviceEvent> {
        if let ControlCommand::Custom { name, .. } = &cmd {
            if name == CLOSE_JOB {
                self.last_now = now;
                let mut events = Vec::new();
                for node in self.parser.finish() {
                    self.handle(node, &mut events);
                }
                let jobs_before = self.jobs;
                self.close_job(&mut events);
                self.job_bytes.clear();
                self.job_nodes.clear();
                if self.jobs == jobs_before {
                    events.push(DeviceEvent::StateChanged);
                }
                return events;
            }
        }
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
        match self.config.transport {
            Transport::Serial => SERIAL_FAULTS.to_vec(),
            Transport::Http => HTTP_FAULTS.to_vec(),
        }
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
            "paper-near-end" => self.paper_near_end = active,
            "slow-write" => self.slow_write = active,
            "baud-mismatch" => self.baud_mismatch = active,
            "garbled" => {
                self.garbled = if active {
                    let asked = opts.get("rate").and_then(Value::as_f64);
                    let rate = asked.unwrap_or(DEFAULT_GARBLE_RATE).clamp(0.0, 1.0);
                    if asked.is_some_and(|r| r != rate) {
                        events.push(warn(format!("garbled rate must be 0 to 1; using {rate}")));
                    }
                    Some(Garble::new(rate, now))
                } else {
                    None
                };
            }
            "picker-cancel" => self.picker_cancel = active,
            "timeout" => self.http_timeout = active,
            "cors-blocked" => self.cors_blocked = active,
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
