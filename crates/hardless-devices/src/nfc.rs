//! NFC reader (`nfc-reader`, FR-27, PRD section 8.3).
//!
//! A phone's NFC reader sees a card while it is held against it, about a second or two. A tap
//! (`ControlCommand::Tap`) puts a card on the reader for `presenceMs` and reads it: the reader
//! emits [`NfcEvent::Reading`], which the shim fires as `NDEFReader`'s `reading` event. While
//! the card is there the app can write to it or lock it (`NDEFReader.write()` and
//! `makeReadOnly()`). The reader remembers every card it has seen, so a card the app wrote to
//! reads back what was written the next time it is tapped.

use hardless_core::{
    serial_number, validate_ndef_message, ConfigError, ControlCommand, Device, DeviceEvent,
    DeviceId, DevicePlugin, FaultId, FaultOptions, LogEntry, LogLevel, Millis, NdefRecord,
    NfcEvent, NfcReading, NfcRejection,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The device kind.
pub const KIND: &str = "nfc-reader";

/// Most cards the reader remembers; the one seen longest ago is forgotten first.
pub const MAX_CARDS: usize = 256;

/// A card the reader knows from the start, e.g. for the dashboard's card list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardConfig {
    /// The card UID as hex, e.g. `04a2b3c4d5e6f7`.
    pub uid: String,
    /// The card's NDEF records.
    #[serde(default)]
    pub records: Vec<NdefRecord>,
    /// Whether the card is read-only.
    #[serde(default)]
    pub read_only: bool,
}

/// NFC reader configuration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NfcConfig {
    /// How long a tapped card stays on the reader, in milliseconds (default 2000).
    #[serde(default = "default_presence_ms")]
    pub presence_ms: u64,
    /// Cards the reader knows from the start.
    #[serde(default)]
    pub cards: Vec<CardConfig>,
}

fn default_presence_ms() -> u64 {
    2_000
}

impl Default for NfcConfig {
    fn default() -> Self {
        Self {
            presence_ms: default_presence_ms(),
            cards: Vec::new(),
        }
    }
}

/// Faults an NFC reader supports (PRD section 10). `picker-cancel` and `permission-denied`
/// are applied by the shim: `NDEFReader.scan()` rejects with `NotAllowedError`.
const FAULTS: [FaultId; 3] = [
    FaultId::PICKER_CANCEL,
    FaultId::PERMISSION_DENIED,
    FaultId::READ_ERROR,
];

/// Creates [`NfcReader`]s.
#[derive(Debug, Default)]
pub struct NfcPlugin;

impl DevicePlugin for NfcPlugin {
    fn kind(&self) -> &'static str {
        KIND
    }

    fn config_schema(&self) -> schemars::Schema {
        schemars::schema_for!(NfcConfig)
    }

    fn faults(&self) -> Vec<FaultId> {
        FAULTS.to_vec()
    }

    fn create(&self, _id: DeviceId, config: Value) -> Result<Box<dyn Device>, ConfigError> {
        let config = if config.is_null() {
            NfcConfig::default()
        } else {
            serde_json::from_value::<NfcConfig>(config)
                .map_err(|e| ConfigError::new(e.to_string()))?
        };
        if config.presence_ms == 0 {
            return Err(ConfigError::new("presenceMs must be at least 1").at("/presenceMs"));
        }
        if config.cards.len() > MAX_CARDS {
            return Err(ConfigError::new(format!("at most {MAX_CARDS} cards")).at("/cards"));
        }
        let mut cards = Vec::with_capacity(config.cards.len());
        for (i, card) in config.cards.iter().enumerate() {
            let serial = serial_number(&card.uid)
                .map_err(|e| ConfigError::new(e).at(format!("/cards/{i}/uid")))?;
            validate_ndef_message(&card.records)
                .map_err(|e| ConfigError::new(e).at(format!("/cards/{i}/records")))?;
            cards.push(Card {
                serial,
                records: card.records.clone(),
                read_only: card.read_only,
            });
        }
        Ok(Box::new(NfcReader::new(config, cards)))
    }
}

/// A card the reader has seen.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Card {
    serial: String,
    records: Vec<NdefRecord>,
    read_only: bool,
}

