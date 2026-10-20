//! The scenario runner (FR-04): an ordered list of steps (`scan`, `tap`, `wait`, `setFault`,
//! `clearFault`, `expect`) run against a [`Registry`].
//!
//! Sans-IO like the rest of the core: [`ScenarioRun::advance`] runs every step it can at the
//! registry's current time and says when to call it again. The server (or a test with a fake
//! clock) does the waiting.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::device::{ControlCommand, DeviceEvent, Terminator};
use crate::fault::{FaultId, FaultOptions};
use crate::ids::DeviceId;
use crate::nfc::NdefRecord;
use crate::registry::Registry;
use crate::time::Millis;

/// Most steps a scenario may have.
pub const MAX_SCENARIO_STEPS: usize = 10_000;
/// How often an `expect` with `withinMs` is checked again, in milliseconds.
pub const EXPECT_POLL_MS: u64 = 20;

/// A scenario: steps run in order, stopping at the first that fails.
///
/// ```
/// use hardless_core::Scenario;
/// let scenario: Scenario = serde_json::from_value(serde_json::json!({
///     "name": "paper runs out",
///     "steps": [
///         { "type": "scan", "device": "scanner-1", "code": "4006381333931" },
///         { "type": "wait", "ms": 200 },
///         { "type": "setFault", "device": "printer-1", "fault": "paper-out" },
///         { "type": "expect", "device": "printer-1", "state": { "paperOut": true } }
///     ]
/// })).unwrap();
/// assert_eq!(scenario.steps.len(), 4);
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scenario {
    /// A name for reports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name: Option<String>,
    /// The steps, in order.
    pub steps: Vec<Step>,
}

/// One scenario step.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Step {
    /// Scan a barcode on a keyboard-wedge scanner.
    #[serde(rename_all = "camelCase")]
    Scan {
        /// The scanner.
        device: DeviceId,
        /// The barcode.
        code: String,
        /// Overrides the scanner's terminator.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        terminator: Option<Terminator>,
        /// Overrides the scanner's inter-key delay.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        delay_ms: Option<u64>,
    },
    /// Tap an NFC card.
    Tap {
        /// The NFC reader.
        device: DeviceId,
        /// Card UID as hex.
        uid: String,
        /// NDEF records; without them the reader uses what it remembers of the card.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        records: Option<Vec<NdefRecord>>,
    },
    /// Wait.
    Wait {
        /// Milliseconds to wait.
        ms: u64,
    },
    /// Turn a fault on.
    SetFault {
        /// The device.
        device: DeviceId,
        /// The fault.
        fault: FaultId,
        /// Its options.
        #[serde(default)]
        #[cfg_attr(
            feature = "ts",
            ts(as = "std::collections::HashMap<String, serde_json::Value>")
        )]
        options: FaultOptions,
    },
    /// Turn a fault off.
    ClearFault {
        /// The device.
        device: DeviceId,
        /// The fault.
        fault: FaultId,
    },
    /// Check a device. Every given check must hold; with `withinMs`, they may become true
    /// within that time (e.g. a receipt the app prints in response to a scan).
    #[serde(rename_all = "camelCase")]
    Expect {
        /// The device.
        device: DeviceId,
        /// Fields the device state must have (a subset match: nested objects compare by the
        /// keys given).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        state: Option<Value>,
        /// Text the latest job must contain.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        receipt_contains: Option<String>,
        /// How many jobs the device must hold.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        jobs: Option<u32>,
        /// How long the checks may take to become true, in milliseconds (default 0).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        within_ms: Option<u64>,
    },
}

impl Step {
    /// The step's `type`, e.g. `"setFault"`.
    pub fn name(&self) -> &'static str {
        match self {
            Step::Scan { .. } => "scan",
            Step::Tap { .. } => "tap",
            Step::Wait { .. } => "wait",
            Step::SetFault { .. } => "setFault",
            Step::ClearFault { .. } => "clearFault",
            Step::Expect { .. } => "expect",
        }
    }
}

/// How a step went.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    /// It ran and its checks held.
    Passed,
    /// It failed; the scenario stopped here.
    Failed,
    /// It did not run because an earlier step failed.
    Skipped,
}

