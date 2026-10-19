//! Keyboard-wedge barcode scanner (`scanner-wedge`, FR-26, PRD section 8.4).
//!
//! A wedge scanner is a keyboard to the computer: for each barcode it types the characters a
//! few milliseconds apart, then a terminator key. This device emits each scan as a
//! [`DeviceEvent::Wedge`]; the shim presses the keys on the page's focused element.

use std::collections::VecDeque;

use hardless_core::{
    ConfigError, ControlCommand, Device, DeviceEvent, DeviceId, DevicePlugin, FaultId,
    FaultOptions, LogEntry, LogLevel, Millis, Terminator, WedgeScan,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The device kind.
pub const KIND: &str = "scanner-wedge";

/// Default `gapMs` for `double-scan` (PRD section 10).
const DEFAULT_GAP_MS: u64 = 30;

/// Longest code accepted, in characters. Real barcodes are far shorter (a QR code holds at
/// most a few thousand characters); this only stops runaway input.
pub const MAX_CODE_CHARS: usize = 4096;

/// Scanner configuration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScannerConfig {
    /// The key pressed after each code: `enter` (default), `tab` or `none`.
    #[serde(default)]
    #[schemars(with = "String")]
    pub terminator: Terminator,
    /// Milliseconds between key presses (default 8).
    #[serde(default = "default_delay_ms")]
    pub delay_ms: u64,
    /// Characters typed before every code.
    #[serde(default)]
    pub prefix: String,
    /// Characters typed after every code, before the terminator.
    #[serde(default)]
    pub suffix: String,
}

fn default_delay_ms() -> u64 {
    8
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            terminator: Terminator::Enter,
            delay_ms: default_delay_ms(),
            prefix: String::new(),
            suffix: String::new(),
        }
    }
}

/// Faults the scanner supports.
const FAULTS: [FaultId; 2] = [FaultId::DOUBLE_SCAN, FaultId::PARTIAL_SCAN];

/// Creates [`Scanner`]s.
#[derive(Debug, Default)]
pub struct ScannerPlugin;

impl DevicePlugin for ScannerPlugin {
    fn kind(&self) -> &'static str {
        KIND
    }

    fn config_schema(&self) -> schemars::Schema {
        schemars::schema_for!(ScannerConfig)
    }

    fn faults(&self) -> Vec<FaultId> {
        FAULTS.to_vec()
    }

    fn create(&self, _id: DeviceId, config: Value) -> Result<Box<dyn Device>, ConfigError> {
        let config = if config.is_null() {
            ScannerConfig::default()
        } else {
            serde_json::from_value::<ScannerConfig>(config)
                .map_err(|e| ConfigError::new(e.to_string()))?
        };
        Ok(Box::new(Scanner::new(config)))
    }
}

/// An emulated keyboard-wedge scanner.
#[derive(Debug)]
pub struct Scanner {
    config: ScannerConfig,
    scans: u64,
    last_code: Option<String>,
    /// `double-scan`: the gap in milliseconds, while on.
    double_scan: Option<u64>,
    partial_scan: bool,
    /// Repeats due from `double-scan`, earliest first.
    pending: VecDeque<(Millis, WedgeScan)>,
}

impl Scanner {
    /// A scanner with no scans yet.
    pub fn new(config: ScannerConfig) -> Self {
        Self {
            config,
            scans: 0,
            last_code: None,
            double_scan: None,
            partial_scan: false,
            pending: VecDeque::new(),
        }
    }

    fn scan(
        &mut self,
        code: String,
        terminator: Option<Terminator>,
        delay_ms: Option<u64>,
        now: Millis,
    ) -> Vec<DeviceEvent> {
        if code.is_empty() {
            return vec![warn("scan ignored: the code is empty")];
        }
        if code.chars().count() > MAX_CODE_CHARS {
            return vec![warn(format!(
                "scan ignored: the code is longer than {MAX_CODE_CHARS} characters"
            ))];
        }
        let terminator = if self.partial_scan {
            Terminator::None
        } else {
            terminator.unwrap_or(self.config.terminator)
        };
        let scan = WedgeScan {
            code: format!("{}{code}{}", self.config.prefix, self.config.suffix),
            delay_ms: delay_ms.unwrap_or(self.config.delay_ms),
            terminator,
        };
        if let Some(gap) = self.double_scan {
            // The repeat starts `gap` ms after the last key of the first scan.
            let at = now + scan.typing_ms().saturating_add(gap);
            let index = self.pending.partition_point(|(t, _)| *t <= at);
            self.pending.insert(index, (at, scan.clone()));
        }
        self.scans += 1;
        self.last_code = Some(code);
        vec![DeviceEvent::Wedge(scan), DeviceEvent::StateChanged]
    }
}

fn warn(message: impl Into<String>) -> DeviceEvent {
    DeviceEvent::Log(LogEntry::new(LogLevel::Warn, message))
}

impl Device for Scanner {
    fn kind(&self) -> &'static str {
        KIND
    }

    fn state(&self) -> Value {
        json!({
            "terminator": self.config.terminator,
            "delayMs": self.config.delay_ms,
            "prefix": self.config.prefix,
            "suffix": self.config.suffix,
            "scans": self.scans,
            "lastCode": self.last_code,
        })
    }

    fn on_host_bytes(&mut self, _data: &[u8], _now: Millis) -> Vec<DeviceEvent> {
        vec![warn(
            "a keyboard-wedge scanner has no data channel from the app; bytes were dropped",
        )]
    }

    fn on_control(&mut self, cmd: ControlCommand, now: Millis) -> Vec<DeviceEvent> {
        match cmd {
            ControlCommand::Scan {
                code,
                terminator,
                delay_ms,
            } => self.scan(code, terminator, delay_ms, now),
            other => {
                let name = serde_json::to_value(&other)
                    .ok()
                    .and_then(|v| v.get("type").and_then(Value::as_str).map(str::to_owned))
                    .unwrap_or_default();
                vec![warn(format!(
                    "{KIND} does not support control command {name}"
                ))]
            }
        }
    }

    fn on_tick(&mut self, now: Millis) -> Vec<DeviceEvent> {
        let mut events = Vec::new();
        while self.pending.front().is_some_and(|(at, _)| *at <= now) {
            if let Some((_, scan)) = self.pending.pop_front() {
                events.push(DeviceEvent::Wedge(scan));
            }
        }
        events
    }

    fn next_deadline(&self) -> Option<Millis> {
        self.pending.front().map(|(at, _)| *at)
    }

    fn faults(&self) -> Vec<FaultId> {
        FAULTS.to_vec()
    }

    fn set_fault(
        &mut self,
        fault: FaultId,
        active: bool,
        opts: FaultOptions,
        _now: Millis,
    ) -> Vec<DeviceEvent> {
        match fault.as_str() {
            "double-scan" => {
                self.double_scan = active.then(|| {
                    opts.get("gapMs")
                        .and_then(Value::as_u64)
                        .unwrap_or(DEFAULT_GAP_MS)
                });
                if !active {
                    self.pending.clear();
                }
            }
            "partial-scan" => self.partial_scan = active,
            _ => {}
        }
        vec![DeviceEvent::StateChanged]
    }

    fn reset(&mut self, _now: Millis) -> Vec<DeviceEvent> {
        *self = Scanner::new(self.config.clone());
        vec![DeviceEvent::StateChanged]
    }
}