impl Card {
    fn to_json(&self) -> Value {
        json!({ "uid": self.serial, "records": self.records, "readOnly": self.read_only })
    }
}

/// An emulated NFC reader.
#[derive(Debug)]
pub struct NfcReader {
    config: NfcConfig,
    /// The cards from the config, restored on reset.
    initial: Vec<Card>,
    /// Every card seen, least recently seen first.
    cards: Vec<Card>,
    /// The card on the reader and when it leaves.
    present: Option<(String, Millis)>,
    taps: u64,
    writes: u64,
    /// The latest write: card serial number and records.
    last_written: Option<(String, Vec<NdefRecord>)>,
    picker_cancel: bool,
    permission_denied: bool,
    read_error: bool,
}

impl NfcReader {
    fn new(config: NfcConfig, cards: Vec<Card>) -> Self {
        Self {
            config,
            initial: cards.clone(),
            cards,
            present: None,
            taps: 0,
            writes: 0,
            last_written: None,
            picker_cancel: false,
            permission_denied: false,
            read_error: false,
        }
    }

    /// The card on the reader at `now`, if any.
    fn present_card(&self, now: Millis) -> Option<&str> {
        self.present
            .as_ref()
            .filter(|(_, until)| now < *until)
            .map(|(serial, _)| serial.as_str())
    }

    fn card_mut(&mut self, serial: &str) -> Option<&mut Card> {
        self.cards.iter_mut().find(|c| c.serial == serial)
    }

    /// Moves `card` to the most recently seen end, forgetting the oldest card if full.
    fn remember(&mut self, card: Card) {
        self.cards.retain(|c| c.serial != card.serial);
        if self.cards.len() >= MAX_CARDS {
            self.cards.remove(0);
        }
        self.cards.push(card);
    }

    fn tap(
        &mut self,
        uid: &str,
        records: Option<Vec<NdefRecord>>,
        now: Millis,
    ) -> Vec<DeviceEvent> {
        let serial = match serial_number(uid) {
            Ok(serial) => serial,
            Err(e) => return vec![warn(format!("tap ignored: {e}"))],
        };
        if let Some(records) = &records {
            if let Err(e) = validate_ndef_message(records) {
                return vec![warn(format!("tap ignored: {e}"))];
            }
        }
        let known = self.cards.iter().find(|c| c.serial == serial).cloned();
        let card = match (records, known) {
            // A tap with records is a card holding exactly those records.
            (Some(records), _) => Card {
                serial: serial.clone(),
                records,
                read_only: false,
            },
            (None, Some(card)) => card,
            (None, None) => Card {
                serial: serial.clone(),
                records: Vec::new(),
                read_only: false,
            },
        };
        self.taps += 1;
        let records = card.records.clone();
        self.remember(card);
        if self.read_error {
            // The card was seen but not read: nothing to write to either.
            self.present = None;
            return vec![
                DeviceEvent::Nfc(NfcEvent::ReadError {
                    serial_number: serial,
                }),
                DeviceEvent::StateChanged,
            ];
        }
        self.present = Some((serial.clone(), now + self.config.presence_ms));
        vec![
            DeviceEvent::Nfc(NfcEvent::Reading(NfcReading {
                serial_number: serial,
                records,
            })),
            DeviceEvent::StateChanged,
        ]
    }

    fn write(
        &mut self,
        records: Vec<NdefRecord>,
        overwrite: bool,
        now: Millis,
    ) -> Vec<DeviceEvent> {
        if let Err(e) = validate_ndef_message(&records) {
            return vec![warn(format!("write ignored: {e}"))];
        }
        let Some(serial) = self.present_card(now).map(str::to_owned) else {
            return vec![rejected(NfcRejection::NoCard, "no card is on the reader")];
        };
        let Some(card) = self.card_mut(&serial) else {
            return vec![rejected(NfcRejection::NoCard, "no card is on the reader")];
        };
        if card.read_only {
            return vec![rejected(
                NfcRejection::ReadOnly,
                format!("card {serial} is read-only"),
            )];
        }
        if !overwrite && !card.records.is_empty() {
            return vec![rejected(
                NfcRejection::NotEmpty,
                format!("card {serial} has data and overwrite is false"),
            )];
        }
        card.records = records.clone();
        self.writes += 1;
        self.last_written = Some((serial.clone(), records.clone()));
        vec![
            DeviceEvent::Nfc(NfcEvent::Written {
                serial_number: serial,
                records,
            }),
            DeviceEvent::StateChanged,
        ]
    }

