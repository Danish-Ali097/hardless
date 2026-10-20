//! FR-27 and PRD section 8.3: the NFC reader and its virtual cards, and its faults (section 10).
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use hardless_core::{
    ControlCommand, DeviceEvent, DeviceId, DevicePlugin, Direction, EventKind, FakeClock, FaultId,
    FaultOptions, NdefRecord, NfcEvent, NfcReading, NfcRejection, Registry,
};
use hardless_devices::nfc::{NfcPlugin, KIND};
use serde_json::{json, Value};

const READER: &str = "nfc-1";
const UID: &str = "04a2b3c4d5e6f7";
const SERIAL: &str = "04:a2:b3:c4:d5:e6:f7";

fn id() -> DeviceId {
    READER.parse().unwrap()
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

    fn nfc(&mut self, cmd: ControlCommand) -> Vec<NfcEvent> {
        self.registry
            .control(&id(), cmd)
            .unwrap()
            .into_iter()
            .filter_map(|e| match e {
                DeviceEvent::Nfc(event) => Some(event),
                _ => None,
            })
            .collect()
    }

    fn tap(&mut self, uid: &str, records: Option<Vec<NdefRecord>>) -> Vec<NfcEvent> {
        self.nfc(ControlCommand::Tap {
            uid: uid.into(),
            records,
        })
    }

    fn write(&mut self, records: Vec<NdefRecord>, overwrite: bool) -> Vec<NfcEvent> {
        self.nfc(ControlCommand::NfcWrite { records, overwrite })
    }

    fn fault(&mut self, fault: FaultId, active: bool) {
        self.registry
            .set_fault(&id(), fault, active, FaultOptions::new())
            .unwrap();
    }

    fn advance(&mut self, ms: u64) {
        self.clock.advance(ms);
        self.registry.tick();
    }

    fn state(&self) -> Value {
        self.registry.summary(&id()).unwrap().state
    }
}

fn reading(records: Vec<NdefRecord>) -> NfcEvent {
    NfcEvent::Reading(NfcReading {
        serial_number: SERIAL.into(),
        records,
    })
}

fn rejection(events: &[NfcEvent]) -> NfcRejection {
    match events {
        [NfcEvent::Rejected { reason, .. }] => *reason,
        other => panic!("expected one rejection, got {other:?}"),
    }
}

#[test]
fn fr_27_tap_reads_the_card_with_its_serial_number_and_records() {
    let mut rig = Rig::new();
    let records = vec![
        NdefRecord::text("Loyalty 1234"),
        NdefRecord::url("https://example.com"),
    ];
    assert_eq!(rig.tap(UID, Some(records.clone())), [reading(records)]);
    let state = rig.state();
    assert_eq!(state["taps"], 1);
    assert_eq!(state["card"], SERIAL);
    assert_eq!(state["cards"][0]["uid"], SERIAL);
}

#[test]
fn fr_27_the_reader_remembers_cards_and_what_the_app_wrote_to_them() {
    let mut rig = Rig::new();
    rig.tap(UID, Some(vec![NdefRecord::text("old")]));
    let written = vec![NdefRecord::text("new")];
    assert_eq!(
        rig.write(written.clone(), true),
        [NfcEvent::Written {
            serial_number: SERIAL.into(),
            records: written.clone(),
        }]
    );
    rig.advance(5_000);
    assert_eq!(rig.state()["card"], Value::Null, "the card was taken away");
    // Tapping the same card again, without records, reads what was written.
    assert_eq!(rig.tap(UID, None), [reading(written.clone())]);
    assert_eq!(rig.state()["lastWritten"]["records"][0]["data"], "new");
    assert_eq!(rig.state()["writes"], 1);
}

#[test]
fn fr_27_an_unknown_card_without_records_reads_as_empty() {
    let mut rig = Rig::new();
    assert_eq!(
        rig.tap("04:01:02:03", None),
        [NfcEvent::Reading(NfcReading {
            serial_number: "04:01:02:03".into(),
            records: vec![],
        })]
    );
}

#[test]
fn spec_8_3_write_needs_a_card_on_the_reader() {
    let mut rig = Rig::with(json!({ "presenceMs": 1000 }));
    assert_eq!(
        rejection(&rig.write(vec![NdefRecord::text("x")], true)),
        NfcRejection::NoCard
    );
    rig.tap(UID, None);
    rig.advance(999);
    assert!(matches!(
        rig.write(vec![NdefRecord::text("x")], true)[..],
        [NfcEvent::Written { .. }]
    ));
    rig.advance(1);
    assert_eq!(rig.state()["card"], Value::Null);
    assert_eq!(
        rejection(&rig.write(vec![NdefRecord::text("y")], true)),
        NfcRejection::NoCard
    );
}

#[test]
fn spec_8_3_make_read_only_blocks_later_writes() {
    let mut rig = Rig::new();
    rig.tap(UID, None);
    assert_eq!(
        rig.nfc(ControlCommand::NfcMakeReadOnly),
        [NfcEvent::MadeReadOnly {
            serial_number: SERIAL.into()
        }]
    );
    assert_eq!(
        rejection(&rig.write(vec![NdefRecord::text("x")], true)),
        NfcRejection::ReadOnly
    );
    assert_eq!(rig.state()["cards"][0]["readOnly"], true);
}

