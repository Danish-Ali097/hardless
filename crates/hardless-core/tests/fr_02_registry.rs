//! FR-02: device registry and event bus.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

mod common;

use common::{id, SamplePlugin, KIND};
use hardless_core::{
    BusEvent, ControlCommand, DeviceEvent, FakeClock, FaultId, FaultOptions, Millis, Registry,
    RegistryError,
};
use serde_json::json;

fn registry() -> (FakeClock, Registry) {
    let clock = FakeClock::new();
    let mut registry = Registry::new(clock.clone());
    registry.register(SamplePlugin).unwrap();
    (clock, registry)
}

#[test]
fn fr_02_register_rejects_duplicate_kind() {
    let (_, mut registry) = registry();
    assert_eq!(
        registry.register(SamplePlugin),
        Err(RegistryError::DuplicateKind(KIND.into()))
    );
    assert_eq!(registry.plugin_kinds().collect::<Vec<_>>(), [KIND]);
    assert!(registry.plugin(KIND).is_some());
}

#[test]
fn fr_02_create_emits_device_added() {
    let (clock, mut registry) = registry();
    clock.set(Millis(42));
    let device = registry
        .create(KIND, Some(id("printer-1")), json!({}))
        .unwrap();
    assert_eq!(device, id("printer-1"));
    assert_eq!(
        registry.drain_bus(),
        [BusEvent::Added {
            device_id: id("printer-1"),
            kind: KIND.into(),
            at: Millis(42)
        }]
    );
    assert!(registry.drain_bus().is_empty(), "drain empties the bus");
}

#[test]
fn fr_02_create_errors() {
    let (_, mut registry) = registry();
    assert_eq!(
        registry.create("laser-printer", None, json!({})),
        Err(RegistryError::UnknownKind("laser-printer".into()))
    );
    registry.create(KIND, Some(id("p1")), json!({})).unwrap();
    assert_eq!(
        registry.create(KIND, Some(id("p1")), json!({})),
        Err(RegistryError::DuplicateId(id("p1")))
    );
    let err = registry
        .create(KIND, Some(id("p2")), json!({ "idleMs": 0 }))
        .unwrap_err();
    assert!(matches!(err, RegistryError::InvalidConfig(_)));
    assert_eq!(err.code(), "invalid_config");
    assert!(
        !registry.contains(&id("p2")),
        "a failed create leaves nothing behind"
    );
}

#[test]
fn fr_02_create_without_id_generates_unique_ids() {
    let (_, mut registry) = registry();
    registry
        .create(KIND, Some(id("sample-printer-1")), json!({}))
        .unwrap();
    let a = registry.create(KIND, None, json!({})).unwrap();
    let b = registry.create(KIND, None, json!({})).unwrap();
    assert_ne!(a, b);
    assert_ne!(a, id("sample-printer-1"));
    assert!(a.as_str().starts_with("sample-printer-"));
}

#[test]
fn fr_02_lookup_by_id_and_list_all_sorted_by_id() {
    let (_, mut registry) = registry();
    registry.create(KIND, Some(id("b")), json!({})).unwrap();
    registry.create(KIND, Some(id("a")), json!({})).unwrap();

    assert_eq!(registry.get(&id("a")).unwrap().kind(), KIND);
    assert!(registry.get(&id("zzz")).is_none());

    let list = registry.list();
    assert_eq!(
        list.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(list[0].kind, KIND);
    assert_eq!(list[0].state["connected"], true);
    assert!(list[0].faults.is_empty());
}

#[test]
fn fr_02_remove_emits_device_removed() {
    let (clock, mut registry) = registry();
    registry.create(KIND, Some(id("p1")), json!({})).unwrap();
    registry.drain_bus();
    clock.set(Millis(7));
    registry.remove(&id("p1")).unwrap();
    assert_eq!(
        registry.drain_bus(),
        [BusEvent::Removed {
            device_id: id("p1"),
            at: Millis(7)
        }]
    );
    assert_eq!(
        registry.remove(&id("p1")),
        Err(RegistryError::NotFound(id("p1")))
    );
    assert_eq!(RegistryError::NotFound(id("p1")).code(), "not_found");
}

#[test]
fn fr_02_state_change_emits_one_device_state_per_call() {
    let (_, mut registry) = registry();
    registry.create(KIND, Some(id("p1")), json!({})).unwrap();
    registry.drain_bus();

    registry.host_bytes(&id("p1"), b"abc").unwrap();
    let bus = registry.drain_bus();
    assert_eq!(bus.len(), 1);
    match &bus[0] {
        BusEvent::State {
            device_id, state, ..
        } => {
            assert_eq!(device_id, &id("p1"));
            assert_eq!(state["buffered"], 3);
        }
        other => panic!("expected device:state, got {other:?}"),
    }
}

#[test]
fn fr_02_job_completed_emits_device_output_and_is_stored_newest_first() {
    let (clock, mut registry) = registry();
    registry.create(KIND, Some(id("p1")), json!({})).unwrap();
    for text in ["first", "second"] {
        registry.host_bytes(&id("p1"), text.as_bytes()).unwrap();
        clock.advance(300);
        registry.tick();
    }
    let outputs: Vec<_> = registry
        .drain_bus()
        .into_iter()
        .filter_map(|e| match e {
            BusEvent::Output { device_id, job } => Some((device_id, job.text)),
            _ => None,
        })
        .collect();
    assert_eq!(
        outputs,
        [(id("p1"), "first".into()), (id("p1"), "second".into())]
    );

    let jobs: Vec<_> = registry
        .jobs(&id("p1"))
        .unwrap()
        .map(|j| j.text.as_str())
        .collect();
    assert_eq!(jobs, ["second", "first"]);
    assert_eq!(
        registry.last_job(&id("p1")).unwrap().unwrap().text,
        "second"
    );
}

#[test]
fn fr_02_host_bytes_returns_device_events_for_the_transport() {
    let (_, mut registry) = registry();
    registry.create(KIND, Some(id("p1")), json!({})).unwrap();
    let events = registry.host_bytes(&id("p1"), b"\x10\x04\x01").unwrap();
    assert!(events.iter().any(|e| matches!(e, DeviceEvent::ToHost(_))));
    assert_eq!(
        registry.host_bytes(&id("missing"), b"x"),
        Err(RegistryError::NotFound(id("missing")))
    );
}

#[test]
fn fr_02_faults_are_tracked_per_device_and_reported_in_list() {
    let (_, mut registry) = registry();
    registry.create(KIND, Some(id("p1")), json!({})).unwrap();
    let mut opts = FaultOptions::new();
    opts.insert("afterBytes".into(), json!(10));
    registry
        .set_fault(&id("p1"), FaultId::DISCONNECT_MID_WRITE, true, opts)
        .unwrap();
    registry
        .set_fault(&id("p1"), FaultId::PAPER_OUT, true, FaultOptions::new())
        .unwrap();

    let summary = registry.summary(&id("p1")).unwrap();
    let ids: Vec<_> = summary.faults.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["disconnect-mid-write", "paper-out"]);
    assert_eq!(summary.faults[0].options["afterBytes"], 10);
    assert_eq!(summary.state["paperOut"], true);

    registry
        .set_fault(&id("p1"), FaultId::PAPER_OUT, false, FaultOptions::new())
        .unwrap();
    assert_eq!(registry.summary(&id("p1")).unwrap().faults.len(), 1);
}