    fn make_read_only(&mut self, now: Millis) -> Vec<DeviceEvent> {
        let Some(serial) = self.present_card(now).map(str::to_owned) else {
            return vec![rejected(NfcRejection::NoCard, "no card is on the reader")];
        };
        let Some(card) = self.card_mut(&serial) else {
            return vec![rejected(NfcRejection::NoCard, "no card is on the reader")];
        };
        card.read_only = true;
        vec![
            DeviceEvent::Nfc(NfcEvent::MadeReadOnly {
                serial_number: serial,
            }),
            DeviceEvent::StateChanged,
        ]
    }
}

fn warn(message: impl Into<String>) -> DeviceEvent {
    DeviceEvent::Log(LogEntry::new(LogLevel::Warn, message))
}

fn rejected(reason: NfcRejection, message: impl Into<String>) -> DeviceEvent {
    DeviceEvent::Nfc(NfcEvent::Rejected {
        reason,
        message: message.into(),
    })
}

impl Device for NfcReader {
    fn kind(&self) -> &'static str {
        KIND
    }

    fn state(&self) -> Value {
        let last_written = self
            .last_written
            .as_ref()
            .map(|(serial, records)| json!({ "uid": serial, "records": records }));
        json!({
            "presenceMs": self.config.presence_ms,
            "card": self.present.as_ref().map(|(serial, _)| serial),
            "taps": self.taps,
            "writes": self.writes,
            "lastWritten": last_written,
            "cards": self.cards.iter().map(Card::to_json).collect::<Vec<_>>(),
            "pickerCancel": self.picker_cancel,
            "permissionDenied": self.permission_denied,
            "readError": self.read_error,
        })
    }

    fn on_host_bytes(&mut self, _data: &[u8], _now: Millis) -> Vec<DeviceEvent> {
        vec![warn(
            "an NFC reader takes NDEF records (nfc-write), not bytes; bytes were dropped",
        )]
    }

    fn on_control(&mut self, cmd: ControlCommand, now: Millis) -> Vec<DeviceEvent> {
        match cmd {
            ControlCommand::Tap { uid, records } => self.tap(&uid, records, now),
            ControlCommand::NfcWrite { records, overwrite } => self.write(records, overwrite, now),
            ControlCommand::NfcMakeReadOnly => self.make_read_only(now),
            other => {
                let name = serde_json::to_value(&other)
                    .ok()
                    .and_then(|v| v.get("type").and_then(Value::as_str).map(str::to_owned))
                    .unwrap_or_default();
                vec![warn(format!(
                    "{KIND} does not support control command {name}"
                ))]
            }
        }
    }

    fn on_tick(&mut self, now: Millis) -> Vec<DeviceEvent> {
        if self
            .present
            .as_ref()
            .is_some_and(|(_, until)| *until <= now)
        {
            // The card has been taken away.
            self.present = None;
            return vec![DeviceEvent::StateChanged];
        }
        Vec::new()
    }

    fn next_deadline(&self) -> Option<Millis> {
        self.present.as_ref().map(|(_, until)| *until)
    }

    fn faults(&self) -> Vec<FaultId> {
        FAULTS.to_vec()
    }

    fn set_fault(
        &mut self,
        fault: FaultId,
        active: bool,
        _opts: FaultOptions,
        _now: Millis,
    ) -> Vec<DeviceEvent> {
        match fault.as_str() {
            "picker-cancel" => self.picker_cancel = active,
            "permission-denied" => self.permission_denied = active,
            "read-error" => self.read_error = active,
            _ => {}
        }
        vec![DeviceEvent::StateChanged]
    }

    fn reset(&mut self, _now: Millis) -> Vec<DeviceEvent> {
        *self = NfcReader::new(self.config.clone(), self.initial.clone());
        vec![DeviceEvent::StateChanged]
    }
}
