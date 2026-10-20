//! The device registry and event bus (FR-02), recording into the event log (FR-03) with an
//! injected clock (FR-05).

use std::collections::{BTreeMap, VecDeque};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::device::{
    ConfigError, ControlCommand, Device, DeviceEvent, DevicePlugin, Job, LogEntry, LogLevel,
    PeripheralSignal,
};
use crate::fault::{FaultId, FaultOptions};
use crate::ids::DeviceId;
use crate::log::{EventKind, EventLog, EventRecord, DEFAULT_LOG_CAPACITY};
use crate::nfc::NfcEvent;
use crate::time::{Clock, Millis};

/// Default number of jobs kept per device.
pub const DEFAULT_JOB_CAPACITY: usize = 1_000;
/// How many hops a chain of peripheral signals may take before it is dropped.
pub const MAX_PERIPHERAL_DEPTH: u8 = 8;
/// Default number of undrained events kept on the bus.
pub const DEFAULT_BUS_CAPACITY: usize = 10_000;

/// Limits for a [`Registry`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegistryConfig {
    /// Event log records kept per device (FR-03; default 10,000).
    pub log_capacity: usize,
    /// Completed jobs kept per device (default 1,000).
    pub job_capacity: usize,
    /// Undrained bus events kept; the oldest are dropped first (default 10,000).
    pub bus_capacity: usize,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            log_capacity: DEFAULT_LOG_CAPACITY,
            job_capacity: DEFAULT_JOB_CAPACITY,
            bus_capacity: DEFAULT_BUS_CAPACITY,
        }
    }
}

/// An event on the registry's event bus, for observers such as the dashboard and MCP server.
///
/// On the wire the `type` is `device:added`, `device:removed`, `device:state` or
/// `device:output`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type")]
pub enum BusEvent {
    /// `device:added`
    #[serde(rename = "device:added", rename_all = "camelCase")]
    Added {
        /// The new device.
        device_id: DeviceId,
        /// Its kind.
        kind: String,
        /// When it was created.
        at: Millis,
    },
    /// `device:removed`
    #[serde(rename = "device:removed", rename_all = "camelCase")]
    Removed {
        /// The removed device.
        device_id: DeviceId,
        /// When it was removed.
        at: Millis,
    },
    /// `device:state`
    #[serde(rename = "device:state", rename_all = "camelCase")]
    State {
        /// The device.
        device_id: DeviceId,
        /// Its new state.
        state: serde_json::Value,
        /// When it changed.
        at: Millis,
    },
    /// `device:output`
    #[serde(rename = "device:output", rename_all = "camelCase")]
    Output {
        /// The device.
        device_id: DeviceId,
        /// The completed job.
        job: Job,
    },
}

/// An active fault and its options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ActiveFault {
    /// The fault.
    pub id: FaultId,
    /// Its options.
    #[cfg_attr(
        feature = "ts",
        ts(as = "std::collections::HashMap<String, serde_json::Value>")
    )]
    pub options: FaultOptions,
}

/// A device as listed by [`Registry::list`] (and `GET /api/v1/devices`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DeviceSummary {
    /// Device ID.
    pub id: DeviceId,
    /// Device kind.
    pub kind: String,
    /// Device state.
    pub state: serde_json::Value,
    /// Active faults, sorted by ID.
    pub faults: Vec<ActiveFault>,
    /// Faults that can be set on this device ([`Device::faults`]), e.g. for the fault panel.
    pub supported_faults: Vec<FaultId>,
    /// The device this one is attached to, e.g. a cash drawer's printer.
    pub attached_to: Option<DeviceId>,
}

/// An error from a [`Registry`] operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryError {
    /// A plugin for this kind is already registered.
    DuplicateKind(String),
    /// No plugin is registered for this kind.
    UnknownKind(String),
    /// A device with this ID already exists.
    DuplicateId(DeviceId),
    /// No device has this ID.
    NotFound(DeviceId),
    /// The plugin rejected the config.
    InvalidConfig(ConfigError),
    /// The device does not declare this fault in [`Device::faults`].
    UnsupportedFault {
        /// The device.
        device: DeviceId,
        /// The fault that was asked for.
        fault: FaultId,
    },
}

