//! Identifiers: [`DeviceId`] and [`JobId`].

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

/// Longest accepted [`DeviceId`], in bytes.
pub const MAX_DEVICE_ID_LEN: usize = 64;

/// The ID of a device instance, e.g. `printer-1`.
///
/// IDs appear in URL paths (`/api/v1/devices/:id`), so they are limited to 1–64 ASCII
/// letters, digits, `-`, `_` and `.`.
///
/// ```
/// use hardless_core::DeviceId;
/// let id: DeviceId = "printer-1".parse().unwrap();
/// assert_eq!(id.as_str(), "printer-1");
/// assert!("printer 1".parse::<DeviceId>().is_err());
/// ```
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(try_from = "String", into = "String")]
pub struct DeviceId(String);

impl DeviceId {
    /// The ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Error returned when a string is not a valid [`DeviceId`] or [`FaultId`](crate::FaultId).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidId {
    pub(crate) what: &'static str,
    pub(crate) value: String,
}

impl fmt::Display for InvalidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {}: {:?}", self.what, self.value)
    }
}

impl std::error::Error for InvalidId {}

impl FromStr for DeviceId {
    type Err = InvalidId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let valid = !s.is_empty()
            && s.len() <= MAX_DEVICE_ID_LEN
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
        if valid {
            Ok(DeviceId(s.to_owned()))
        } else {
            Err(InvalidId {
                what: "device id",
                value: s.to_owned(),
            })
        }
    }
}

impl TryFrom<String> for DeviceId {
    type Error = InvalidId;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<DeviceId> for String {
    fn from(id: DeviceId) -> String {
        id.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for DeviceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// The ID of a print job, unique per device until the device is reset. Devices number their
/// jobs from 1.
///
/// ```
/// use hardless_core::JobId;
/// assert_eq!(serde_json::to_string(&JobId(3)).unwrap(), "3");
/// ```
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(transparent)]
pub struct JobId(pub u64);

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