#[test]
fn fr_02_set_fault_runs_the_due_tick_so_disconnects_are_immediate() {
    let (_, mut registry) = registry();
    registry.create(KIND, Some(id("p1")), json!({})).unwrap();
    let events = registry
        .set_fault(&id("p1"), FaultId::OFFLINE, true, FaultOptions::new())
        .unwrap();
    assert!(matches!(events[..], [DeviceEvent::Disconnected]));
    let events = registry
        .set_fault(&id("p1"), FaultId::OFFLINE, false, FaultOptions::new())
        .unwrap();
    assert!(matches!(events[..], [DeviceEvent::Connected]));
}

#[test]
fn fr_02_reset_clears_state_jobs_and_faults() {
    let (clock, mut registry) = registry();
    registry.create(KIND, Some(id("p1")), json!({})).unwrap();
    registry
        .set_fault(&id("p1"), FaultId::PAPER_OUT, true, FaultOptions::new())
        .unwrap();
    registry.host_bytes(&id("p1"), b"job").unwrap();
    clock.advance(300);
    registry.tick();
    assert_eq!(registry.jobs(&id("p1")).unwrap().count(), 1);

    registry.drain_bus();
    registry.reset(&id("p1")).unwrap();
    let summary = registry.summary(&id("p1")).unwrap();
    assert!(summary.faults.is_empty());
    assert_eq!(summary.state["paperOut"], false);
    assert_eq!(registry.jobs(&id("p1")).unwrap().count(), 0);
    assert!(matches!(registry.drain_bus()[..], [BusEvent::State { .. }]));
}

#[test]
fn fr_02_control_is_routed_to_the_device() {
    let (_, mut registry) = registry();
    registry.create(KIND, Some(id("p1")), json!({})).unwrap();
    let events = registry
        .control(
            &id("p1"),
            ControlCommand::Custom {
                name: "unplug".into(),
                payload: json!(null),
            },
        )
        .unwrap();
    assert!(matches!(events[..], [DeviceEvent::Disconnected]));
    assert_eq!(
        registry.summary(&id("p1")).unwrap().state["connected"],
        false
    );
}

#[test]
fn fr_02_bus_event_wire_names() {
    let (clock, mut registry) = registry();
    registry.create(KIND, Some(id("p1")), json!({})).unwrap();
    registry.host_bytes(&id("p1"), b"Total: 1,250\n").unwrap();
    clock.advance(300);
    registry.tick();
    registry.remove(&id("p1")).unwrap();
    let bus = registry.drain_bus();
    let types: Vec<String> = bus
        .iter()
        .map(|e| {
            serde_json::to_value(e).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(
        types,
        [
            "device:added",
            "device:state",
            "device:output",
            "device:state",
            "device:removed"
        ]
    );
    insta::assert_json_snapshot!("fr_02_bus_events", bus);
}

#[test]
fn fr_02_bus_is_bounded() {
    let clock = FakeClock::new();
    let mut registry = Registry::with_config(
        clock,
        hardless_core::RegistryConfig {
            bus_capacity: 3,
            ..Default::default()
        },
    );
    registry.register(SamplePlugin).unwrap();
    for n in 0..5 {
        registry
            .create(KIND, Some(id(&format!("p{n}"))), json!({}))
            .unwrap();
    }
    let bus = registry.drain_bus();
    assert_eq!(bus.len(), 3);
    assert!(matches!(&bus[0], BusEvent::Added { device_id, .. } if device_id == &id("p2")));
}