impl RegistryError {
    /// A stable, machine-readable code for API error bodies (`{ error: { code, … } }`).
    ///
    /// ```
    /// use hardless_core::RegistryError;
    /// assert_eq!(RegistryError::UnknownKind("x".into()).code(), "unknown_kind");
    /// ```
    pub fn code(&self) -> &'static str {
        match self {
            RegistryError::DuplicateKind(_) => "duplicate_kind",
            RegistryError::UnknownKind(_) => "unknown_kind",
            RegistryError::DuplicateId(_) => "duplicate_id",
            RegistryError::NotFound(_) => "not_found",
            RegistryError::InvalidConfig(_) => "invalid_config",
            RegistryError::UnsupportedFault { .. } => "unsupported_fault",
        }
    }
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegistryError::DuplicateKind(kind) => {
                write!(f, "a plugin for kind `{kind}` is already registered")
            }
            RegistryError::UnknownKind(kind) => write!(f, "no plugin for device kind `{kind}`"),
            RegistryError::DuplicateId(id) => write!(f, "device `{id}` already exists"),
            RegistryError::NotFound(id) => write!(f, "device `{id}` not found"),
            RegistryError::InvalidConfig(err) => write!(f, "invalid device config: {err}"),
            RegistryError::UnsupportedFault { device, fault } => {
                write!(f, "device `{device}` does not support fault `{fault}`")
            }
        }
    }
}

impl std::error::Error for RegistryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RegistryError::InvalidConfig(err) => Some(err),
            _ => None,
        }
    }
}

struct Slot {
    kind: &'static str,
    device: Box<dyn Device>,
    faults: BTreeMap<FaultId, FaultOptions>,
    log: EventLog,
    /// Newest at the back.
    jobs: VecDeque<Job>,
}

/// Holds plugins and device instances, routes input to devices, records everything in each
/// device's event log and publishes [`BusEvent`]s.
///
/// The registry is sans-IO: it reads time only from the injected [`Clock`] and never blocks.
/// Methods that feed a device return the device's [`DeviceEvent`]s so the caller can deliver
/// `ToHost` bytes, wedge scans and connection changes to the app.
///
/// ```
/// use hardless_core::{FakeClock, Registry};
/// let registry = Registry::new(FakeClock::new());
/// assert!(registry.list().is_empty());
/// ```
pub struct Registry {
    clock: Box<dyn Clock>,
    config: RegistryConfig,
    plugins: BTreeMap<&'static str, Box<dyn DevicePlugin>>,
    devices: BTreeMap<DeviceId, Slot>,
    bus: VecDeque<BusEvent>,
    last_now: Millis,
    next_seq: u64,
}

impl Registry {
    /// A registry with default limits.
    pub fn new(clock: impl Clock + 'static) -> Self {
        Self::with_config(clock, RegistryConfig::default())
    }

    /// A registry with custom limits.
    pub fn with_config(clock: impl Clock + 'static, config: RegistryConfig) -> Self {
        Self {
            clock: Box::new(clock),
            config,
            plugins: BTreeMap::new(),
            devices: BTreeMap::new(),
            bus: VecDeque::new(),
            last_now: Millis::ZERO,
            next_seq: 1,
        }
    }

    /// The registry's limits.
    pub fn config(&self) -> RegistryConfig {
        self.config
    }

    /// The current time from the injected clock, clamped so it never goes backwards.
    pub fn now(&mut self) -> Millis {
        let now = self.clock.now().max(self.last_now);
        self.last_now = now;
        now
    }

    // ----- plugins -------------------------------------------------------------------------

    /// Registers a device plugin. Each kind can be registered once.
    pub fn register(&mut self, plugin: impl DevicePlugin) -> Result<(), RegistryError> {
        self.register_boxed(Box::new(plugin))
    }

    /// Registers a boxed device plugin.
    pub fn register_boxed(&mut self, plugin: Box<dyn DevicePlugin>) -> Result<(), RegistryError> {
        let kind = plugin.kind();
        if self.plugins.contains_key(kind) {
            return Err(RegistryError::DuplicateKind(kind.to_owned()));
        }
        self.plugins.insert(kind, plugin);
        Ok(())
    }

    /// The plugin for a kind.
    pub fn plugin(&self, kind: &str) -> Option<&dyn DevicePlugin> {
        self.plugins.get(kind).map(|p| p.as_ref())
    }

