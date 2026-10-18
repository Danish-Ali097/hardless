//! Faults: [`FaultId`], [`FaultOptions`] and the catalogue from PRD section 10.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::ids::InvalidId;

/// Options for a fault, as a JSON object (e.g. `{ "afterBytes": 64 }`).
pub type FaultOptions = serde_json::Map<String, serde_json::Value>;

/// A fault identifier in kebab-case, e.g. `paper-out`.
///
/// The IDs from PRD section 10 are associated constants. Plugins may define their own IDs;
/// any lowercase kebab-case string is accepted.
///
/// ```
/// use hardless_core::FaultId;
/// let id: FaultId = "paper-out".parse().unwrap();
/// assert_eq!(id, FaultId::PAPER_OUT);
/// assert!("Paper_Out".parse::<FaultId>().is_err());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(try_from = "String", into = "String")]
pub struct FaultId(#[cfg_attr(feature = "ts", ts(as = "String"))] Cow<'static, str>);

macro_rules! fault_ids {
    ($($(#[$doc:meta])* $name:ident = $id:literal;)*) => {
        impl FaultId {
            $($(#[$doc])* pub const $name: FaultId = FaultId(Cow::Borrowed($id));)*
        }
    };
}

fault_ids! {
    /// Printers: prints rejected; status reports paper end.
    PAPER_OUT = "paper-out";
    /// Printers: prints succeed; status reports near end.
    PAPER_NEAR_END = "paper-near-end";
    /// Printers: prints rejected; status reports cover open.
    COVER_OPEN = "cover-open";
    /// Printers: serial/USB disconnect; HTTP refuses connections.
    OFFLINE = "offline";
    /// Serial and USB printers: disconnect after `afterBytes` bytes of the next job.
    DISCONNECT_MID_WRITE = "disconnect-mid-write";
    /// Serial and USB printers: delay each chunk acknowledgement by `msPerChunk`.
    SLOW_WRITE = "slow-write";
    /// Serial printers: received bytes are scrambled.
    BAUD_MISMATCH = "baud-mismatch";
    /// All printers: random byte corruption at `rate`.
    GARBLED = "garbled";
    /// HTTP printers: response delayed past the request's `timeout`.
    TIMEOUT = "timeout";
    /// HTTP printers: CORS headers omitted.
    CORS_BLOCKED = "cors-blocked";
    /// USB printers: `transferOut` returns `status: "stall"`.
    STALL = "stall";
    /// USB printers: `claimInterface` rejects.
    INTERFACE_BUSY = "interface-busy";
    /// Serial, USB and NFC: the next `request*` call behaves as a user cancel.
    PICKER_CANCEL = "picker-cancel";
    /// NFC: `scan()` rejects with `NotAllowedError`.
    PERMISSION_DENIED = "permission-denied";
    /// NFC: the next tap fires `readingerror`.
    READ_ERROR = "read-error";
    /// Drawer: kick received, drawer stays closed.
    JAMMED = "jammed";
    /// Drawer: kick ignored; status shows no drawer.
    NOT_CONNECTED = "not-connected";
    /// Scanner: each scan is sent twice, `gapMs` apart.
    DOUBLE_SCAN = "double-scan";
    /// Scanner: terminator dropped.
    PARTIAL_SCAN = "partial-scan";
}

impl FaultId {
    /// The ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The catalogue entry for this ID, if it is one of the built-in faults.
    ///
    /// ```
    /// use hardless_core::FaultId;
    /// assert_eq!(FaultId::lookup("garbled").unwrap().options[0].name, "rate");
    /// assert!(FaultId::lookup("made-up").is_none());
    /// ```
    pub fn lookup(id: &str) -> Option<&'static FaultSpec> {
        CATALOGUE.iter().find(|spec| spec.id.as_str() == id)
    }
}

fn is_kebab_case(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && !s.starts_with('-')
        && !s.ends_with('-')
        && !s.contains("--")
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl FromStr for FaultId {
    type Err = InvalidId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(spec) = FaultId::lookup(s) {
            return Ok(spec.id.clone());
        }
        if is_kebab_case(s) {
            Ok(FaultId(Cow::Owned(s.to_owned())))
        } else {
            Err(InvalidId {
                what: "fault id",
                value: s.to_owned(),
            })
        }
    }
}

impl TryFrom<String> for FaultId {
    type Error = InvalidId;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<FaultId> for String {
    fn from(id: FaultId) -> String {
        id.0.into_owned()
    }
}

impl fmt::Display for FaultId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl schemars::JsonSchema for FaultId {
    fn schema_name() -> Cow<'static, str> {
        "FaultId".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^[a-z0-9]+(-[a-z0-9]+)*$",
            "maxLength": 64
        })
    }
}

/// The default value of a fault option.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(untagged)]
pub enum OptionDefault {
    /// A whole number, e.g. a byte count or milliseconds.
    Integer(u64),
    /// A real number, e.g. a rate between 0 and 1.
    Number(f64),
}

/// One option a fault accepts.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FaultOptionSpec {
    /// Option name in the options object, e.g. `afterBytes`.
    pub name: &'static str,
    /// Value used when the option is not given.
    pub default: OptionDefault,
    /// What the option controls.
    pub description: &'static str,
}

/// A catalogue entry: one row of PRD section 10.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct FaultSpec {
    /// The fault ID.
    pub id: FaultId,
    /// The devices it applies to, as written in the catalogue (e.g. `serial, USB printers`).
    pub applies_to: &'static str,
    /// The observable effect.
    pub effect: &'static str,
    /// Accepted options, empty if none.
    pub options: &'static [FaultOptionSpec],
}

