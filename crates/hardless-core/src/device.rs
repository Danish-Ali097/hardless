//! The device plugin interface (FR-01).

use std::fmt;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::fault::{FaultId, FaultOptions};
use crate::ids::{DeviceId, JobId};
use crate::time::Millis;

/// A device type that can be instantiated from JSON config, e.g. `printer-escpos`.
///
/// Register plugins with [`Registry::register`](crate::Registry::register).
pub trait DevicePlugin: Send + Sync + 'static {
    /// The device kind this plugin creates, e.g. `"printer-escpos"`.
    fn kind(&self) -> &'static str;

    /// JSON Schema for the `config` accepted by [`DevicePlugin::create`].
    fn config_schema(&self) -> schemars::Schema;

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

/// A control action sent to a device by a person, test or agent rather than by the app.
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
        /// Card UID as hex.
        uid: String,
        /// NDEF records; their shape is defined by the NFC device.
        #[serde(default)]
        records: Vec<serde_json::Value>,
    },
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
