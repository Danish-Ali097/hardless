# Writing a device plugin

Every Hardless device, from the receipt printer to the NFC reader, is a plugin: a Rust type
that implements two traits from `hardless-core`. You can add your own the same way, for
example a customer display or a scale.

In phase 1 plugins are compiled into Hardless: add your crate to the workspace and register
the plugin next to the built-in ones (`hardless_devices::register_all`). Loading plugins at
run time is not supported yet.

## The two traits

A **`DevicePlugin`** describes a kind of device and creates instances of it from JSON config:

```rust
pub trait DevicePlugin: Send + Sync + 'static {
    fn kind(&self) -> &'static str;                  // e.g. "display-vfd"
    fn config_schema(&self) -> schemars::Schema;     // shown to API users
    fn faults(&self) -> Vec<FaultId>;                // every fault this kind can have
    fn create(&self, id: DeviceId, config: serde_json::Value)
        -> Result<Box<dyn Device>, ConfigError>;
}
```

A **`Device`** is one instance. It is a pure state machine: it never does I/O and never reads
the clock. Hardless hands it bytes from the app, control commands from people and tests, and
the current time, and it answers with `DeviceEvent`s:

```rust
pub trait Device: Send {
    fn kind(&self) -> &'static str;
    fn state(&self) -> serde_json::Value;            // shown in the dashboard and APIs
    fn on_host_bytes(&mut self, data: &[u8], now: Millis) -> Vec<DeviceEvent>;
    fn on_control(&mut self, cmd: ControlCommand, now: Millis) -> Vec<DeviceEvent>;
    fn on_tick(&mut self, now: Millis) -> Vec<DeviceEvent>;
    fn next_deadline(&self) -> Option<Millis>;
    fn faults(&self) -> Vec<FaultId>;
    fn set_fault(&mut self, fault: FaultId, active: bool, opts: FaultOptions, now: Millis)
        -> Vec<DeviceEvent>;
    fn reset(&mut self, now: Millis) -> Vec<DeviceEvent>;
    // attached_to and on_peripheral have defaults; see the cash drawer for how they are used.
}
```

Because devices never do I/O, the same code runs in the server and, compiled to WebAssembly,
inside the page (standalone mode), and tests can drive it with a fake clock.

## Events

| Event                       | Means                                                  |
| --------------------------- | ------------------------------------------------------ |
| `ToHost(bytes)`             | bytes for the app, e.g. a status reply                 |
| `StateChanged`              | `state()` changed; the dashboard and APIs update       |
| `JobCompleted(job)`         | a printed job, with its AST and text                   |
| `Connected`, `Disconnected` | the device was plugged in or unplugged                 |
| `Log(entry)`                | a message for the event log, e.g. a command it ignored |
| `Peripheral(signal)`        | a signal to attached devices, e.g. a drawer kick       |
| `Wedge(scan)`               | keys for the shim to type (keyboard-wedge scanners)    |
| `Nfc(event)`                | a card read, or the answer to an app's NFC write       |

## A small example

A two-line customer display that shows the last two lines of text the app sends:

```rust
use hardless_core::*;
use serde_json::{json, Value};

pub struct DisplayPlugin;

impl DevicePlugin for DisplayPlugin {
    fn kind(&self) -> &'static str { "display-text" }
    fn config_schema(&self) -> schemars::Schema { schemars::json_schema!({ "type": "object" }) }
    fn faults(&self) -> Vec<FaultId> { vec![] }
    fn create(&self, _id: DeviceId, _config: Value) -> Result<Box<dyn Device>, ConfigError> {
        Ok(Box::new(Display { lines: vec![String::new(), String::new()] }))
    }
}

struct Display { lines: Vec<String> }

impl Device for Display {
    fn kind(&self) -> &'static str { "display-text" }
    fn state(&self) -> Value { json!({ "lines": self.lines }) }
    fn on_host_bytes(&mut self, data: &[u8], _now: Millis) -> Vec<DeviceEvent> {
        for line in String::from_utf8_lossy(data).lines() {
            self.lines.remove(0);
            self.lines.push(line.to_owned());
        }
        vec![DeviceEvent::StateChanged]
    }
    fn on_control(&mut self, _cmd: ControlCommand, _now: Millis) -> Vec<DeviceEvent> {
        vec![DeviceEvent::Log(LogEntry::new(LogLevel::Warn, "the display takes no commands"))]
    }
    fn on_tick(&mut self, _now: Millis) -> Vec<DeviceEvent> { vec![] }
    fn next_deadline(&self) -> Option<Millis> { None }
    fn faults(&self) -> Vec<FaultId> { vec![] }
    fn set_fault(&mut self, _: FaultId, _: bool, _: FaultOptions, _: Millis) -> Vec<DeviceEvent> {
        vec![DeviceEvent::StateChanged]
    }
    fn reset(&mut self, _now: Millis) -> Vec<DeviceEvent> {
        self.lines = vec![String::new(), String::new()];
        vec![DeviceEvent::StateChanged]
    }
}
```

Register it and drive it in a test:

```rust
let clock = FakeClock::new();
let mut registry = Registry::new(clock.clone());
registry.register(DisplayPlugin).unwrap();
let id = registry.create("display-text", None, serde_json::json!({})).unwrap();
registry.host_bytes(&id, b"Total\n12.50\n").unwrap();
assert_eq!(registry.summary(&id).unwrap().state["lines"][1], "12.50");
```

## Rules the built-in devices follow

- **Never panic on input**: answer bad bytes with a `Log` warning (the ESC/POS parser turns
  unknown commands into `unknown` nodes).
- **Faults are data**: declare them in `faults()`, apply them in `set_fault`, and report them
  in `state()` so the dashboard and the shim can see them. Use the IDs from the
  [fault catalogue](reference/faults.md) when the meaning matches, or your own kebab-case IDs.
- **Time comes in as `now`**: return a deadline from `next_deadline` and do timed work in
  `on_tick`.
- **Test against the PRD IDs**: name tests after the requirement they cover, e.g.
  `fn fr_27_tap_reads_the_card()`.

The built-in devices in `crates/hardless-devices` are complete examples: `scanner.rs` is the
simplest, `drawer.rs` shows peripherals and timing, and `printer.rs` shows a parser, jobs and
status replies.
