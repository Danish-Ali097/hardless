//! FR-36 to FR-39: the cash drawer.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

mod common;

use common::{id, Rig, DRAWER, PRINTER};
use hardless_core::{ControlCommand, DevicePlugin, FaultId};
use hardless_devices::drawer::DrawerPlugin;
use serde_json::json;

/// `ESC p 0 25 250`: pin 2, 50 ms on, 500 ms off.
const KICK_PIN2: &[u8] = b"\x1bp\x00\x19\xfa";
/// `ESC p 1 50 50`: pin 5, 100 ms on.
const KICK_PIN5: &[u8] = b"\x1bp\x01\x32\x32";

#[test]
fn fr_36_drawer_attaches_to_a_printer_and_opens_on_its_pin() {
    let mut rig = Rig::new();
    assert_eq!(
        rig.registry.summary(&id(DRAWER)).unwrap().attached_to,
        Some(id(PRINTER))
    );
    rig.print(KICK_PIN2);
    assert_eq!(rig.state(DRAWER)["state"], "opening");
    assert_eq!(rig.state(DRAWER)["kicks"], 1);
}

#[test]
fn fr_36_kicks_on_the_other_pin_are_ignored() {
    let mut rig = Rig::new();
    rig.print(KICK_PIN5);
    assert_eq!(rig.state(DRAWER)["state"], "closed");
    assert_eq!(rig.state(DRAWER)["kicks"], 0);

    let mut rig5 = Rig::with(json!({}), json!({ "attachedTo": PRINTER, "pin": 5 }));
    rig5.print(KICK_PIN5);
    assert_eq!(rig5.state(DRAWER)["state"], "opening");
}

#[test]
fn fr_36_realtime_kick_also_opens_the_drawer() {
    let mut rig = Rig::new();
    rig.print(b"\x10\x14\x01\x00\x01"); // DLE DC4 1, pin 2, 100 ms
    assert_eq!(rig.state(DRAWER)["state"], "opening");
}

#[test]
fn fr_36_drawer_config_is_validated() {
    let plugin = DrawerPlugin;
    assert!(
        plugin.create(id("d"), json!({})).is_err(),
        "attachedTo is required"
    );
    assert!(plugin
        .create(id("d"), json!({ "attachedTo": "p", "pin": 3 }))
        .is_err());
    assert!(plugin
        .create(id("d"), json!({ "attachedTo": "d" }))
        .is_err());
}

#[test]
fn fr_37_opening_lasts_the_pulse_on_time_then_the_drawer_is_open() {
    let mut rig = Rig::new();
    rig.print(KICK_PIN5_AS_PIN2_100MS);
    rig.advance(99);
    assert_eq!(rig.state(DRAWER)["state"], "opening");
    rig.advance(1);
    assert_eq!(rig.state(DRAWER)["state"], "open");
    assert_eq!(rig.state(DRAWER)["opened"], 1);
}

/// `ESC p 0 50 50`: pin 2, 100 ms on.
const KICK_PIN5_AS_PIN2_100MS: &[u8] = b"\x1bp\x00\x32\x32";

#[test]
fn fr_37_opening_takes_at_least_50ms() {
    let mut rig = Rig::new();
    rig.print(b"\x1bp\x00\x01\x01"); // 2 ms on
    rig.advance(49);
    assert_eq!(rig.state(DRAWER)["state"], "opening");
    rig.advance(1);
    assert_eq!(rig.state(DRAWER)["state"], "open");
}

#[test]
fn fr_37_closes_by_hand() {
    let mut rig = Rig::new();
    rig.print(KICK_PIN2);
    rig.advance(50);
    rig.registry
        .control(&id(DRAWER), ControlCommand::CloseDrawer)
        .unwrap();
    assert_eq!(rig.state(DRAWER)["state"], "closed");
}

#[test]
fn fr_37_auto_close_is_off_by_default_and_configurable() {
    let mut rig = Rig::new();
    rig.print(KICK_PIN2);
    rig.advance(50);
    rig.advance(3_600_000);
    assert_eq!(
        rig.state(DRAWER)["state"],
        "open",
        "stays open without autoCloseMs"
    );

    let mut rig = Rig::with(
        json!({}),
        json!({ "attachedTo": PRINTER, "autoCloseMs": 2_000 }),
    );
    rig.print(KICK_PIN2);
    rig.advance(50);
    rig.advance(1_999);
    assert_eq!(rig.state(DRAWER)["state"], "open");
    rig.advance(1);
    assert_eq!(rig.state(DRAWER)["state"], "closed");
}

#[test]
fn fr_37_kicks_while_open_are_counted() {
    let mut rig = Rig::new();
    rig.print(KICK_PIN2);
    rig.advance(50);
    rig.print(KICK_PIN2);
    assert_eq!(rig.state(DRAWER)["kicks"], 2);
    assert_eq!(rig.state(DRAWER)["opened"], 1);
}

#[test]
fn fr_38_printer_status_reports_the_drawer_on_bit_2() {
    let mut rig = Rig::new();
    assert_eq!(rig.status(1) & 0x04, 0, "closed");
    rig.print(KICK_PIN2);
    assert_eq!(rig.status(1) & 0x04, 0, "still opening");
    rig.advance(50);
    assert_eq!(rig.status(1) & 0x04, 0x04, "open");
    assert_eq!(rig.state(PRINTER)["drawerOpen"], true);
    rig.registry
        .control(&id(DRAWER), ControlCommand::CloseDrawer)
        .unwrap();
    assert_eq!(rig.status(1) & 0x04, 0, "closed again");
}

#[test]
fn fr_39_jammed_drawer_counts_the_kick_but_stays_closed() {
    let mut rig = Rig::new();
    rig.fault(DRAWER, FaultId::JAMMED, true);
    rig.print(KICK_PIN2);
    rig.advance(1_000);
    assert_eq!(rig.state(DRAWER)["state"], "closed");
    assert_eq!(rig.state(DRAWER)["kicks"], 1);
    assert_eq!(rig.status(1) & 0x04, 0);
}

#[test]
fn fr_39_not_connected_drawer_ignores_kicks_and_reads_closed() {
    let mut rig = Rig::new();
    rig.print(KICK_PIN2);
    rig.advance(50);
    assert_eq!(rig.status(1) & 0x04, 0x04);

    rig.fault(DRAWER, FaultId::NOT_CONNECTED, true);
    assert_eq!(
        rig.status(1) & 0x04,
        0,
        "an unplugged drawer reads as closed"
    );
    assert_eq!(rig.state(DRAWER)["connected"], false);

    rig.registry
        .control(&id(DRAWER), ControlCommand::CloseDrawer)
        .unwrap();
    rig.print(KICK_PIN2);
    rig.advance(1_000);
    assert_eq!(rig.state(DRAWER)["state"], "closed");
    assert_eq!(
        rig.state(DRAWER)["kicks"],
        1,
        "the ignored kick is not counted"
    );
}

#[test]
fn drawer_reset_closes_it_and_tells_the_printer() {
    let mut rig = Rig::new();
    rig.print(KICK_PIN2);
    rig.advance(50);
    rig.registry.reset(&id(DRAWER)).unwrap();
    assert_eq!(rig.state(DRAWER)["state"], "closed");
    assert_eq!(rig.state(DRAWER)["kicks"], 0);
    assert_eq!(rig.status(1) & 0x04, 0);
}
