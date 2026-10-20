//! NFC wire types (PRD section 8.3): NDEF records, card readings and what an NFC reader
//! reports in [`DeviceEvent::Nfc`](crate::DeviceEvent::Nfc).
//!
//! Records follow Web NFC's `NDEFRecordInit` field names, so the shim can hand them to the
//! app as `NDEFRecord`s without translation. Text payloads travel as `data`, binary ones as
//! `dataBase64`.

use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// Most records in one NDEF message. Real tags hold a handful; this only stops runaway input.
pub const MAX_NDEF_RECORDS: usize = 64;

/// Most payload bytes in one NDEF message. The largest common tags (NTAG216) hold 888 bytes;
/// this only stops runaway input.
pub const MAX_NDEF_BYTES: usize = 32 * 1024;

/// One NDEF record, as Web NFC's `NDEFRecordInit`.
///
/// ```
/// use hardless_core::NdefRecord;
/// let text: NdefRecord =
///     serde_json::from_str(r#"{ "recordType": "text", "data": "Hello", "lang": "en" }"#).unwrap();
/// assert_eq!(text.payload(), b"Hello");
/// let png: NdefRecord = serde_json::from_str(
///     r#"{ "recordType": "mime", "mediaType": "image/png", "dataBase64": "iVBORw==" }"#,
/// ).unwrap();
/// assert_eq!(png.payload(), b"\x89PNG");
/// assert!(text.validate().is_ok() && png.validate().is_ok());
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NdefRecord {
    /// `empty`, `text`, `url`, `absolute-url`, `mime`, `unknown`, or an external type such as
    /// `example.com:loyalty`.
    pub record_type: String,
    /// The MIME type of a `mime` record, e.g. `application/json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub media_type: Option<String>,
    /// The record identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub id: Option<String>,
    /// The text encoding of a `text` record: `utf-8` (default), `utf-16`, `utf-16be` or
    /// `utf-16le`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub encoding: Option<String>,
    /// The language of a `text` record, e.g. `en`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub lang: Option<String>,
    /// A text payload: the text of a `text` record, the URL of a `url` record, or any payload
    /// that is UTF-8 text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub data: Option<String>,
    /// A binary payload, base64. At most one of `data` and `dataBase64` is set.
    #[serde(
        default,
        rename = "dataBase64",
        skip_serializing_if = "Option::is_none",
        with = "opt_base64"
    )]
    #[cfg_attr(feature = "ts", ts(rename = "dataBase64", optional, type = "string"))]
    #[schemars(with = "Option<String>")]
    pub data_base64: Option<Bytes>,
}

impl NdefRecord {
    /// A `text` record.
    ///
    /// ```
    /// assert_eq!(hardless_core::NdefRecord::text("Hi").record_type, "text");
    /// ```
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            record_type: "text".into(),
            data: Some(text.into()),
            ..Self::default()
        }
    }

    /// A `url` record.
    pub fn url(url: impl Into<String>) -> Self {
        Self {
            record_type: "url".into(),
            data: Some(url.into()),
            ..Self::default()
        }
    }

    /// The payload bytes: `data` as UTF-8, or the decoded `dataBase64`.
    pub fn payload(&self) -> Vec<u8> {
        match (&self.data, &self.data_base64) {
            (Some(text), _) => text.as_bytes().to_vec(),
            (None, Some(bytes)) => bytes.to_vec(),
            (None, None) => Vec::new(),
        }
    }

    /// Checks the record the way Web NFC checks an `NDEFRecordInit` before writing it.
    pub fn validate(&self) -> Result<(), String> {
        let kind = self.record_type.as_str();
        if kind.is_empty() || kind.len() > 255 {
            return Err("recordType must be 1 to 255 characters".into());
        }
        if self.data.is_some() && self.data_base64.is_some() {
            return Err("set either data or dataBase64, not both".into());
        }
        let has_data = self.data.is_some() || self.data_base64.is_some();
        match kind {
            "empty" if has_data || self.media_type.is_some() => {
                Err("an empty record has no data or mediaType".into())
            }
            "empty" | "mime" | "unknown" => Ok(()),
            "text" | "url" | "absolute-url" if !has_data => {
                Err(format!("a {kind} record needs data"))
            }
            "text" => match self.encoding.as_deref() {
                None | Some("utf-8" | "utf-16" | "utf-16be" | "utf-16le") => Ok(()),
                Some(other) => Err(format!("unsupported text encoding `{other}`")),
            },
            "url" | "absolute-url" => Ok(()),
            "smart-poster" => {
                Err("smart-poster records (nested messages) are not supported".into())
            }
            local if local.starts_with(':') => {
                Err("local record types are only allowed inside nested messages".into())
            }
            external if external.contains(':') => Ok(()),
            other => Err(format!("unknown recordType `{other}`")),
        }
    }
}

