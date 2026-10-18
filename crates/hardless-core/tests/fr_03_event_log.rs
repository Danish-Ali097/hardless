//! FR-03: event log.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

mod common;

use common::{id, SamplePlugin, KIND, STATUS_OK};
use hardless_core::{
    Direction, EventKind, EventLog, EventRecord, FakeClock, FaultId, FaultOptions, Millis,
    Registry, RegistryConfig, DEFAULT_LOG_CAPACITY,
};
use proptest::prelude::*;
use serde_json::json;

fn registry_with(log_capacity: usize) -> (FakeClock, Registry) {
    let clock = FakeClock::new();
    let mut registry = Registry::with_config(
        clock.clone(),
        RegistryConfig {
            log_capacity,
            ..Default::default()
        },
    );
    registry.register(SamplePlugin).unwrap();
    registry.create(KIND, Some(id("p1")), json!({})).unwrap();
    (clock, registry)
}

fn kinds(records: &[&EventRecord]) -> Vec<&'static str> {
    records.iter().map(|r| r.kind.name()).collect()
}

#[test]
fn fr_03_records_bytes_in_and_out_with_direction() {
    let (clock, mut registry) = registry_with(DEFAULT_LOG_CAPACITY);
    clock.set(Millis(10));
    registry.host_bytes(&id("p1"), b"\x10\x04\x01").unwrap();
    let log = registry.events(&id("p1"), None).unwrap();
    assert_eq!(
        kinds(&log),
        ["created", "bytes-in", "bytes-out", "state-changed"]
    );

    match &log[1].kind {
        EventKind::BytesIn { data } => assert_eq!(data.as_ref(), b"\x10\x04\x01"),
        other => panic!("{other:?}"),
    }
    match &log[2].kind {
        EventKind::BytesOut { data } => assert_eq!(data.as_ref(), [STATUS_OK]),
        other => panic!("{other:?}"),
    }
    assert_eq!(log[1].direction(), Some(Direction::HostToDevice));
    assert_eq!(log[2].direction(), Some(Direction::DeviceToHost));
    assert_eq!(log[3].direction(), None);
    assert!(log[1..]
        .iter()
        .all(|r| r.at == Millis(10) && r.device_id == id("p1")));
}

#[test]
fn fr_03_records_every_state_change_with_a_snapshot() {
    let (clock, mut registry) = registry_with(DEFAULT_LOG_CAPACITY);
    registry
        .set_fault(&id("p1"), FaultId::PAPER_OUT, true, FaultOptions::new())
        .unwrap();
    registry.host_bytes(&id("p1"), b"hi").unwrap();
    clock.advance(300);
    registry.tick();
    registry.reset(&id("p1")).unwrap();

    let log = registry.events(&id("p1"), None).unwrap();
    assert_eq!(
        kinds(&log),
        [
            "created",
            "fault",
            "state-changed",
            "bytes-in",
            "state-changed",
            "job-completed",
            "state-changed",
            "reset",
            "state-changed",
        ]
    );
    match &log[2].kind {
        EventKind::StateChanged { state } => assert_eq!(state["paperOut"], true),
        other => panic!("{other:?}"),
    }
}

#[test]
fn fr_03_timestamps_are_monotonic_even_if_the_clock_goes_back() {
    let (clock, mut registry) = registry_with(DEFAULT_LOG_CAPACITY);
    clock.set(Millis(1_000));
    registry.host_bytes(&id("p1"), b"a").unwrap();
    clock.set(Millis(500));
    registry.host_bytes(&id("p1"), b"b").unwrap();
    let log = registry.events(&id("p1"), None).unwrap();
    let times: Vec<_> = log.iter().map(|r| r.at).collect();
    assert!(times.windows(2).all(|w| w[0] <= w[1]), "{times:?}");
    assert_eq!(log.last().unwrap().at, Millis(1_000));
}