#[test]
fn spec_8_3_overwrite_false_refuses_a_card_with_data() {
    let mut rig = Rig::new();
    rig.tap(UID, Some(vec![NdefRecord::text("keep")]));
    assert_eq!(
        rejection(&rig.write(vec![NdefRecord::text("x")], false)),
        NfcRejection::NotEmpty
    );
    rig.tap("04:00:00:01", None);
    assert!(matches!(
        rig.write(vec![NdefRecord::text("x")], false)[..],
        [NfcEvent::Written { .. }]
    ));
}

#[test]
fn fault_read_error_fails_the_next_tap_only() {
    let mut rig = Rig::new();
    rig.fault(FaultId::READ_ERROR, true);
    assert_eq!(
        rig.tap(UID, None),
        [NfcEvent::ReadError {
            serial_number: SERIAL.into()
        }]
    );
    // One-shot: the registry turned the fault off again.
    let summary = rig.registry.summary(&id()).unwrap();
    assert!(summary.faults.is_empty(), "{:?}", summary.faults);
    assert_eq!(summary.state["readError"], false);
    assert_eq!(
        rig.state()["card"],
        Value::Null,
        "an unread card cannot be written"
    );
    assert_eq!(rig.tap(UID, None), [reading(vec![])]);
}

#[test]
fn faults_permission_denied_and_picker_cancel_show_in_state_for_the_shim() {
    let mut rig = Rig::new();
    rig.fault(FaultId::PERMISSION_DENIED, true);
    rig.fault(FaultId::PICKER_CANCEL, true);
    assert_eq!(rig.state()["permissionDenied"], true);
    assert_eq!(rig.state()["pickerCancel"], true);
    let plugin = NfcPlugin;
    assert_eq!(
        plugin.faults(),
        [
            FaultId::PICKER_CANCEL,
            FaultId::PERMISSION_DENIED,
            FaultId::READ_ERROR
        ]
    );
}

#[test]
fn bad_uids_and_records_are_refused_with_a_warning() {
    let mut rig = Rig::new();
    for (uid, records) in [
        ("xyz", None),
        ("04a", None),
        ("0102030405060708090a0b", None),
        (UID, Some(vec![NdefRecord::default()])),
        (
            UID,
            Some(vec![NdefRecord {
                record_type: "mime".into(),
                data: Some("a".into()),
                data_base64: Some(vec![1].into()),
                ..NdefRecord::default()
            }]),
        ),
    ] {
        let events = rig
            .registry
            .control(
                &id(),
                ControlCommand::Tap {
                    uid: uid.into(),
                    records,
                },
            )
            .unwrap();
        assert!(
            matches!(&events[..], [DeviceEvent::Log(entry)] if entry.message.starts_with("tap ignored")),
            "{uid}: {events:?}"
        );
    }
    assert_eq!(rig.state()["taps"], 0);
}

#[test]
fn config_cards_are_known_from_the_start_and_restored_on_reset() {
    let mut rig = Rig::with(json!({
        "cards": [{ "uid": UID, "records": [{ "recordType": "text", "data": "member" }], "readOnly": true }]
    }));
    assert_eq!(rig.state()["cards"][0]["readOnly"], true);
    rig.tap(UID, None);
    assert_eq!(
        rejection(&rig.write(vec![NdefRecord::text("x")], true)),
        NfcRejection::ReadOnly
    );
    rig.tap("04:00:00:02", None);
    rig.registry.reset(&id()).unwrap();
    let state = rig.state();
    assert_eq!(state["cards"].as_array().unwrap().len(), 1);
    assert_eq!(state["taps"], 0);

    let plugin = NfcPlugin;
    for bad in [
        json!({ "presenceMs": 0 }),
        json!({ "cards": [{ "uid": "nope" }] }),
        json!({ "cards": [{ "uid": UID, "records": [{ "recordType": "url" }] }] }),
    ] {
        assert!(plugin.create(id(), bad.clone()).is_err(), "{bad}");
    }
}

#[test]
fn fr_03_reads_and_writes_are_in_the_event_log_with_a_direction() {
    let mut rig = Rig::new();
    rig.tap(UID, None);
    rig.write(vec![NdefRecord::text("x")], true);
    let log = rig.registry.events(&id(), None).unwrap();
    let nfc: Vec<(Option<Direction>, &str)> = log
        .iter()
        .filter(|r| matches!(r.kind, EventKind::Nfc { .. } | EventKind::Control { .. }))
        .map(|r| (r.direction(), r.kind.name()))
        .collect();
    assert_eq!(
        nfc,
        [
            (None, "control"),
            (Some(Direction::DeviceToHost), "nfc"),
            (Some(Direction::HostToDevice), "control"),
            (None, "nfc"),
        ]
    );
}
