//! FR-05: deterministic clock, and the M0 acceptance check: a sample device receives bytes
//! and emits `DeviceEvent`s driven by a fake clock.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

mod common;

use common::{id, SamplePlugin, KIND};
use hardless_core::{BusEvent, Clock, DeviceEvent, FakeClock, Millis, Registry};
use serde_json::json;

#[test]
fn fr_05_fake_clock_is_shared_between_clones() {
    let clock = FakeClock::new();
    let other = clock.clone();
    assert_eq!(clock.now(), Millis::ZERO);
    other.advance(250);
    assert_eq!(clock.now(), Millis(250));
    clock.set(Millis(1_000));
    assert_eq!(other.now(), Millis(1_000));
    assert_eq!(FakeClock::at(Millis(9)).now(), Millis(9));
}

#[test]
fn fr_05_any_closure_can_be_a_clock() {
    let clock = || Millis(77);
    assert_eq!(clock.now(), Millis(77));
    let mut registry = Registry::new(clock);
    registry.register(SamplePlugin).unwrap();
    registry.create(KIND, Some(id("p1")), json!({})).unwrap();
    assert!(matches!(
        registry.drain_bus()[..],
        [BusEvent::Added { at: Millis(77), .. }]
    ));
}

#[test]
fn fr_05_idle_job_close_is_driven_by_the_fake_clock() {
    let clock = FakeClock::new();
    let mut registry = Registry::new(clock.clone());
    registry.register(SamplePlugin).unwrap();
    registry
        .create(KIND, Some(id("p1")), json!({ "idleMs": 300 }))
        .unwrap();

    clock.set(Millis(1_000));
    let events = registry.host_bytes(&id("p1"), b"Total: 1,250\n").unwrap();
    assert!(matches!(events[..], [DeviceEvent::StateChanged]));
    assert_eq!(registry.next_deadline(), Some(Millis(1_300)));

    // More bytes push the idle deadline out.
    clock.advance(200);
    registry.host_bytes(&id("p1"), b"Thank you\n").unwrap();
    assert_eq!(registry.next_deadline(), Some(Millis(1_500)));

    clock.set(Millis(1_499));
    assert!(
        registry.tick().is_empty(),
        "nothing is due before the deadline"
    );

    clock.set(Millis(1_500));
    let events = registry.tick();
    let (device_id, job) = events
        .iter()
        .find_map(|(device_id, e)| match e {
            DeviceEvent::JobCompleted(job) => Some((device_id, job)),
            _ => None,
        })
        .expect("job closes at the idle deadline");
    assert_eq!(device_id, &id("p1"));
    assert_eq!(job.at, Millis(1_500));
    assert_eq!(job.text, "Total: 1,250\nThank you\n");
    assert_eq!(registry.next_deadline(), None);
}

#[test]
fn fr_05_tick_runs_only_due_devices_and_next_deadline_is_the_earliest() {
    let clock = FakeClock::new();
    let mut registry = Registry::new(clock.clone());
    registry.register(SamplePlugin).unwrap();
    registry
        .create(KIND, Some(id("fast")), json!({ "idleMs": 100 }))
        .unwrap();
    registry
        .create(KIND, Some(id("slow")), json!({ "idleMs": 500 }))
        .unwrap();
    registry.host_bytes(&id("fast"), b"a").unwrap();
    registry.host_bytes(&id("slow"), b"b").unwrap();
    assert_eq!(registry.next_deadline(), Some(Millis(100)));

    clock.set(Millis(100));
    let ids: Vec<_> = registry.tick().into_iter().map(|(id, _)| id).collect();
    assert!(ids.iter().all(|d| d == &id("fast")) && !ids.is_empty());
    assert_eq!(registry.next_deadline(), Some(Millis(500)));
}

#[test]
fn fr_05_registry_never_reads_wall_clock_time() {
    // With a frozen clock, nothing ever becomes due, however long the test takes.
    let clock = FakeClock::new();
    let mut registry = Registry::new(clock);
    registry.register(SamplePlugin).unwrap();
    registry
        .create(KIND, Some(id("p1")), json!({ "idleMs": 1 }))
        .unwrap();
    registry.host_bytes(&id("p1"), b"a").unwrap();
    for _ in 0..1_000 {
        assert!(registry.tick().is_empty());
    }
}
