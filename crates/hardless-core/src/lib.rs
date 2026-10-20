#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod base64;
pub mod bridge;
mod device;
mod fault;
mod hex;
mod ids;
mod log;
mod nfc;
mod registry;
mod scenario;
mod time;

pub use bytes::Bytes;

pub use device::{
    ConfigError, ControlCommand, Device, DeviceEvent, DevicePlugin, Job, LogEntry, LogLevel,
    PeripheralSignal, Terminator, WedgeScan,
};
pub use fault::{catalogue, FaultId, FaultOptionSpec, FaultOptions, FaultSpec, OptionDefault};
pub use ids::{DeviceId, InvalidId, JobId, MAX_DEVICE_ID_LEN};
pub use log::{Direction, EventKind, EventLog, EventRecord, DEFAULT_LOG_CAPACITY};
pub use nfc::{
    serial_number, validate_ndef_message, NdefRecord, NfcEvent, NfcReading, NfcRejection,
    MAX_NDEF_BYTES, MAX_NDEF_RECORDS,
};
pub use registry::{
    ActiveFault, BusEvent, DeviceSummary, Registry, RegistryConfig, RegistryError,
    DEFAULT_BUS_CAPACITY, DEFAULT_JOB_CAPACITY, MAX_PERIPHERAL_DEPTH,
};
pub use scenario::{
    Progress, Scenario, ScenarioReport, ScenarioRun, Step, StepResult, StepStatus, EXPECT_POLL_MS,
    MAX_SCENARIO_STEPS,
};
pub use time::{Clock, FakeClock, Millis};
