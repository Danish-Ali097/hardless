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
mod registry;
mod time;

pub use bytes::Bytes;

pub use device::{
    ConfigError, ControlCommand, Device, DeviceEvent, DevicePlugin, Job, LogEntry, LogLevel,
    PeripheralSignal, Terminator,
};
pub use fault::{catalogue, FaultId, FaultOptionSpec, FaultOptions, FaultSpec, OptionDefault};
pub use ids::{DeviceId, InvalidId, JobId, MAX_DEVICE_ID_LEN};
pub use log::{Direction, EventKind, EventLog, EventRecord, DEFAULT_LOG_CAPACITY};
pub use registry::{
    ActiveFault, BusEvent, DeviceSummary, Registry, RegistryConfig, RegistryError,
    DEFAULT_BUS_CAPACITY, DEFAULT_JOB_CAPACITY, MAX_PERIPHERAL_DEPTH,
};
pub use time::{Clock, FakeClock, Millis};
