//! The device plugin interface (FR-01).

use std::fmt;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::fault::{FaultId, FaultOptions};
use crate::ids::{DeviceId, JobId};
use crate::nfc::{NdefRecord, NfcEvent};
use crate::time::Millis;

/// A device type that can be instantiated from JSON config, e.g. `printer-escpos`.
///
/// Register plugins with [`Registry::register`](crate::Registry::register).
pub trait DevicePlugin: Send + Sync + 'static {
    /// The device kind this plugin creates, e.g. `"printer-escpos"`.
    fn kind(&self) -> &'static str;

    /// JSON Schema for the `config` accepted by [`DevicePlugin::create`].
    fn config_schema(&self) -> schemars::Schema;

    /// Every fault a device of this kind can support, e.g. for listing faults by kind. A
    /// particular instance may support fewer, see [`Device::faults`].
    fn faults(&self) -> Vec<FaultId>;

    /// Creates a device instance. Invalid config is a [`ConfigError`], never a panic.
    fn create(
        &self,
        id: DeviceId,
        config: serde_json::Value,
    ) -> Result<Box<dyn Device>, ConfigError>;
}

/// A device instance.
///
/// Sans-IO: a device is a pure state machine. Transports own all I/O and time, and pass the
/// current time in as `now`. A device asks to be woken by returning a deadline from
/// [`Device::next_deadline`]; the caller then calls [`Device::on_tick`] at or after it.
pub trait Device: Send {
    /// The device kind, matching its plugin's [`DevicePlugin::kind`].
    fn kind(&self) -> &'static str;

    /// A JSON snapshot of the device's state, shown in the dashboard and APIs.
    fn state(&self) -> serde_json::Value;

    /// Host -> device bytes. A command may be split across calls.
    fn on_host_bytes(&mut self, data: &[u8], now: Millis) -> Vec<DeviceEvent>;

    /// A control action from the dashboard, API, CLI or a scenario: scan, tap, close drawer.
    fn on_control(&mut self, cmd: ControlCommand, now: Millis) -> Vec<DeviceEvent>;

    /// Time has reached (or passed) [`Device::next_deadline`]: pulses, idle job close.
    fn on_tick(&mut self, now: Millis) -> Vec<DeviceEvent>;

    /// When the device next needs [`Device::on_tick`], if ever. A deadline at or before the
    /// current time means "as soon as possible".
    fn next_deadline(&self) -> Option<Millis>;

    /// The faults that apply to this instance, which may depend on its config (a serial
    /// printer has `baud-mismatch`, a USB one has `stall`). The registry rejects any other
    /// fault with [`RegistryError::UnsupportedFault`](crate::RegistryError::UnsupportedFault).
    fn faults(&self) -> Vec<FaultId>;

    /// Turns a fault on or off at `now`, returning what that caused, e.g.
    /// [`DeviceEvent::Disconnected`] for `offline`. Turning on a fault that is already on
    /// replaces its options.
    fn set_fault(
        &mut self,
        fault: FaultId,
        active: bool,
        opts: FaultOptions,
        now: Millis,
    ) -> Vec<DeviceEvent>;

    /// Back to the freshly created state at `now`: clears buffers, jobs and faults. Returns
    /// what that caused, e.g. [`DeviceEvent::Connected`] if the device was offline.
    fn reset(&mut self, now: Millis) -> Vec<DeviceEvent>;

    /// The device this one is attached to as a peripheral, e.g. a cash drawer's printer.
    /// [`DeviceEvent::Peripheral`] signals travel between attached devices.
    fn attached_to(&self) -> Option<DeviceId> {
        None
    }

    /// A [`PeripheralSignal`] from an attached device: from a peripheral to the device it is
    /// attached to (e.g. the drawer's open sensor), or from that device to its peripherals
    /// (e.g. a drawer-kick pulse).
    fn on_peripheral(
        &mut self,
        from: &DeviceId,
        signal: PeripheralSignal,
        now: Millis,
    ) -> Vec<DeviceEvent> {
        let _ = (from, signal, now);
        Vec::new()
    }
}