#[test]
fn fr_03_default_cap_is_10000_events_per_device() {
    assert_eq!(DEFAULT_LOG_CAPACITY, 10_000);
    assert_eq!(RegistryConfig::default().log_capacity, 10_000);
    let (_, mut registry) = registry_with(DEFAULT_LOG_CAPACITY);
    for _ in 0..6_000 {
        registry.host_bytes(&id("p1"), b"x").unwrap(); // 2 records per call
    }
    let log = registry.events(&id("p1"), None).unwrap();
    assert_eq!(log.len(), 10_000);
    assert_eq!(log.last().unwrap().kind.name(), "state-changed");
}

#[test]
fn fr_03_cap_is_configurable_and_drops_oldest_first() {
    let (_, mut registry) = registry_with(4);
    for _ in 0..3 {
        registry.host_bytes(&id("p1"), b"x").unwrap();
    }
    let log = registry.events(&id("p1"), None).unwrap();
    assert_eq!(log.len(), 4);
    assert_eq!(
        kinds(&log),
        ["bytes-in", "state-changed", "bytes-in", "state-changed"]
    );
}

#[test]
fn fr_03_since_returns_only_newer_records() {
    let (_, mut registry) = registry_with(DEFAULT_LOG_CAPACITY);
    registry.host_bytes(&id("p1"), b"a").unwrap();
    let last_seen = registry
        .events(&id("p1"), None)
        .unwrap()
        .last()
        .unwrap()
        .seq;
    registry.host_bytes(&id("p1"), b"b").unwrap();
    let newer = registry.events(&id("p1"), Some(last_seen)).unwrap();
    assert_eq!(kinds(&newer), ["bytes-in", "state-changed"]);
    assert!(newer.iter().all(|r| r.seq > last_seen));
}

#[test]
fn fr_03_sequence_numbers_are_unique_across_devices() {
    let (_, mut registry) = registry_with(DEFAULT_LOG_CAPACITY);
    registry.create(KIND, Some(id("p2")), json!({})).unwrap();
    registry.host_bytes(&id("p1"), b"a").unwrap();
    registry.host_bytes(&id("p2"), b"a").unwrap();
    let mut seqs: Vec<u64> = registry
        .events(&id("p1"), None)
        .unwrap()
        .iter()
        .chain(registry.events(&id("p2"), None).unwrap().iter())
        .map(|r| r.seq)
        .collect();
    let n = seqs.len();
    seqs.sort_unstable();
    seqs.dedup();
    assert_eq!(seqs.len(), n);
}

#[test]
fn fr_03_record_wire_format_uses_hex_for_bytes() {
    let (clock, mut registry) = registry_with(DEFAULT_LOG_CAPACITY);
    clock.set(Millis(3));
    registry.host_bytes(&id("p1"), b"\x1b@").unwrap();
    let log = registry.events(&id("p1"), Some(0)).unwrap();
    let wire = serde_json::to_value(log[1]).unwrap();
    assert_eq!(
        wire,
        json!({ "seq": 2, "at": 3, "deviceId": "p1", "type": "bytes-in", "dataHex": "1b40" })
    );
    let back: EventRecord = serde_json::from_value(wire).unwrap();
    assert_eq!(&back, log[1]);
    insta::assert_json_snapshot!("fr_03_records", log);
}

#[test]
fn fr_03_unknown_device_is_an_error() {
    let (_, registry) = registry_with(DEFAULT_LOG_CAPACITY);
    assert!(registry.events(&id("nope"), None).is_err());
}

proptest! {
    #[test]
    fn fr_03_log_never_exceeds_capacity_and_seq_strictly_increases(
        cap in 1usize..64,
        pushes in proptest::collection::vec(0u64..1_000, 0..200),
    ) {
        let mut log = EventLog::new(cap);
        for (seq, at) in pushes.iter().enumerate() {
            log.push(EventRecord {
                seq: seq as u64 + 1,
                at: Millis(*at),
                device_id: id("p1"),
                kind: EventKind::Reset,
            });
            prop_assert!(log.len() <= cap);
        }
        let seqs: Vec<u64> = log.iter().map(|r| r.seq).collect();
        prop_assert!(seqs.windows(2).all(|w| w[0] < w[1]));
        prop_assert_eq!(log.len(), pushes.len().min(cap));
    }
}