/// The result of one step.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    /// Position in the scenario, from 0.
    pub index: u32,
    /// The step's `type`.
    pub step: String,
    /// How it went.
    pub status: StepStatus,
    /// When it finished, in milliseconds from the start of the run.
    pub at_ms: u64,
    /// Why it failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub message: Option<String>,
}

/// The result of a whole run (`POST /scenarios/run`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ScenarioReport {
    /// The scenario's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name: Option<String>,
    /// Whether every step passed.
    pub ok: bool,
    /// One result per step.
    pub steps: Vec<StepResult>,
    /// How long the run took, in milliseconds.
    pub duration_ms: u64,
}

/// What [`ScenarioRun::advance`] wants next.
#[derive(Clone, Debug, PartialEq)]
pub enum Progress {
    /// Call `advance` again at or after this time.
    WaitUntil(Millis),
    /// The run is over.
    Done(ScenarioReport),
}

/// A scenario being run.
#[derive(Clone, Debug)]
pub struct ScenarioRun {
    scenario: Scenario,
    index: usize,
    started: Option<Millis>,
    /// The end of the current `wait`, or the deadline of the current `expect`.
    until: Option<Millis>,
    results: Vec<StepResult>,
}

impl ScenarioRun {
    /// A run that has not started.
    pub fn new(scenario: Scenario) -> Self {
        Self {
            scenario,
            index: 0,
            started: None,
            until: None,
            results: Vec::new(),
        }
    }

    /// Runs every step it can now. Device events that need a transport (wedge scans, bytes
    /// for the app, connects and disconnects) are returned for the caller to deliver.
    pub fn advance(&mut self, registry: &mut Registry) -> (Progress, Vec<(DeviceId, DeviceEvent)>) {
        let now = registry.now();
        let started = *self.started.get_or_insert(now);
        let mut transport = Vec::new();
        while let Some(step) = self.scenario.steps.get(self.index).cloned() {
            let outcome = match &step {
                Step::Wait { ms } => {
                    let until = *self.until.get_or_insert(now + *ms);
                    if now < until {
                        return (Progress::WaitUntil(until), transport);
                    }
                    Ok(())
                }
                Step::Expect {
                    device,
                    state,
                    receipt_contains,
                    jobs,
                    within_ms,
                } => match check(
                    registry,
                    device,
                    state.as_ref(),
                    receipt_contains.as_deref(),
                    *jobs,
                ) {
                    Ok(()) => Ok(()),
                    Err(message) => {
                        let deadline = *self.until.get_or_insert(now + within_ms.unwrap_or(0));
                        if now < deadline {
                            let next = (now + EXPECT_POLL_MS).min(deadline);
                            return (Progress::WaitUntil(next), transport);
                        }
                        Err(message)
                    }
                },
                Step::Scan {
                    device,
                    code,
                    terminator,
                    delay_ms,
                } => control(
                    registry,
                    device,
                    ControlCommand::Scan {
                        code: code.clone(),
                        terminator: *terminator,
                        delay_ms: *delay_ms,
                    },
                    &mut transport,
                ),
                Step::Tap {
                    device,
                    uid,
                    records,
                } => control(
                    registry,
                    device,
                    ControlCommand::Tap {
                        uid: uid.clone(),
                        records: records.clone(),
                    },
                    &mut transport,
                ),
                Step::SetFault {
                    device,
                    fault,
                    options,
                } => fault_step(
                    registry,
                    device,
                    fault,
                    true,
                    options.clone(),
                    &mut transport,
                ),
                Step::ClearFault { device, fault } => fault_step(
                    registry,
                    device,
                    fault,
                    false,
                    FaultOptions::new(),
                    &mut transport,
                ),
            };
            self.until = None;
            let at_ms = now.since(started);
            let failed = outcome.is_err();
            self.results.push(StepResult {
                index: index_u32(self.index),
                step: step.name().to_owned(),
                status: if failed {
                    StepStatus::Failed
                } else {
                    StepStatus::Passed
                },
                at_ms,
                message: outcome.err(),
            });
            self.index += 1;
            if failed {
                break;
            }
        }
        // Whatever is left was skipped.
        for (i, step) in self.scenario.steps.iter().enumerate().skip(self.index) {
            self.results.push(StepResult {
                index: index_u32(i),
                step: step.name().to_owned(),
                status: StepStatus::Skipped,
                at_ms: now.since(started),
                message: None,
            });
        }
        self.index = self.scenario.steps.len();
        let report = ScenarioReport {
            name: self.scenario.name.clone(),
            ok: self.results.iter().all(|r| r.status == StepStatus::Passed),
            steps: std::mem::take(&mut self.results),
            duration_ms: now.since(started),
        };
        (Progress::Done(report), transport)
    }
}