const fn spec(
    id: FaultId,
    applies_to: &'static str,
    effect: &'static str,
    options: &'static [FaultOptionSpec],
) -> FaultSpec {
    FaultSpec {
        id,
        applies_to,
        effect,
        options,
    }
}

const fn opt(
    name: &'static str,
    default: OptionDefault,
    description: &'static str,
) -> FaultOptionSpec {
    FaultOptionSpec {
        name,
        default,
        description,
    }
}

static CATALOGUE: [FaultSpec; 19] = [
    spec(
        FaultId::PAPER_OUT,
        "printers",
        "Prints rejected; status reports paper end",
        &[],
    ),
    spec(
        FaultId::PAPER_NEAR_END,
        "printers",
        "Prints succeed; status reports near end",
        &[],
    ),
    spec(
        FaultId::COVER_OPEN,
        "printers",
        "Prints rejected; status reports cover open",
        &[],
    ),
    spec(
        FaultId::OFFLINE,
        "printers",
        "Serial/USB: device disconnects. HTTP: connection refused",
        &[],
    ),
    spec(
        FaultId::DISCONNECT_MID_WRITE,
        "serial, USB printers",
        "Disconnects after N bytes of the next job",
        &[opt(
            "afterBytes",
            OptionDefault::Integer(64),
            "Bytes of the next job before the disconnect",
        )],
    ),
    spec(
        FaultId::SLOW_WRITE,
        "serial, USB printers",
        "Delays each chunk ack",
        &[opt(
            "msPerChunk",
            OptionDefault::Integer(200),
            "Delay before each chunk is acknowledged",
        )],
    ),
    spec(
        FaultId::BAUD_MISMATCH,
        "serial printers",
        "Received bytes are scrambled",
        &[],
    ),
    spec(
        FaultId::GARBLED,
        "all printers",
        "Random byte corruption",
        &[opt(
            "rate",
            OptionDefault::Number(0.05),
            "Probability (0 to 1) that a byte is corrupted",
        )],
    ),
    spec(
        FaultId::TIMEOUT,
        "HTTP printers",
        "Response delayed past the request's timeout",
        &[],
    ),
    spec(
        FaultId::CORS_BLOCKED,
        "HTTP printers",
        "CORS headers omitted",
        &[],
    ),
    spec(
        FaultId::STALL,
        "USB printers",
        "transferOut returns status: \"stall\"",
        &[],
    ),
    spec(
        FaultId::INTERFACE_BUSY,
        "USB printers",
        "claimInterface rejects",
        &[],
    ),
    spec(
        FaultId::PICKER_CANCEL,
        "serial, USB, NFC",
        "Next request* call behaves as user cancel",
        &[],
    ),
    spec(
        FaultId::PERMISSION_DENIED,
        "NFC",
        "scan() rejects NotAllowedError",
        &[],
    ),
    spec(
        FaultId::READ_ERROR,
        "NFC",
        "Next tap fires readingerror",
        &[],
    ),
    spec(
        FaultId::JAMMED,
        "drawer",
        "Kick received, drawer stays closed",
        &[],
    ),
    spec(
        FaultId::NOT_CONNECTED,
        "drawer",
        "Kick ignored; status shows no drawer",
        &[],
    ),
    spec(
        FaultId::DOUBLE_SCAN,
        "scanner",
        "Each scan is sent twice",
        &[opt(
            "gapMs",
            OptionDefault::Integer(30),
            "Delay between the two scans",
        )],
    ),
    spec(FaultId::PARTIAL_SCAN, "scanner", "Terminator dropped", &[]),
];

/// The built-in fault catalogue (PRD section 10), in table order.
///
/// ```
/// let ids: Vec<_> = hardless_core::catalogue().iter().map(|f| f.id.as_str()).collect();
/// assert_eq!(ids[0], "paper-out");
/// ```
pub fn catalogue() -> &'static [FaultSpec] {
    &CATALOGUE
}