/// Checks a whole NDEF message: every record, the record count and the total size.
///
/// ```
/// use hardless_core::{validate_ndef_message, NdefRecord};
/// assert!(validate_ndef_message(&[NdefRecord::text("ok")]).is_ok());
/// assert!(validate_ndef_message(&[NdefRecord::default()]).is_err());
/// ```
pub fn validate_ndef_message(records: &[NdefRecord]) -> Result<(), String> {
    if records.len() > MAX_NDEF_RECORDS {
        return Err(format!("at most {MAX_NDEF_RECORDS} records"));
    }
    let mut total = 0usize;
    for (i, record) in records.iter().enumerate() {
        record.validate().map_err(|e| format!("record {i}: {e}"))?;
        total += record.payload().len();
    }
    if total > MAX_NDEF_BYTES {
        return Err(format!("records hold more than {MAX_NDEF_BYTES} bytes"));
    }
    Ok(())
}

/// Parses an NFC card UID given as hex, with or without `:`, `-` or space separators, and
/// returns it in Web NFC's `serialNumber` form: lowercase, colon-separated bytes.
///
/// Real UIDs are 4, 7 or 10 bytes (ISO 14443) or 8 bytes (ISO 15693); 1 to 10 are accepted.
///
/// ```
/// use hardless_core::serial_number;
/// assert_eq!(serial_number("04A2B3C4").unwrap(), "04:a2:b3:c4");
/// assert_eq!(serial_number("04:a2:b3:c4").unwrap(), "04:a2:b3:c4");
/// assert!(serial_number("4a2").is_err());
/// ```
pub fn serial_number(uid: &str) -> Result<String, String> {
    let hex: String = uid
        .chars()
        .filter(|c| !matches!(c, ':' | '-' | ' '))
        .collect();
    if hex.is_empty() || !hex.len().is_multiple_of(2) || !hex.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(format!("uid `{uid}` is not hex bytes, e.g. 04a2b3c4d5e6f7"));
    }
    if hex.len() > 20 {
        return Err(format!("uid `{uid}` is longer than 10 bytes"));
    }
    let lower = hex.to_ascii_lowercase();
    let bytes: Vec<&str> = (0..lower.len())
        .step_by(2)
        .filter_map(|i| lower.get(i..i + 2))
        .collect();
    Ok(bytes.join(":"))
}

/// A card read: the `reading` event of `NDEFReader`.
///
/// ```
/// use hardless_core::{NdefRecord, NfcReading};
/// let reading = NfcReading { serial_number: "04:a2".into(), records: vec![NdefRecord::text("Hi")] };
/// assert_eq!(
///     serde_json::to_value(&reading).unwrap(),
///     serde_json::json!({ "serialNumber": "04:a2", "records": [{ "recordType": "text", "data": "Hi" }] })
/// );
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct NfcReading {
    /// The card UID, lowercase colon-separated hex (see [`serial_number`]).
    pub serial_number: String,
    /// The card's NDEF records.
    pub records: Vec<NdefRecord>,
}

/// Why an NFC reader refused a write or a lock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "kebab-case")]
pub enum NfcRejection {
    /// No card is on the reader; the shim waits for the next tap and tries again.
    NoCard,
    /// The card is read-only (`NotAllowedError`).
    ReadOnly,
    /// The card has data and the write asked not to overwrite it (`NotAllowedError`).
    NotEmpty,
}

/// What an NFC reader reports, in [`DeviceEvent::Nfc`](crate::DeviceEvent::Nfc).
///
/// ```
/// use hardless_core::NfcEvent;
/// let event = NfcEvent::ReadError { serial_number: "04:a2".into() };
/// assert_eq!(
///     serde_json::to_value(&event).unwrap(),
///     serde_json::json!({ "type": "read-error", "serialNumber": "04:a2" })
/// );
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum NfcEvent {
    /// A card was tapped and read; the bridge's `nfc-reading` message.
    Reading(NfcReading),
    /// A card was tapped but could not be read; the bridge's `nfc-error` message.
    #[serde(rename_all = "camelCase")]
    ReadError {
        /// The card UID.
        serial_number: String,
    },
    /// The app wrote records to the card on the reader.
    #[serde(rename_all = "camelCase")]
    Written {
        /// The card UID.
        serial_number: String,
        /// What was written.
        records: Vec<NdefRecord>,
    },
    /// The app made the card on the reader read-only.
    #[serde(rename_all = "camelCase")]
    MadeReadOnly {
        /// The card UID.
        serial_number: String,
    },
    /// A write or lock was refused.
    Rejected {
        /// Why.
        reason: NfcRejection,
        /// A human-readable message.
        message: String,
    },
}

/// `#[serde(with = "opt_base64")]` for `Option<Bytes>` fields.
mod opt_base64 {
    use bytes::Bytes;
    use serde::{de, Deserialize, Deserializer, Serializer};

    pub(crate) fn serialize<S: Serializer>(data: &Option<Bytes>, s: S) -> Result<S::Ok, S::Error> {
        match data {
            Some(bytes) => s.serialize_str(&crate::base64::encode(bytes)),
            None => s.serialize_none(),
        }
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Bytes>, D::Error> {
        let text = Option::<std::borrow::Cow<'de, str>>::deserialize(d)?;
        text.map(|t| {
            crate::base64::decode(&t)
                .map(Bytes::from)
                .map_err(de::Error::custom)
        })
        .transpose()
    }
}
