//! FR-26 and PRD section 8.4: the keyboard-wedge scanner, and its faults (section 10).
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use hardless_core::{
    ControlCommand, DeviceEvent, DeviceId, DevicePlugin, Direction, EventKind, FakeClock, FaultId,
    FaultOptions, Registry, Terminator, WedgeScan,
};
use hardless_devices::scanner::{ScannerPlugin, KIND, MAX_CODE_CHARS};
use serde_json::{json, Value};

const SCANNER: &str = "scanner-1";

fn id() -> DeviceId {
    SCANNER.parse().unwrap()
}

struct Rig {
    clock: FakeClock,
    registry: Registry,
}

impl Rig {
    fn with(config: Value) -> Self {
        let clock = FakeClock::new();
        let mut registry = Registry::new(clock.clone());
        hardless_devices::register_all(&mut registry).unwrap();
        registry.create(KIND, Some(id()), config).unwrap();
        Self { clock, registry }
    }

    fn new() -> Self {
        Self::with(json!({}))
    }

    fn scan(&mut self, code: &str) -> Vec<WedgeScan> {
        self.command(ControlCommand::Scan {
            code: code.into(),
            terminator: None,
            delay_ms: None,
        })
    }

    fn command(&mut self, cmd: ControlCommand) -> Vec<WedgeScan> {
        wedges(self.registry.control(&id(), cmd).unwrap())
    }

    fn fault(&mut self, fault: FaultId, active: bool, options: Value) {
        let options: FaultOptions = serde_json::from_value(options).unwrap();
        self.registry
            .set_fault(&id(), fault, active, options)
            .unwrap();
    }

    /// Advances the clock and returns the scans that came due.
    fn advance(&mut self, ms: u64) -> Vec<WedgeScan> {
        self.clock.advance(ms);
        wedges(self.registry.tick().into_iter().map(|(_, e)| e).collect())
    }

    fn state(&self) -> Value {
        self.registry.summary(&id()).unwrap().state
    }
}

fn wedges(events: Vec<DeviceEvent>) -> Vec<WedgeScan> {
    events
        .into_iter()
        .filter_map(|e| match e {
            DeviceEvent::Wedge(scan) => Some(scan),
            _ => None,
        })
        .collect()
}

fn typed(code: &str, delay_ms: u64, terminator: Terminator) -> WedgeScan {
    WedgeScan {
        code: code.into(),
        delay_ms,
        terminator,
    }
}

#[test]
fn fr_26_a_scan_types_the_code_then_enter_8ms_apart_by_default() {
    let mut rig = Rig::new();
    assert_eq!(
        rig.scan("4006381333931"),
        [typed("4006381333931", 8, Terminator::Enter)]
    );
    let state = rig.state();
    assert_eq!(state["scans"], 1);
    assert_eq!(state["lastCode"], "4006381333931");
    assert_eq!(state["terminator"], "enter");
    assert_eq!(state["delayMs"], 8);
}

#[test]
fn fr_26_prefix_suffix_terminator_and_delay_are_configurable() {
    let mut rig = Rig::with(json!({
        "prefix": "]C1", "suffix": "#", "terminator": "tab", "delayMs": 2
    }));
    assert_eq!(rig.scan("ABC"), [typed("]C1ABC#", 2, Terminator::Tab)]);
    let no_terminator = Rig::with(json!({ "terminator": "none" })).scan("1");
    assert_eq!(no_terminator, [typed("1", 8, Terminator::None)]);
}

#[test]
fn fr_26_a_scan_can_override_terminator_and_delay() {
    let mut rig = Rig::new();
    let scans = rig.command(ControlCommand::Scan {
        code: "X".into(),
        terminator: Some(Terminator::Tab),
        delay_ms: Some(0),
    });
    assert_eq!(scans, [typed("X", 0, Terminator::Tab)]);
}

#[test]
fn fr_26_empty_and_runaway_codes_are_ignored_with_a_warning() {
    let mut rig = Rig::new();
    assert!(rig.scan("").is_empty());
    assert!(rig.scan(&"9".repeat(MAX_CODE_CHARS + 1)).is_empty());
    assert_eq!(rig.state()["scans"], 0);
    let warnings = rig
        .registry
        .events(&id(), None)
        .unwrap()
        .into_iter()
        .filter(|r| matches!(&r.kind, EventKind::Log { message, .. } if message.starts_with("scan ignored")))
        .count();
    assert_eq!(warnings, 2);
}