    /// Registered kinds, sorted.
    pub fn plugin_kinds(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.plugins.keys().copied()
    }

    // ----- devices -------------------------------------------------------------------------

    /// Creates a device. Without an `id`, one is generated as `<kind>-<n>`.
    /// Emits `device:added`.
    pub fn create(
        &mut self,
        kind: &str,
        id: Option<DeviceId>,
        config: serde_json::Value,
    ) -> Result<DeviceId, RegistryError> {
        let plugin = self
            .plugins
            .get(kind)
            .ok_or_else(|| RegistryError::UnknownKind(kind.to_owned()))?;
        let kind = plugin.kind();
        let id = match id {
            Some(id) if self.devices.contains_key(&id) => {
                return Err(RegistryError::DuplicateId(id))
            }
            Some(id) => id,
            None => self.generate_id(kind),
        };
        let device = plugin
            .create(id.clone(), config)
            .map_err(RegistryError::InvalidConfig)?;
        let slot = Slot {
            kind,
            device,
            faults: BTreeMap::new(),
            log: EventLog::new(self.config.log_capacity),
            jobs: VecDeque::new(),
        };
        self.devices.insert(id.clone(), slot);
        let at = self.now();
        self.record(
            &id,
            at,
            EventKind::Created {
                kind: kind.to_owned(),
            },
        );
        self.publish(BusEvent::Added {
            device_id: id.clone(),
            kind: kind.to_owned(),
            at,
        });
        Ok(id)
    }

    fn generate_id(&self, kind: &str) -> DeviceId {
        (1u64..)
            .filter_map(|n| format!("{kind}-{n}").parse::<DeviceId>().ok())
            .find(|id| !self.devices.contains_key(id))
            // Only reachable if the kind itself is not a valid ID prefix.
            .unwrap_or_else(|| self.fallback_id())
    }

    fn fallback_id(&self) -> DeviceId {
        let mut n = self.devices.len() as u64 + 1;
        loop {
            if let Ok(id) = format!("device-{n}").parse::<DeviceId>() {
                if !self.devices.contains_key(&id) {
                    return id;
                }
            }
            n += 1;
        }
    }

    /// Removes a device. Emits `device:removed`.
    pub fn remove(&mut self, id: &DeviceId) -> Result<(), RegistryError> {
        self.devices
            .remove(id)
            .ok_or_else(|| RegistryError::NotFound(id.clone()))?;
        let at = self.now();
        self.publish(BusEvent::Removed {
            device_id: id.clone(),
            at,
        });
        Ok(())
    }

    /// Whether a device exists.
    pub fn contains(&self, id: &DeviceId) -> bool {
        self.devices.contains_key(id)
    }

    /// A device by ID.
    pub fn get(&self, id: &DeviceId) -> Option<&dyn Device> {
        self.devices.get(id).map(|slot| slot.device.as_ref())
    }

    /// A device's summary: kind, state and active faults.
    pub fn summary(&self, id: &DeviceId) -> Option<DeviceSummary> {
        self.devices.get(id).map(|slot| summarize(id, slot))
    }

    /// Every device, sorted by ID.
    pub fn list(&self) -> Vec<DeviceSummary> {
        self.devices
            .iter()
            .map(|(id, slot)| summarize(id, slot))
            .collect()
    }

    // ----- driving devices -----------------------------------------------------------------

    /// Delivers host -> device bytes and returns the device's events.
    pub fn host_bytes(
        &mut self,
        id: &DeviceId,
        data: &[u8],
    ) -> Result<Vec<DeviceEvent>, RegistryError> {
        let now = self.now();
        self.slot(id)?;
        self.record(
            id,
            now,
            EventKind::BytesIn {
                data: bytes::Bytes::copy_from_slice(data),
            },
        );
        let events = self.slot_mut(id)?.device.on_host_bytes(data, now);
        self.absorb(id, now, &events);
        Ok(events)
    }

    /// Records serial output signals set by the app (`SerialPort.setSignals()`) in the
    /// device's event log. Devices do not react to them yet.
    pub fn host_signals(
        &mut self,
        id: &DeviceId,
        signals: crate::bridge::SerialSignals,
    ) -> Result<(), RegistryError> {
        let now = self.now();
        self.slot(id)?;
        self.record(id, now, EventKind::Signals { signals });
        Ok(())
    }

