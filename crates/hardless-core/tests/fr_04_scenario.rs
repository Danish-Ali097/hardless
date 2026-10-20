//! FR-04: the scenario runner.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

mod common;

use common::{id, SamplePlugin, KIND};
use hardless_core::{
    FakeClock, Millis, Progress, Registry, Scenario, ScenarioReport, ScenarioRun, StepStatus,
    EXPECT_POLL_MS,
};
use serde_json::{json, Value};

fn rig() -> (FakeClock, Registry) {
    let clock = FakeClock::new();
    let mut registry = Registry::new(clock.clone());
    registry.register(SamplePlugin).unwrap();
    registry.create(KIND, Some(id("p1")), json!({})).unwrap();
    (clock, registry)
}

fn scenario(steps: Value) -> Scenario {
    serde_json::from_value(json!({ "name": "test", "steps": steps })).unwrap()
}

/// Runs to the end, advancing the fake clock (and ticking devices) as the run asks.
fn run(clock: &FakeClock, registry: &mut Registry, scenario: Scenario) -> ScenarioReport {
    let mut run = ScenarioRun::new(scenario);
    for _ in 0..10_000 {
        match run.advance(registry).0 {
            Progress::Done(report) => return report,
            Progress::WaitUntil(until) => {
                clock.set(until);
                registry.tick();
            }
        }
    }
    panic!("the scenario did not finish");
}

fn statuses(report: &ScenarioReport) -> Vec<StepStatus> {
    report.steps.iter().map(|s| s.status).collect()
}

#[test]
fn fr_04_faults_and_expectations_run_in_order() {
    let (clock, mut registry) = rig();
    let report = run(
        &clock,
        &mut registry,
        scenario(json!([
            { "type": "setFault", "device": "p1", "fault": "paper-out" },
            { "type": "expect", "device": "p1", "state": { "paperOut": true } },
            { "type": "clearFault", "device": "p1", "fault": "paper-out" },
            { "type": "expect", "device": "p1", "state": { "paperOut": false, "jobs": 0 } }
        ])),
    );
    assert!(report.ok, "{report:?}");
    assert_eq!(report.name.as_deref(), Some("test"));
    assert_eq!(statuses(&report), [StepStatus::Passed; 4]);
    let names: Vec<&str> = report.steps.iter().map(|s| s.step.as_str()).collect();
    assert_eq!(names, ["setFault", "expect", "clearFault", "expect"]);
}

#[test]
fn fr_04_waits_are_measured_in_milliseconds() {
    let (clock, mut registry) = rig();
    let mut run = ScenarioRun::new(scenario(json!([
        { "type": "wait", "ms": 250 },
        { "type": "wait", "ms": 100 }
    ])));
    assert_eq!(
        run.advance(&mut registry).0,
        Progress::WaitUntil(Millis(250))
    );
    clock.set(Millis(249));
    assert_eq!(
        run.advance(&mut registry).0,
        Progress::WaitUntil(Millis(250))
    );
    clock.set(Millis(250));
    assert_eq!(
        run.advance(&mut registry).0,
        Progress::WaitUntil(Millis(350))
    );
    clock.set(Millis(400));
    let Progress::Done(report) = run.advance(&mut registry).0 else {
        panic!("done after the waits");
    };
    assert_eq!(report.steps[0].at_ms, 250);
    assert_eq!(report.steps[1].at_ms, 400);
    assert_eq!(report.duration_ms, 400);
}

#[test]
fn fr_04_a_failed_expectation_stops_the_run() {
    let (clock, mut registry) = rig();
    let report = run(
        &clock,
        &mut registry,
        scenario(json!([
            { "type": "expect", "device": "p1", "state": { "paperOut": true } },
            { "type": "setFault", "device": "p1", "fault": "paper-out" }
        ])),
    );
    assert!(!report.ok);
    assert_eq!(statuses(&report), [StepStatus::Failed, StepStatus::Skipped]);
    let message = report.steps[0].message.as_deref().unwrap();
    assert!(message.contains("paperOut"), "{message}");
    assert!(
        !registry.summary(&id("p1")).unwrap().state["paperOut"]
            .as_bool()
            .unwrap(),
        "skipped steps do not run"
    );
}

#[test]
fn fr_04_expectations_can_wait_for_a_receipt() {
    let (clock, mut registry) = rig();
    // The sample device closes a job 300 ms after its last byte.
    registry.host_bytes(&id("p1"), b"Total: 1,250").unwrap();
    let report = run(
        &clock,
        &mut registry,
        scenario(json!([
            { "type": "expect", "device": "p1", "receiptContains": "Total: 1,250", "jobs": 1, "withinMs": 1000 }
        ])),
    );
    assert!(report.ok, "{report:?}");
    let at = report.steps[0].at_ms;
    assert!((300..300 + EXPECT_POLL_MS).contains(&at), "{at}");

    let report = run(
        &clock,
        &mut registry,
        scenario(json!([
            { "type": "expect", "device": "p1", "receiptContains": "Change due", "withinMs": 100 }
        ])),
    );
    assert!(!report.ok);
    assert_eq!(report.duration_ms, 100, "gives up at the deadline");
}

#[test]
fn fr_04_unknown_devices_and_unsupported_steps_fail() {
    let (clock, mut registry) = rig();
    let report = run(
        &clock,
        &mut registry,
        scenario(json!([{ "type": "setFault", "device": "nope", "fault": "paper-out" }])),
    );
    assert!(report.steps[0].message.as_deref().unwrap().contains("nope"));
    // The sample device does not support scans: it says so, and the step fails.
    let report = run(
        &clock,
        &mut registry,
        scenario(json!([{ "type": "scan", "device": "p1", "code": "1" }])),
    );
    assert_eq!(statuses(&report), [StepStatus::Failed]);
}

#[test]
fn fr_04_scenarios_are_strict_json() {
    assert!(serde_json::from_value::<Scenario>(json!({ "steps": [{ "type": "dance" }] })).is_err());
    assert!(serde_json::from_value::<Scenario>(json!({ "steps": [], "extra": 1 })).is_err());
    let report = ScenarioReport {
        name: None,
        ok: true,
        steps: vec![],
        duration_ms: 5,
    };
    assert_eq!(
        serde_json::to_value(report).unwrap(),
        json!({ "ok": true, "steps": [], "durationMs": 5 })
    );
}
