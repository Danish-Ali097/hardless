//! The engine: the device registry, a real clock, and a task that wakes devices on time.
//! Every change is broadcast to the bridge and stream connections.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use hardless_core::{
    BusEvent, DeviceEvent, DeviceId, DeviceSummary, Millis, Registry, RegistryError,
};
use tokio::sync::{broadcast, Notify};

use crate::config::DeviceSpec;

/// Something that happened, for connected clients.
#[derive(Clone, Debug)]
pub enum Event {
    /// An event bus event (`device:added`, `device:state`, ...).
    Bus(BusEvent),
    /// A device event that reached a transport: bytes for the host, a wedge scan, connect,
    /// disconnect.
    Device {
        /// The device.
        device_id: DeviceId,
        /// What it did.
        event: DeviceEvent,
        /// The bridge connection whose write caused it, which already has it.
        origin: Option<SessionId>,
    },
}

/// Which bridge connection has a port open.
pub type SessionId = u64;

/// Shared engine state.
pub struct Engine {
    registry: Mutex<Registry>,
    events: broadcast::Sender<Arc<Event>>,
    wake: Notify,
    open_ports: Mutex<HashMap<DeviceId, SessionId>>,
    next_session: Mutex<SessionId>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine").finish_non_exhaustive()
    }
}

impl Engine {
    /// An engine with every built-in device kind, creating `devices`. Time is measured from
    /// now (FR-05: the registry never reads the clock itself).
    pub fn new(devices: &[DeviceSpec]) -> Result<Arc<Self>, RegistryError> {
        let start = Instant::now();
        let clock = move || Millis(u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX));
        let mut registry = Registry::new(clock);
        hardless_devices::register_all(&mut registry)?;
        for spec in devices {
            registry.create(&spec.kind, spec.id.clone(), spec.config.clone())?;
        }
        registry.drain_bus();
        let (events, _) = broadcast::channel(4096);
        Ok(Arc::new(Self {
            registry: Mutex::new(registry),
            events,
            wake: Notify::new(),
            open_ports: Mutex::new(HashMap::new()),
            next_session: Mutex::new(1),
        }))
    }

    fn lock(&self) -> MutexGuard<'_, Registry> {
        // A panic while holding the lock cannot leave the registry half-updated in a way
        // that matters more than stopping the server, so keep going with the inner value.
        self.registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Runs `f` on the registry, then broadcasts what happened and wakes the timer task.
    pub fn with<R>(&self, f: impl FnOnce(&mut Registry) -> R) -> R {
        let (result, bus) = {
            let mut registry = self.lock();
            let result = f(&mut registry);
            (result, registry.drain_bus())
        };
        for event in bus {
            let _ = self.events.send(Arc::new(Event::Bus(event)));
        }
        self.wake.notify_one();
        result
    }

    /// Runs `f` (which drives `device_id`) and broadcasts the device's transport events.
    pub fn drive(
        &self,
        device_id: &DeviceId,
        f: impl FnOnce(&mut Registry) -> Result<Vec<DeviceEvent>, RegistryError>,
    ) -> Result<Vec<DeviceEvent>, RegistryError> {
        self.drive_from(device_id, None, f)
    }

    /// [`Engine::drive`] on behalf of a bridge connection, which handles the returned events
    /// itself; other connections still get them.
    pub fn drive_from(
        &self,
        device_id: &DeviceId,
        origin: Option<SessionId>,
        f: impl FnOnce(&mut Registry) -> Result<Vec<DeviceEvent>, RegistryError>,
    ) -> Result<Vec<DeviceEvent>, RegistryError> {
        let events = self.with(f)?;
        self.broadcast_device_events(device_id, &events, origin);
        Ok(events)
    }

    fn broadcast_device_events(
        &self,
        device_id: &DeviceId,
        events: &[DeviceEvent],
        origin: Option<SessionId>,
    ) {
        for event in events {
            if matches!(
                event,
                DeviceEvent::ToHost(_)
                    | DeviceEvent::Wedge(_)
                    | DeviceEvent::Connected
                    | DeviceEvent::Disconnected
            ) {
                let _ = self.events.send(Arc::new(Event::Device {
                    device_id: device_id.clone(),
                    event: event.clone(),
                    origin,
                }));
            }
        }
    }

    /// Subscribes to everything that happens.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Event>> {
        self.events.subscribe()
    }

    /// Every device.
    pub fn list(&self) -> Vec<DeviceSummary> {
        self.lock().list()
    }

    /// A device's summary.
    pub fn summary(&self, id: &DeviceId) -> Option<DeviceSummary> {
        self.lock().summary(id)
    }

    /// A new bridge connection ID.
    pub fn new_session(&self) -> SessionId {
        let mut next = self
            .next_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let id = *next;
        *next += 1;
        id
    }

    /// Claims a port for a connection; `false` if another connection has it open.
    pub fn claim_port(&self, device_id: &DeviceId, session: SessionId) -> bool {
        let mut ports = self
            .open_ports
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match ports.get(device_id) {
            Some(owner) if *owner != session => false,
            _ => {
                ports.insert(device_id.clone(), session);
                true
            }
        }
    }

    /// Releases one port, or every port of a connection when `device_id` is `None`.
    pub fn release_ports(&self, session: SessionId, device_id: Option<&DeviceId>) {
        let mut ports = self
            .open_ports
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ports.retain(|id, owner| *owner != session || device_id.is_some_and(|d| d != id));
    }

    /// Wakes devices when their deadlines pass (idle job close, drawer timing). Runs until
    /// the engine is dropped.
    pub async fn run_timers(self: Arc<Self>) {
        loop {
            let deadline = {
                let mut registry = self.lock();
                let now = registry.now();
                registry.next_deadline().map(|d| d.since(now))
            };
            let wait = deadline.map_or(Duration::from_secs(3600), Duration::from_millis);
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                () = self.wake.notified() => continue,
            }
            let due = self.with(Registry::tick);
            for (device_id, event) in due {
                self.broadcast_device_events(&device_id, std::slice::from_ref(&event), None);
            }
        }
    }
}