    /// Sends a control command and returns the device's events.
    pub fn control(
        &mut self,
        id: &DeviceId,
        command: ControlCommand,
    ) -> Result<Vec<DeviceEvent>, RegistryError> {
        let now = self.now();
        self.slot(id)?;
        self.record(
            id,
            now,
            EventKind::Control {
                command: command.clone(),
            },
        );
        let events = self.slot_mut(id)?.device.on_control(command, now);
        self.absorb(id, now, &events);
        Ok(events)
    }

    /// Records a diagnostic in a device's event log on behalf of a transport, e.g. a part of
    /// an HTTP print request the device cannot reproduce.
    pub fn log(&mut self, id: &DeviceId, entry: LogEntry) -> Result<(), RegistryError> {
        let now = self.now();
        self.slot(id)?;
        let kind = EventKind::Log {
            level: entry.level,
            message: entry.message,
        };
        self.record(id, now, kind);
        Ok(())
    }

    /// Turns a fault on or off, returns the device's events (e.g. `Disconnected` for
    /// `offline`) and emits `device:state`, since the active faults are part of the summary.
    ///
    /// A fault the device does not declare in [`Device::faults`] is rejected with
    /// [`RegistryError::UnsupportedFault`], whether it is being turned on or off, and nothing
    /// is changed or recorded.
    pub fn set_fault(
        &mut self,
        id: &DeviceId,
        fault: FaultId,
        active: bool,
        options: FaultOptions,
    ) -> Result<Vec<DeviceEvent>, RegistryError> {
        if !self.slot(id)?.device.faults().contains(&fault) {
            return Err(RegistryError::UnsupportedFault {
                device: id.clone(),
                fault,
            });
        }
        let now = self.now();
        let slot = self.slot_mut(id)?;
        if active {
            slot.faults.insert(fault.clone(), options.clone());
        } else {
            slot.faults.remove(&fault);
        }
        let events = slot
            .device
            .set_fault(fault.clone(), active, options.clone(), now);
        self.record(
            id,
            now,
            EventKind::Fault {
                fault_id: fault,
                active,
                options,
            },
        );
        self.absorb_with_state(id, now, events)
    }

    /// Resets a device: clears its state, jobs and faults (its event log is kept), returns
    /// the device's events and emits `device:state`.
    pub fn reset(&mut self, id: &DeviceId) -> Result<Vec<DeviceEvent>, RegistryError> {
        let now = self.now();
        let slot = self.slot_mut(id)?;
        slot.faults.clear();
        slot.jobs.clear();
        let events = slot.device.reset(now);
        self.record(id, now, EventKind::Reset);
        self.absorb_with_state(id, now, events)
    }

    /// Absorbs `events` and publishes `device:state` even if the device reported no state
    /// change, because a fault toggle or reset changes the summary regardless.
    fn absorb_with_state(
        &mut self,
        id: &DeviceId,
        now: Millis,
        events: Vec<DeviceEvent>,
    ) -> Result<Vec<DeviceEvent>, RegistryError> {
        if events.iter().any(is_state_change) {
            self.absorb(id, now, &events);
        } else {
            let mut absorbed = events.clone();
            absorbed.push(DeviceEvent::StateChanged);
            self.absorb(id, now, &absorbed);
        }
        Ok(events)
    }

    /// Runs `on_tick` for every device whose deadline has passed and returns their events.
    pub fn tick(&mut self) -> Vec<(DeviceId, DeviceEvent)> {
        let now = self.now();
        let due: Vec<DeviceId> = self
            .devices
            .iter()
            .filter(|(_, slot)| slot.device.next_deadline().is_some_and(|d| d <= now))
            .map(|(id, _)| id.clone())
            .collect();
        let mut out = Vec::new();
        for id in due {
            let Some(slot) = self.devices.get_mut(&id) else {
                continue;
            };
            let events = slot.device.on_tick(now);
            self.absorb(&id, now, &events);
            out.extend(events.into_iter().map(|e| (id.clone(), e)));
        }
        out
    }