fn index_u32(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

/// Whether an event needs a transport to reach the app.
fn for_transport(event: &DeviceEvent) -> bool {
    matches!(
        event,
        DeviceEvent::ToHost(_)
            | DeviceEvent::Wedge(_)
            | DeviceEvent::Nfc(_)
            | DeviceEvent::Connected
            | DeviceEvent::Disconnected
    )
}

fn control(
    registry: &mut Registry,
    device: &DeviceId,
    command: ControlCommand,
    transport: &mut Vec<(DeviceId, DeviceEvent)>,
) -> Result<(), String> {
    let events = registry
        .control(device, command)
        .map_err(|e| e.to_string())?;
    if let Some(DeviceEvent::Log(entry)) = events
        .iter()
        .find(|e| matches!(e, DeviceEvent::Log(entry) if entry.level >= crate::LogLevel::Warn))
    {
        // The device refused (an empty code, a command it does not support).
        return Err(entry.message.clone());
    }
    transport.extend(
        events
            .into_iter()
            .filter(for_transport)
            .map(|e| (device.clone(), e)),
    );
    Ok(())
}

fn fault_step(
    registry: &mut Registry,
    device: &DeviceId,
    fault: &FaultId,
    active: bool,
    options: FaultOptions,
    transport: &mut Vec<(DeviceId, DeviceEvent)>,
) -> Result<(), String> {
    let events = registry
        .set_fault(device, fault.clone(), active, options)
        .map_err(|e| e.to_string())?;
    transport.extend(
        events
            .into_iter()
            .filter(for_transport)
            .map(|e| (device.clone(), e)),
    );
    Ok(())
}

fn check(
    registry: &Registry,
    device: &DeviceId,
    state: Option<&Value>,
    receipt_contains: Option<&str>,
    jobs: Option<u32>,
) -> Result<(), String> {
    let summary = registry
        .summary(device)
        .ok_or_else(|| format!("device `{device}` not found"))?;
    if let Some(expected) = state {
        if !subset(expected, &summary.state) {
            return Err(format!("state {} does not match {expected}", summary.state));
        }
    }
    if let Some(count) = jobs {
        let held = registry.jobs(device).map(Iterator::count).unwrap_or(0);
        if held != count as usize {
            return Err(format!("{held} jobs, expected {count}"));
        }
    }
    if let Some(text) = receipt_contains {
        let last = registry.last_job(device).ok().flatten();
        match last {
            Some(job) if job.text.contains(text) => {}
            Some(job) => {
                return Err(format!(
                    "the latest receipt does not contain {text:?}: {:?}",
                    job.text
                ))
            }
            None => return Err(format!("no receipt yet (expected one containing {text:?})")),
        }
    }
    Ok(())
}

/// Whether `expected` matches `actual`: objects by the keys `expected` gives, anything else
/// by equality.
fn subset(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Object(want), Value::Object(have)) => want
            .iter()
            .all(|(k, v)| have.get(k).is_some_and(|h| subset(v, h))),
        _ => expected == actual,
    }
}

#[cfg(test)]
mod tests {
    use super::subset;
    use serde_json::json;

    #[test]
    fn subset_matches_nested_keys() {
        let actual = json!({ "a": 1, "b": { "c": 2, "d": 3 }, "e": [1, 2] });
        assert!(subset(&json!({ "b": { "c": 2 } }), &actual));
        assert!(subset(&json!({ "e": [1, 2] }), &actual));
        assert!(!subset(&json!({ "e": [1] }), &actual));
        assert!(!subset(&json!({ "z": null }), &actual));
        assert!(!subset(&json!({ "a": "1" }), &actual));
    }
}