/// A signal on the wires between a device and its peripherals, modelled on the drawer-kick
/// connector of receipt printers.
///
/// ```
/// use hardless_core::PeripheralSignal;
/// let kick = PeripheralSignal::Pulse { pin: 2, on_ms: 50, off_ms: 500 };
/// assert_eq!(
///     serde_json::to_value(&kick).unwrap(),
///     serde_json::json!({ "type": "pulse", "pin": 2, "onMs": 50, "offMs": 500 })
/// );
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PeripheralSignal {
    /// A drawer-kick pulse: `pin` (2 or 5) is driven for `on_ms`, then released for `off_ms`.
    #[serde(rename_all = "camelCase")]
    Pulse {
        /// Connector pin, 2 or 5.
        pin: u8,
        /// Pulse on time in milliseconds.
        on_ms: u64,
        /// Pulse off time in milliseconds.
        off_ms: u64,
    },
    /// The drawer-open sensor (connector pin 3): `true` while the drawer is open.
    Sensor {
        /// Whether the drawer is open.
        open: bool,
    },
}

/// Something a device did, returned from every [`Device`] call.
#[derive(Clone, Debug, PartialEq)]
pub enum DeviceEvent {
    /// Device -> host bytes (status replies).
    ToHost(Bytes),
    /// [`Device::state`] changed.
    StateChanged,
    /// A print job with its AST.
    JobCompleted(Job),
    /// The device became reachable by the host.
    Connected,
    /// The device stopped being reachable by the host.
    Disconnected,
    /// A diagnostic message, e.g. an unknown command.
    Log(LogEntry),
    /// A signal to attached devices; the registry delivers it (see [`Device::attached_to`]).
    Peripheral(PeripheralSignal),
    /// Keystrokes a keyboard-wedge scanner types into the page (PRD section 8.4). The
    /// transport hands them to the shim, which presses the keys like a keyboard driver.
    Wedge(WedgeScan),
    /// An NFC reader read a card, failed to read one, or answered a write from the app (PRD
    /// section 8.3). The transport hands reads to the shim as `reading` / `readingerror`.
    Nfc(NfcEvent),
}

/// What a keyboard-wedge scanner types for one scan: the characters, how fast, and the key
/// that ends it. On the bridge this is the `wedge-scan` message.
///
/// ```
/// use hardless_core::{Terminator, WedgeScan};
/// let scan = WedgeScan { code: "4006381333931".into(), delay_ms: 8, terminator: Terminator::Enter };
/// assert_eq!(
///     serde_json::to_value(&scan).unwrap(),
///     serde_json::json!({ "code": "4006381333931", "delayMs": 8, "terminator": "enter" })
/// );
/// assert_eq!(scan.keys(), 14, "13 characters and Enter");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct WedgeScan {
    /// The characters to type, including any prefix and suffix.
    pub code: String,
    /// Milliseconds between key presses.
    pub delay_ms: u64,
    /// The key pressed after the characters.
    pub terminator: Terminator,
}

impl WedgeScan {
    /// How many keys the scan presses: one per character, plus the terminator.
    pub fn keys(&self) -> u64 {
        let terminator = u64::from(self.terminator != Terminator::None);
        self.code.chars().count() as u64 + terminator
    }

    /// How long typing takes, from the first key press to the last.
    pub fn typing_ms(&self) -> u64 {
        self.keys().saturating_sub(1).saturating_mul(self.delay_ms)
    }
}

/// A completed print job.
///
/// The `ast` is device-specific JSON (the ESC/POS AST for receipt printers), so the core does
/// not depend on any command set. On the wire this is the REST job shape
/// `{ id, at, ast, text, bytesHex }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Job {
    /// Job ID, unique per device.
    pub id: JobId,
    /// When the job closed.
    pub at: Millis,
    /// The parsed command AST.
    pub ast: serde_json::Value,
    /// Plain-text rendering, for assertions.
    pub text: String,
    /// The raw bytes of the job.
    #[serde(rename = "bytesHex", with = "crate::hex::serde_bytes")]
    #[cfg_attr(feature = "ts", ts(rename = "bytesHex", type = "string"))]
    pub bytes: Bytes,
}