#[test]
fn fr_03_scans_are_recorded_in_the_event_log_as_device_to_host() {
    let mut rig = Rig::new();
    rig.scan("123");
    let events = rig.registry.events(&id(), None).unwrap();
    let wedge = events
        .iter()
        .find(|r| r.kind.name() == "wedge")
        .expect("a wedge record");
    assert_eq!(wedge.direction(), Some(Direction::DeviceToHost));
    assert_eq!(
        serde_json::to_value(wedge).unwrap()["scan"],
        json!({ "code": "123", "delayMs": 8, "terminator": "enter" })
    );
}

#[test]
fn fault_double_scan_repeats_each_scan_after_typing_plus_the_gap() {
    let mut rig = Rig::new();
    rig.fault(FaultId::DOUBLE_SCAN, true, json!({}));
    // "123" + Enter is 4 keys, 3 gaps of 8 ms: typing ends at 24 ms; default gap 30 ms.
    assert_eq!(rig.scan("123").len(), 1);
    assert_eq!(
        rig.registry.next_deadline(),
        Some(hardless_core::Millis(54))
    );
    assert!(rig.advance(53).is_empty());
    assert_eq!(rig.advance(1), [typed("123", 8, Terminator::Enter)]);
    assert_eq!(
        rig.state()["scans"],
        1,
        "the repeat is the fault, not a new scan"
    );
}

#[test]
fn fault_double_scan_gap_is_configurable_and_clears_pending_repeats() {
    let mut rig = Rig::new();
    rig.fault(FaultId::DOUBLE_SCAN, true, json!({ "gapMs": 500 }));
    rig.scan("1");
    // "1" + Enter: 8 ms of typing, then the 500 ms gap.
    assert!(rig.advance(507).is_empty());
    assert_eq!(rig.advance(1).len(), 1);

    rig.scan("2");
    rig.fault(FaultId::DOUBLE_SCAN, false, json!({}));
    assert_eq!(rig.registry.next_deadline(), None);
    assert!(rig.advance(10_000).is_empty());
}

#[test]
fn fault_partial_scan_drops_the_terminator() {
    let mut rig = Rig::new();
    rig.fault(FaultId::PARTIAL_SCAN, true, json!({}));
    assert_eq!(rig.scan("777"), [typed("777", 8, Terminator::None)]);
    let overridden = rig.command(ControlCommand::Scan {
        code: "8".into(),
        terminator: Some(Terminator::Tab),
        delay_ms: None,
    });
    assert_eq!(overridden[0].terminator, Terminator::None);
    rig.fault(FaultId::PARTIAL_SCAN, false, json!({}));
    assert_eq!(rig.scan("9")[0].terminator, Terminator::Enter);
}

#[test]
fn scanner_reset_clears_counts_and_faults() {
    let mut rig = Rig::new();
    rig.fault(FaultId::PARTIAL_SCAN, true, json!({}));
    rig.scan("1");
    rig.registry.reset(&id()).unwrap();
    assert_eq!(rig.state()["scans"], 0);
    assert_eq!(rig.scan("2")[0].terminator, Terminator::Enter);
}

#[test]
fn scanner_rejects_bad_config_and_other_commands() {
    let plugin = ScannerPlugin;
    let id = id();
    assert!(plugin
        .create(id.clone(), json!({ "terminator": "space" }))
        .is_err());
    assert!(plugin.create(id.clone(), json!({ "nope": 1 })).is_err());
    let schema = serde_json::to_value(plugin.config_schema()).unwrap();
    for field in ["terminator", "delayMs", "prefix", "suffix"] {
        assert!(schema["properties"][field].is_object(), "{field}");
    }
    let mut rig = Rig::new();
    assert!(rig.command(ControlCommand::CloseDrawer).is_empty());
    assert!(rig.registry.host_bytes(&id, b"x").unwrap().iter().any(
        |e| matches!(e, DeviceEvent::Log(entry) if entry.message.contains("no data channel"))
    ));
}
