//! The per-device event log (FR-03).

use std::collections::VecDeque;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::device::{ControlCommand, LogLevel};
use crate::fault::{FaultId, FaultOptions};
use crate::ids::{DeviceId, JobId};
use crate::time::Millis;

/// Default number of records kept per device.
pub const DEFAULT_LOG_CAPACITY: usize = 10_000;

/// Which way bytes travelled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    /// App -> device.
    HostToDevice,
    /// Device -> app.
    DeviceToHost,
}

/// What happened, in an [`EventRecord`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum EventKind {
    /// The device was created.
    Created {
        /// Device kind.
        kind: String,
    },
    /// Host -> device bytes.
    BytesIn {
        /// The bytes.
        #[serde(rename = "dataHex", with = "crate::hex::serde_bytes")]
        #[cfg_attr(feature = "ts", ts(rename = "dataHex", type = "string"))]
        data: Bytes,
    },
    /// Device -> host bytes.
    BytesOut {
        /// The bytes.
        #[serde(rename = "dataHex", with = "crate::hex::serde_bytes")]
        #[cfg_attr(feature = "ts", ts(rename = "dataHex", type = "string"))]
        data: Bytes,
    },
    /// A control command was sent to the device.
    Control {
        /// The command.
        command: ControlCommand,
    },
    /// A fault was turned on or off.
    #[serde(rename_all = "camelCase")]
    Fault {
        /// The fault.
        fault_id: FaultId,
        /// Whether it is now on.
        active: bool,
        /// Its options.
        #[cfg_attr(
            feature = "ts",
            ts(as = "std::collections::HashMap<String, serde_json::Value>")
        )]
        options: FaultOptions,
    },
    /// The device state changed.
    StateChanged {
        /// Snapshot of the new state.
        state: serde_json::Value,
    },
    /// The device became reachable.
    Connected,
    /// The device stopped being reachable.
    Disconnected,
    /// A print job closed.
    #[serde(rename_all = "camelCase")]
    JobCompleted {
        /// The job.
        job_id: JobId,
    },
    /// A device diagnostic.
    Log {
        /// Severity.
        level: LogLevel,
        /// Message.
        message: String,
    },
    /// The device was reset.
    Reset,
}

impl EventKind {
    /// The wire name of this kind, e.g. `"bytes-in"`.
    pub fn name(&self) -> &'static str {
        match self {
            EventKind::Created { .. } => "created",
            EventKind::BytesIn { .. } => "bytes-in",
            EventKind::BytesOut { .. } => "bytes-out",
            EventKind::Control { .. } => "control",
            EventKind::Fault { .. } => "fault",
            EventKind::StateChanged { .. } => "state-changed",
            EventKind::Connected => "connected",
            EventKind::Disconnected => "disconnected",
            EventKind::JobCompleted { .. } => "job-completed",
            EventKind::Log { .. } => "log",
            EventKind::Reset => "reset",
        }
    }
}

/// One entry in a device's event log.
///
/// `seq` is unique and increasing across all devices in a registry, so clients can poll with
/// `since=<last seq>`. `at` never goes backwards.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct EventRecord {
    /// Sequence number.
    pub seq: u64,
    /// Monotonic timestamp.
    pub at: Millis,
    /// The device.
    pub device_id: DeviceId,
    /// What happened.
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub kind: EventKind,
}

impl EventRecord {
    /// The direction of a byte record, `None` for everything else (used by the log filter).
    pub fn direction(&self) -> Option<Direction> {
        match self.kind {
            EventKind::BytesIn { .. } => Some(Direction::HostToDevice),
            EventKind::BytesOut { .. } => Some(Direction::DeviceToHost),
            _ => None,
        }
    }
}

/// A bounded log that drops the oldest record when full.
///
/// ```
/// use hardless_core::{EventKind, EventLog, EventRecord, Millis};
/// let mut log = EventLog::new(2);
/// for seq in 1..=3 {
///     log.push(EventRecord { seq, at: Millis(0), device_id: "d".parse().unwrap(), kind: EventKind::Reset });
/// }
/// assert_eq!(log.iter().map(|r| r.seq).collect::<Vec<_>>(), [2, 3]);
/// ```
#[derive(Clone, Debug)]
pub struct EventLog {
    capacity: usize,
    records: VecDeque<EventRecord>,
}

impl EventLog {
    /// An empty log holding at most `capacity` records (at least one).
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            capacity,
            records: VecDeque::with_capacity(capacity.min(1024)),
        }
    }

    /// Appends a record, dropping the oldest if the log is full.
    pub fn push(&mut self, record: EventRecord) {
        if self.records.len() == self.capacity {
            self.records.pop_front();
        }
        self.records.push_back(record);
    }

    /// Records oldest first.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &EventRecord> + ExactSizeIterator {
        self.records.iter()
    }

    /// Records with `seq` greater than `since`, oldest first.
    pub fn since(&self, since: u64) -> impl Iterator<Item = &EventRecord> {
        // Records are in `seq` order, so skip the old ones with a binary search.
        let start = self.records.partition_point(|r| r.seq <= since);
        self.records.range(start..)
    }

    /// Number of records.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the log is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Maximum number of records.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Removes every record.
    pub fn clear(&mut self) {
        self.records.clear();
    }
}