/// Severity of a [`LogEntry`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Detailed tracing.
    Debug,
    /// Normal operation.
    Info,
    /// Something the app under test probably did wrong, e.g. an unknown command.
    Warn,
    /// The device could not do what was asked.
    Error,
}

/// A diagnostic message from a device.
///
/// ```
/// use hardless_core::{LogEntry, LogLevel};
/// let entry = LogEntry::new(LogLevel::Warn, "unknown command 1b 7f");
/// assert_eq!(entry.message, "unknown command 1b 7f");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LogEntry {
    /// Severity.
    pub level: LogLevel,
    /// Human-readable message.
    pub message: String,
}

impl LogEntry {
    /// A log entry.
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            message: message.into(),
        }
    }
}

/// What a keyboard-wedge scanner sends after the code (PRD section 8.4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum Terminator {
    /// `Enter` (the default).
    #[default]
    Enter,
    /// `Tab`.
    Tab,
    /// No terminator.
    None,
}

/// A control action sent to a device: by a person, test or agent (scan, tap, close drawer),
/// or by the app through a browser API that does not carry bytes (an NFC write).
///
/// ```
/// use hardless_core::ControlCommand;
/// let cmd: ControlCommand =
///     serde_json::from_str(r#"{ "type": "scan", "code": "4006381333931" }"#).unwrap();
/// assert!(matches!(cmd, ControlCommand::Scan { .. }));
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ControlCommand {
    /// Scan a barcode (`POST /devices/:id/scan`).
    #[serde(rename_all = "camelCase")]
    Scan {
        /// The barcode content.
        code: String,
        /// Overrides the device's terminator.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminator: Option<Terminator>,
        /// Overrides the device's inter-key delay.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delay_ms: Option<u64>,
    },
    /// Tap an NFC card (`POST /devices/:id/tap`).
    Tap {
        /// Card UID as hex, e.g. `04a2b3c4d5e6f7`.
        uid: String,
        /// The card's NDEF records. Without them the reader uses what it remembers of this
        /// card (including anything the app wrote to it), or an empty card.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        records: Option<Vec<NdefRecord>>,
    },
    /// The app writes records to the card on the reader (`NDEFReader.write()`).
    NfcWrite {
        /// The records to write.
        records: Vec<NdefRecord>,
        /// Whether existing data may be replaced (Web NFC's `overwrite`, default `true`).
        #[serde(default = "yes")]
        overwrite: bool,
    },
    /// The app makes the card on the reader read-only (`NDEFReader.makeReadOnly()`).
    NfcMakeReadOnly,
    /// Close an open cash drawer by hand.
    CloseDrawer,
    /// A device-specific command for plugins.
    Custom {
        /// Command name.
        name: String,
        /// Command arguments.
        #[serde(default)]
        payload: serde_json::Value,
    },
}

fn yes() -> bool {
    true
}

/// A device config was rejected by [`DevicePlugin::create`].
///
/// ```
/// use hardless_core::ConfigError;
/// let err = ConfigError::new("pin must be 2 or 5").at("/pin");
/// assert_eq!(err.to_string(), "/pin: pin must be 2 or 5");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    message: String,
    pointer: Option<String>,
}

impl ConfigError {
    /// A config error with a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            pointer: None,
        }
    }

    /// Points the error at a JSON Pointer in the config, e.g. `/pin`.
    pub fn at(mut self, pointer: impl Into<String>) -> Self {
        self.pointer = Some(pointer.into());
        self
    }

    /// The message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The JSON Pointer to the bad value, if known.
    pub fn pointer(&self) -> Option<&str> {
        self.pointer.as_deref()
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.pointer {
            Some(p) => write!(f, "{p}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for ConfigError {}