    /// The earliest deadline of any device: when the caller should next call [`Registry::tick`].
    pub fn next_deadline(&self) -> Option<Millis> {
        self.devices
            .values()
            .filter_map(|slot| slot.device.next_deadline())
            .min()
    }

    // ----- reading -------------------------------------------------------------------------

    /// A device's event log, oldest first, optionally only records with `seq > since`.
    pub fn events(
        &self,
        id: &DeviceId,
        since: Option<u64>,
    ) -> Result<Vec<&EventRecord>, RegistryError> {
        let slot = self.slot(id)?;
        Ok(slot.log.since(since.unwrap_or(0)).collect())
    }

    /// A device's completed jobs, newest first.
    pub fn jobs(&self, id: &DeviceId) -> Result<impl Iterator<Item = &Job> + '_, RegistryError> {
        Ok(self.slot(id)?.jobs.iter().rev())
    }

    /// A device's latest job.
    pub fn last_job(&self, id: &DeviceId) -> Result<Option<&Job>, RegistryError> {
        Ok(self.slot(id)?.jobs.back())
    }

    /// Takes every event published on the bus since the last drain, oldest first.
    pub fn drain_bus(&mut self) -> Vec<BusEvent> {
        self.bus.drain(..).collect()
    }

    // ----- internals -----------------------------------------------------------------------

    fn slot(&self, id: &DeviceId) -> Result<&Slot, RegistryError> {
        self.devices
            .get(id)
            .ok_or_else(|| RegistryError::NotFound(id.clone()))
    }

    fn slot_mut(&mut self, id: &DeviceId) -> Result<&mut Slot, RegistryError> {
        self.devices
            .get_mut(id)
            .ok_or_else(|| RegistryError::NotFound(id.clone()))
    }

    fn record(&mut self, id: &DeviceId, at: Millis, kind: EventKind) {
        let seq = self.next_seq;
        if let Some(slot) = self.devices.get_mut(id) {
            self.next_seq += 1;
            slot.log.push(EventRecord {
                seq,
                at,
                device_id: id.clone(),
                kind,
            });
        }
    }

    fn publish(&mut self, event: BusEvent) {
        if self.bus.len() >= self.config.bus_capacity.max(1) {
            self.bus.pop_front();
        }
        self.bus.push_back(event);
    }

    /// Records a device's events in its log, stores jobs, and publishes bus events. State
    /// changes are coalesced into one `device:state`, published after any `device:output`.
    fn absorb(&mut self, id: &DeviceId, now: Millis, events: &[DeviceEvent]) {
        self.absorb_at_depth(id, now, events, 0);
    }

    fn absorb_at_depth(&mut self, id: &DeviceId, now: Millis, events: &[DeviceEvent], depth: u8) {
        let mut state_changed = false;
        let mut signals = Vec::new();
        let mut spent = Vec::new();
        for event in events {
            match event {
                DeviceEvent::ToHost(data) => {
                    self.record(id, now, EventKind::BytesOut { data: data.clone() });
                }
                DeviceEvent::StateChanged => state_changed = true,
                DeviceEvent::JobCompleted(job) => {
                    self.record(id, now, EventKind::JobCompleted { job_id: job.id });
                    let capacity = self.config.job_capacity.max(1);
                    if let Some(slot) = self.devices.get_mut(id) {
                        if slot.jobs.len() >= capacity {
                            slot.jobs.pop_front();
                        }
                        slot.jobs.push_back(job.clone());
                    }
                    self.publish(BusEvent::Output {
                        device_id: id.clone(),
                        job: job.clone(),
                    });
                }
                DeviceEvent::Connected => {
                    self.record(id, now, EventKind::Connected);
                    state_changed = true;
                }
                DeviceEvent::Disconnected => {
                    self.record(id, now, EventKind::Disconnected);
                    state_changed = true;
                }
                DeviceEvent::Log(entry) => {
                    let kind = EventKind::Log {
                        level: entry.level,
                        message: entry.message.clone(),
                    };
                    self.record(id, now, kind);
                }
                DeviceEvent::Peripheral(signal) => signals.push(signal.clone()),
                DeviceEvent::Wedge(scan) => {
                    self.record(id, now, EventKind::Wedge { scan: scan.clone() });
                }
                DeviceEvent::Nfc(event) => {
                    if matches!(event, NfcEvent::ReadError { .. }) {
                        // `read-error` fails the next tap only (PRD section 10).
                        spent.push(FaultId::READ_ERROR);
                    }
                    self.record(
                        id,
                        now,
                        EventKind::Nfc {
                            event: event.clone(),
                        },
                    );
                }
            }
        }
        for fault in spent {
            state_changed |= self.spend_fault(id, now, fault);
        }
        if state_changed {
            if let Some(state) = self.devices.get(id).map(|slot| slot.device.state()) {
                self.record(
                    id,
                    now,
                    EventKind::StateChanged {
                        state: state.clone(),
                    },
                );
                self.publish(BusEvent::State {
                    device_id: id.clone(),
                    state,
                    at: now,
                });
            }
        }
        for signal in signals {
            self.deliver_peripheral(id, now, signal, depth);
        }
    }

    /// Turns off a one-shot fault that has had its effect, as if it had been switched off.
    /// Returns whether it was on.
    fn spend_fault(&mut self, id: &DeviceId, now: Millis, fault: FaultId) -> bool {
        let Some(slot) = self.devices.get_mut(id) else {
            return false;
        };
        if slot.faults.remove(&fault).is_none() {
            return false;
        }
        // The device's own reaction (a state change) is reported by the caller.
        let _ = slot
            .device
            .set_fault(fault.clone(), false, FaultOptions::new(), now);
        self.record(
            id,
            now,
            EventKind::Fault {
                fault_id: fault,
                active: false,
                options: FaultOptions::new(),
            },
        );
        true
    }

    /// Delivers a peripheral signal from `from` to the device it is attached to and to every
    /// device attached to it. Signals that cause further signals are delivered too, up to
    /// [`MAX_PERIPHERAL_DEPTH`] hops, so two devices cannot loop forever.
    fn deliver_peripheral(
        &mut self,
        from: &DeviceId,
        now: Millis,
        signal: PeripheralSignal,
        depth: u8,
    ) {
        if depth >= MAX_PERIPHERAL_DEPTH {
            let message = format!(
                "peripheral signal dropped after {MAX_PERIPHERAL_DEPTH} hops (devices signalling in a loop?)"
            );
            self.record(
                from,
                now,
                EventKind::Log {
                    level: LogLevel::Warn,
                    message,
                },
            );
            return;
        }
        let host = self
            .devices
            .get(from)
            .and_then(|slot| slot.device.attached_to());
        let mut targets: Vec<DeviceId> = host.into_iter().collect();
        targets.extend(
            self.devices
                .iter()
                .filter(|(id, slot)| {
                    *id != from && slot.device.attached_to().as_ref() == Some(from)
                })
                .map(|(id, _)| id.clone()),
        );
        targets.retain(|t| t != from);
        targets.dedup();
        for to in targets {
            let Some(slot) = self.devices.get_mut(&to) else {
                // Attached to a device that does not exist (yet): nothing receives the signal.
                continue;
            };
            let events = slot.device.on_peripheral(from, signal.clone(), now);
            let kind = EventKind::Peripheral {
                from: from.clone(),
                to: to.clone(),
                signal: signal.clone(),
            };
            self.record(from, now, kind.clone());
            self.record(&to, now, kind);
            self.absorb_at_depth(&to, now, &events, depth + 1);
        }
    }
}

fn is_state_change(event: &DeviceEvent) -> bool {
    matches!(
        event,
        DeviceEvent::StateChanged | DeviceEvent::Connected | DeviceEvent::Disconnected
    )
}

fn summarize(id: &DeviceId, slot: &Slot) -> DeviceSummary {
    DeviceSummary {
        id: id.clone(),
        kind: slot.kind.to_owned(),
        state: slot.device.state(),
        faults: slot
            .faults
            .iter()
            .map(|(id, options)| ActiveFault {
                id: id.clone(),
                options: options.clone(),
            })
            .collect(),
        supported_faults: slot.device.faults(),
        attached_to: slot.device.attached_to(),
    }
}

impl fmt::Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Registry")
            .field("config", &self.config)
            .field("plugins", &self.plugins.keys().collect::<Vec<_>>())
            .field("devices", &self.devices.keys().collect::<Vec<_>>())
            .field("bus_len", &self.bus.len())
            .finish_non_exhaustive()
    }
}
