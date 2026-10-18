//! Fault catalogue (PRD section 10) and identifier validation.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use hardless_core::{catalogue, DeviceId, FaultId, OptionDefault};

/// Every fault ID in PRD section 10, in table order.
const PRD_SECTION_10: [&str; 19] = [
    "paper-out",
    "paper-near-end",
    "cover-open",
    "offline",
    "disconnect-mid-write",
    "slow-write",
    "baud-mismatch",
    "garbled",
    "timeout",
    "cors-blocked",
    "stall",
    "interface-busy",
    "picker-cancel",
    "permission-denied",
    "read-error",
    "jammed",
    "not-connected",
    "double-scan",
    "partial-scan",
];

#[test]
fn faults_catalogue_matches_prd_section_10() {
    let ids: Vec<_> = catalogue().iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, PRD_SECTION_10);
    assert!(catalogue()
        .iter()
        .all(|f| !f.applies_to.is_empty() && !f.effect.is_empty()));
}

#[test]
fn faults_catalogue_option_defaults_match_prd_section_10() {
    let default = |fault: &str, option: &str| {
        let spec = FaultId::lookup(fault).expect(fault);
        spec.options
            .iter()
            .find(|o| o.name == option)
            .expect(option)
            .default
    };
    assert_eq!(
        default("disconnect-mid-write", "afterBytes"),
        OptionDefault::Integer(64)
    );
    assert_eq!(
        default("slow-write", "msPerChunk"),
        OptionDefault::Integer(200)
    );
    assert_eq!(default("garbled", "rate"), OptionDefault::Number(0.05));
    assert_eq!(default("double-scan", "gapMs"), OptionDefault::Integer(30));
    let with_options = catalogue().iter().filter(|f| !f.options.is_empty()).count();
    assert_eq!(with_options, 4);
}

#[test]
fn fault_ids_are_kebab_case_and_open_to_plugins() {
    let custom: FaultId = "label-gap-lost".parse().unwrap();
    assert_eq!(custom.as_str(), "label-gap-lost");
    assert!(FaultId::lookup("label-gap-lost").is_none());
    for bad in [
        "",
        "Paper-Out",
        "paper_out",
        "-paper",
        "paper-",
        "paper--out",
    ] {
        assert!(
            bad.parse::<FaultId>().is_err(),
            "{bad:?} should be rejected"
        );
    }
    assert_eq!(
        serde_json::to_string(&FaultId::PAPER_OUT).unwrap(),
        "\"paper-out\""
    );
    assert!(serde_json::from_str::<FaultId>("\"Bad\"").is_err());
}

#[test]
fn device_ids_are_url_safe() {
    for good in ["printer-1", "drawer_1", "a", "Printer.2"] {
        assert!(good.parse::<DeviceId>().is_ok(), "{good:?}");
    }
    let too_long = "x".repeat(65);
    for bad in ["", "printer 1", "printer/1", "ünicode", too_long.as_str()] {
        assert!(
            bad.parse::<DeviceId>().is_err(),
            "{bad:?} should be rejected"
        );
    }
    assert!(serde_json::from_str::<DeviceId>("\"a/b\"").is_err());
}
