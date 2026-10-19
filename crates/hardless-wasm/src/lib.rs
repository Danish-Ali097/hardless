//! WebAssembly build of the Hardless engine, loaded lazily by `@hardless/shim-web` in
//! standalone mode (FR-15), so devices behave the same with and without a server: this is
//! the same registry and the same devices the server runs.
//!
//! The JavaScript side owns time: it passes `performance.now()` with every call (FR-05).
//! Values cross the boundary as JSON strings in the same wire format as the REST API.

use hardless_core::{
    base64, ControlCommand, DeviceEvent, DeviceId, FakeClock, FaultId, FaultOptions, Millis,
    Registry, WedgeScan,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wasm_bindgen::prelude::{wasm_bindgen, JsError};

/// The engine version, so the shim can check it loaded a matching build.
///
/// ```
/// assert_eq!(hardless_wasm::version(), env!("CARGO_PKG_VERSION"));
/// ```
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// A device to create, as in the server's config file.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceSpec {
    kind: String,
    #[serde(default)]
    id: Option<DeviceId>,
    #[serde(default)]
    config: Value,
}

/// A device event in JSON, for the shim (bytes are base64, like the bridge).
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum EventOut {
    #[serde(rename_all = "camelCase")]
    Data { device_id: String, data: String },
    #[serde(rename_all = "camelCase")]
    Connect { device_id: String },
    #[serde(rename_all = "camelCase")]
    Disconnect { device_id: String },
    /// A keyboard-wedge scan for the shim to type, like the bridge `wedge-scan` message.
    #[serde(rename_all = "camelCase")]
    Wedge {
        device_id: String,
        #[serde(flatten)]
        scan: WedgeScan,
    },
}

fn out(device_id: &DeviceId, event: &DeviceEvent) -> Option<EventOut> {
    let device_id = device_id.to_string();
    match event {
        DeviceEvent::ToHost(data) => Some(EventOut::Data {
            device_id,
            data: base64::encode(data),
        }),
        DeviceEvent::Connected => Some(EventOut::Connect { device_id }),
        DeviceEvent::Disconnected => Some(EventOut::Disconnect { device_id }),
        DeviceEvent::Wedge(scan) => Some(EventOut::Wedge {
            device_id,
            scan: scan.clone(),
        }),
        _ => None,
    }
}

fn to_json<T: Serialize>(value: &T) -> Result<String, JsError> {
    serde_json::to_string(value).map_err(|e| JsError::new(&e.to_string()))
}

fn id(raw: &str) -> Result<DeviceId, JsError> {
    raw.parse()
        .map_err(|e: hardless_core::InvalidId| JsError::new(&e.to_string()))
}

fn err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// The engine: a registry with every device kind.
#[wasm_bindgen]
pub struct Engine {
    clock: FakeClock,
    registry: Registry,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine").finish_non_exhaustive()
    }
}

#[wasm_bindgen]
impl Engine {
    /// Creates the engine with `devices`: a JSON array of `{ kind, id?, config? }`.
    #[wasm_bindgen(constructor)]
    pub fn new(devices: &str) -> Result<Engine, JsError> {
        let specs: Vec<DeviceSpec> = serde_json::from_str(devices).map_err(err)?;
        let clock = FakeClock::new();
        let mut registry = Registry::new(clock.clone());
        hardless_devices::register_all(&mut registry).map_err(err)?;
        for spec in specs {
            registry
                .create(&spec.kind, spec.id, spec.config)
                .map_err(err)?;
        }
        registry.drain_bus();
        Ok(Engine { clock, registry })
    }

    fn at(&mut self, now: f64) {
        // f64 milliseconds from performance.now(); negative or NaN becomes 0.
        let ms = if now.is_finite() && now > 0.0 {
            now as u64
        } else {
            0
        };
        self.clock.set(Millis(ms));
    }

    /// Every device, as JSON `DeviceSummary[]`.
    pub fn devices(&self) -> Result<String, JsError> {
        to_json(&self.registry.list())
    }

