//! A small printer-like device used by the integration tests.
//!
//! It buffers host bytes, closes a job after `idleMs` of silence (default 300 ms), answers
//! `DLE EOT n` with one status byte, and supports the `paper-out` and `offline` faults.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, missing_docs)]

use hardless_core::{
    ConfigError, ControlCommand, Device, DeviceEvent, DeviceId, DevicePlugin, FaultId,
    FaultOptions, Job, JobId, LogEntry, LogLevel, Millis,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

pub const KIND: &str = "sample-printer";

/// Status byte for `DLE EOT n`: bit 1 and bit 4 are always set on real printers.
pub const STATUS_OK: u8 = 0x12;
/// `STATUS_OK` with the paper-end bits (5 and 6) set.
pub const STATUS_PAPER_OUT: u8 = 0x72;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SampleConfig {
    /// Silence after the last byte before a job is closed.
    #[serde(default = "default_idle")]
    pub idle_ms: u64,
}

fn default_idle() -> u64 {
    300
}

/// Faults the sample device supports. `disconnect-mid-write` is accepted but has no effect.
pub const SUPPORTED_FAULTS: &[FaultId] = &[
    FaultId::DISCONNECT_MID_WRITE,
    FaultId::OFFLINE,
    FaultId::PAPER_OUT,
];

pub struct SamplePlugin;

impl DevicePlugin for SamplePlugin {
    fn kind(&self) -> &'static str {
        KIND
    }

    fn config_schema(&self) -> schemars::Schema {
        schemars::schema_for!(SampleConfig)
    }

    fn faults(&self) -> Vec<FaultId> {
        SUPPORTED_FAULTS.to_vec()
    }

    fn create(&self, id: DeviceId, config: Value) -> Result<Box<dyn Device>, ConfigError> {
        let config: SampleConfig =
            serde_json::from_value(config).map_err(|e| ConfigError::new(e.to_string()))?;
        if config.idle_ms == 0 {
            return Err(ConfigError::new("idleMs must be at least 1").at("/idleMs"));
        }
        Ok(Box::new(SampleDevice::new(id, config.idle_ms)))
    }
}

pub struct SampleDevice {
    id: DeviceId,
    idle_ms: u64,
    buffer: Vec<u8>,
    deadline: Option<Millis>,
    jobs: u64,
    paper_out: bool,
    connected: bool,
    pending_eot: bool,
}

impl SampleDevice {
    pub fn new(id: DeviceId, idle_ms: u64) -> Self {
        Self {
            id,
            idle_ms,
            buffer: Vec::new(),
            deadline: None,
            jobs: 0,
            paper_out: false,
            connected: true,
            pending_eot: false,
        }
    }

    fn status(&self) -> u8 {
        if self.paper_out {
            STATUS_PAPER_OUT
        } else {
            STATUS_OK
        }
    }
}

impl Device for SampleDevice {
    fn kind(&self) -> &'static str {
        KIND
    }

    fn state(&self) -> Value {
        json!({
            "id": self.id.as_str(),
            "connected": self.connected,
            "buffered": self.buffer.len(),
            "jobs": self.jobs,
            "paperOut": self.paper_out,
        })
    }

    fn on_host_bytes(&mut self, data: &[u8], now: Millis) -> Vec<DeviceEvent> {
        if !self.connected {
            return vec![DeviceEvent::Log(LogEntry::new(
                LogLevel::Warn,
                "bytes received while offline were dropped",
            ))];
        }
        let mut out = Vec::new();
        for &b in data {
            // `DLE EOT n` may be split across chunks: remember a pending `DLE EOT`.
            if self.pending_eot {
                self.pending_eot = false;
                out.push(DeviceEvent::ToHost(vec![self.status()].into()));
                continue;
            }
            if b == 0x04 && self.buffer.last() == Some(&0x10) {
                self.buffer.pop();
                self.pending_eot = true;
                continue;
            }
            self.buffer.push(b);
        }
        if !self.buffer.is_empty() {
            self.deadline = Some(now + self.idle_ms);
        }
        out.push(DeviceEvent::StateChanged);
        out
    }

    fn on_control(&mut self, cmd: ControlCommand, _now: Millis) -> Vec<DeviceEvent> {
        match cmd {
            ControlCommand::Custom { name, .. } if name == "unplug" => {
                self.connected = false;
                vec![DeviceEvent::Disconnected]
            }
            ControlCommand::Custom { name, .. } if name == "plug" => {
                self.connected = true;
                vec![DeviceEvent::Connected]
            }
            _ => vec![DeviceEvent::Log(LogEntry::new(
                LogLevel::Warn,
                "control command not supported",
            ))],
        }
    }

    fn on_tick(&mut self, now: Millis) -> Vec<DeviceEvent> {
        let mut out = Vec::new();
        if let Some(deadline) = self.deadline {
            if deadline <= now {
                self.deadline = None;
                self.jobs += 1;
                let bytes = std::mem::take(&mut self.buffer);
                out.push(DeviceEvent::JobCompleted(Job {
                    id: JobId(self.jobs),
                    at: now,
                    ast: json!({ "byteCount": bytes.len() }),
                    text: String::from_utf8_lossy(&bytes).into_owned(),
                    bytes: bytes.into(),
                }));
                out.push(DeviceEvent::StateChanged);
            }
        }
        out
    }

    fn next_deadline(&self) -> Option<Millis> {
        self.deadline
    }

    fn faults(&self) -> Vec<FaultId> {
        SUPPORTED_FAULTS.to_vec()
    }

    fn set_fault(
        &mut self,
        fault: FaultId,
        active: bool,
        _opts: FaultOptions,
        _now: Millis,
    ) -> Vec<DeviceEvent> {
        if fault == FaultId::PAPER_OUT {
            self.paper_out = active;
            vec![DeviceEvent::StateChanged]
        } else if fault == FaultId::OFFLINE && self.connected == active {
            self.connected = !active;
            vec![if active {
                DeviceEvent::Disconnected
            } else {
                DeviceEvent::Connected
            }]
        } else {
            Vec::new()
        }
    }

    fn reset(&mut self, _now: Millis) -> Vec<DeviceEvent> {
        let was_connected = self.connected;
        *self = Self::new(self.id.clone(), self.idle_ms);
        if was_connected {
            vec![DeviceEvent::StateChanged]
        } else {
            vec![DeviceEvent::Connected]
        }
    }
}

pub fn id(s: &str) -> DeviceId {
    s.parse().unwrap()
}
