# hardless-core

The sans-IO core of [Hardless](https://github.com/Danish-Ali097/hardless), an open-source
POS hardware emulator.

It defines what a device is and keeps track of the devices that exist. It does no I/O and
reads no clock: devices are pure state machines that receive bytes, control commands and a
`now` timestamp from the caller. Transports and the server own all I/O and time.

| Piece                         | Requirement | Types                                                            |
| ----------------------------- | ----------- | ---------------------------------------------------------------- |
| Device plugin interface       | FR-01       | `DevicePlugin`, `Device`, `DeviceEvent`, `ControlCommand`, `Job` |
| Device registry and event bus | FR-02       | `Registry`, `BusEvent`, `DeviceSummary`, `RegistryError`         |
| Event log                     | FR-03       | `EventLog`, `EventRecord`, `EventKind`                           |
| Faults                        | section 10  | `FaultId`, `FaultOptions`, `FaultSpec`, `catalogue()`            |
| Deterministic clock           | FR-05       | `Clock`, `FakeClock`, `Millis`                                   |

## Usage

```rust
use hardless_core::{
    ConfigError, ControlCommand, Device, DeviceEvent, DeviceId, DevicePlugin, FakeClock,
    FaultId, FaultOptions, Millis, Registry,
};

/// A device that answers every byte it receives with `0x06` (ACK).
struct Echo;

impl Device for Echo {
    fn kind(&self) -> &'static str { "echo" }
    fn state(&self) -> serde_json::Value { serde_json::json!({}) }
    fn on_host_bytes(&mut self, data: &[u8], _now: Millis) -> Vec<DeviceEvent> {
        vec![DeviceEvent::ToHost(vec![0x06; data.len()].into())]
    }
    fn on_control(&mut self, _cmd: ControlCommand, _now: Millis) -> Vec<DeviceEvent> { vec![] }
    fn on_tick(&mut self, _now: Millis) -> Vec<DeviceEvent> { vec![] }
    fn next_deadline(&self) -> Option<Millis> { None }
    fn faults(&self) -> Vec<FaultId> { vec![] }
    fn set_fault(&mut self, _fault: FaultId, _active: bool, _opts: FaultOptions, _now: Millis)
        -> Vec<DeviceEvent> { vec![] }
    fn reset(&mut self, _now: Millis) -> Vec<DeviceEvent> { vec![] }
}

struct EchoPlugin;

impl DevicePlugin for EchoPlugin {
    fn kind(&self) -> &'static str { "echo" }
    fn config_schema(&self) -> schemars::Schema { schemars::json_schema!({ "type": "object" }) }
    fn faults(&self) -> Vec<FaultId> { vec![] }
    fn create(&self, _id: DeviceId, _config: serde_json::Value)
        -> Result<Box<dyn Device>, ConfigError> {
        Ok(Box::new(Echo))
    }
}

let clock = FakeClock::new();
let mut registry = Registry::new(clock.clone());
registry.register(EchoPlugin).unwrap();

let id = registry.create("echo", Some("echo-1".parse().unwrap()), serde_json::json!({})).unwrap();
clock.advance(5);
let events = registry.host_bytes(&id, b"hi").unwrap();
assert!(matches!(&events[0], DeviceEvent::ToHost(b) if b.as_ref() == [0x06, 0x06]));

// Everything that happened is in the device's event log, stamped by the injected clock.
let log = registry.events(&id, None).unwrap();
assert_eq!(log.last().unwrap().at, Millis(5));

// Observers (dashboard, MCP) read the event bus.
let bus = registry.drain_bus();
assert_eq!(bus.len(), 1); // device:added
```

## Design notes

- **Sans-IO.** The registry reads time only through the `Clock` it was given; the server
  passes a system clock, tests pass `FakeClock`. No API in this crate blocks or sleeps.
- **Two outputs per call.** Methods that feed a device return its raw `DeviceEvent`s so the
  caller (a transport) can route `ToHost` bytes back to the app. The same events are recorded
  in the event log, and the ones observers care about are put on the event bus.
- **Timestamps are monotonic.** If an injected clock goes backwards, the registry keeps the
  last timestamp it handed out.
- **Faults are open.** `FaultId` is a string so plugins can define their own faults; the IDs in
  PRD section 10 are provided as constants and in `catalogue()`.

## Features

- `ts`: derives TypeScript bindings with `ts-rs`. Used by `cargo xtask gen-types`, which
  writes `packages/types`.

## License

Apache-2.0