    /// Bytes written by the app. Returns JSON events (`data`, `connect`, `disconnect`).
    #[wasm_bindgen(js_name = hostBytes)]
    pub fn host_bytes(
        &mut self,
        now: f64,
        device_id: &str,
        data: &[u8],
    ) -> Result<String, JsError> {
        self.at(now);
        let device = id(device_id)?;
        let events = self.registry.host_bytes(&device, data).map_err(err)?;
        to_json(
            &events
                .iter()
                .filter_map(|e| out(&device, e))
                .collect::<Vec<_>>(),
        )
    }

    /// Serial output signals (JSON `SerialSignals`), recorded in the event log.
    #[wasm_bindgen(js_name = hostSignals)]
    pub fn host_signals(
        &mut self,
        now: f64,
        device_id: &str,
        signals: &str,
    ) -> Result<(), JsError> {
        self.at(now);
        let signals = serde_json::from_str(signals).map_err(err)?;
        self.registry
            .host_signals(&id(device_id)?, signals)
            .map_err(err)
    }

    /// Runs due timers. Returns JSON events.
    pub fn tick(&mut self, now: f64) -> Result<String, JsError> {
        self.at(now);
        let events = self.registry.tick();
        to_json(
            &events
                .iter()
                .filter_map(|(d, e)| out(d, e))
                .collect::<Vec<_>>(),
        )
    }

    /// When `tick` is next needed (milliseconds, same clock as `now`), if ever.
    #[wasm_bindgen(js_name = nextDeadline)]
    pub fn next_deadline(&self) -> Option<f64> {
        self.registry.next_deadline().map(|m| m.0 as f64)
    }

    /// Turns a fault on or off; `options` is a JSON object. Returns JSON events.
    #[wasm_bindgen(js_name = setFault)]
    pub fn set_fault(
        &mut self,
        now: f64,
        device_id: &str,
        fault: &str,
        active: bool,
        options: &str,
    ) -> Result<String, JsError> {
        self.at(now);
        let device = id(device_id)?;
        let fault: FaultId = fault.parse().map_err(err)?;
        let options: FaultOptions = if options.is_empty() {
            FaultOptions::new()
        } else {
            serde_json::from_str(options).map_err(err)?
        };
        let events = self
            .registry
            .set_fault(&device, fault, active, options)
            .map_err(err)?;
        to_json(
            &events
                .iter()
                .filter_map(|e| out(&device, e))
                .collect::<Vec<_>>(),
        )
    }

    /// Sends a control command (JSON `ControlCommand`). Returns JSON events.
    pub fn control(&mut self, now: f64, device_id: &str, command: &str) -> Result<String, JsError> {
        self.at(now);
        let device = id(device_id)?;
        let command: ControlCommand = serde_json::from_str(command).map_err(err)?;
        let events = self.registry.control(&device, command).map_err(err)?;
        to_json(
            &events
                .iter()
                .filter_map(|e| out(&device, e))
                .collect::<Vec<_>>(),
        )
    }

    /// Resets a device. Returns JSON events.
    pub fn reset(&mut self, now: f64, device_id: &str) -> Result<String, JsError> {
        self.at(now);
        let device = id(device_id)?;
        let events = self.registry.reset(&device).map_err(err)?;
        to_json(
            &events
                .iter()
                .filter_map(|e| out(&device, e))
                .collect::<Vec<_>>(),
        )
    }

    /// A device's jobs, newest first, as JSON `Job[]`.
    pub fn jobs(&self, device_id: &str) -> Result<String, JsError> {
        let jobs: Vec<_> = self.registry.jobs(&id(device_id)?).map_err(err)?.collect();
        to_json(&jobs)
    }

    /// A device's event log, as JSON `EventRecord[]`.
    pub fn events(&self, device_id: &str) -> Result<String, JsError> {
        to_json(&self.registry.events(&id(device_id)?, None).map_err(err)?)
    }

    /// Event bus events since the last call, as JSON `BusEvent[]`.
    #[wasm_bindgen(js_name = drainBus)]
    pub fn drain_bus(&mut self) -> Result<String, JsError> {
        to_json(&self.registry.drain_bus())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_crate() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }
}
