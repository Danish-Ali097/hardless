//! Messages of the WebSocket bridge between the browser shim and the server (PRD section
//! 11.2). One JSON message per frame; byte payloads are base64.
//!
//! Every message has `v: 1`, a `type`, and optionally an `id`. A client message with an
//! `id` is answered with `ack` (`replyTo` = that id) or `error`.

use serde::{Deserialize, Serialize};

use crate::ids::DeviceId;
use crate::registry::DeviceSummary;
use crate::Bytes;

/// The protocol version in every message's `v`.
pub const BRIDGE_VERSION: u8 = 1;

/// A message from the shim to the server.
///
/// ```
/// use hardless_core::bridge::{ClientFrame, ClientMessage};
/// let frame: ClientFrame = serde_json::from_str(
///     r#"{ "v": 1, "type": "write", "id": "7", "deviceId": "printer-1", "data": "G0A=" }"#,
/// ).unwrap();
/// assert_eq!(frame.id.as_deref(), Some("7"));
/// assert!(matches!(frame.message, ClientMessage::Write { ref data, .. } if data.as_ref() == b"\x1b@"));
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ClientFrame {
    /// Always [`BRIDGE_VERSION`].
    pub v: u8,
    /// Correlates the server's `ack` or `error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub id: Option<String>,
    /// The message.
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub message: ClientMessage,
}

/// The body of a [`ClientFrame`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ClientMessage {
    /// First message: who is connecting and which APIs it shims.
    #[serde(rename_all = "camelCase")]
    Hello {
        /// The page's origin.
        origin: String,
        /// The shim version.
        shim_version: String,
        /// Shimmed APIs: `serial`, `usb`, `nfc`, `wedge`.
        apis: Vec<String>,
    },
    /// `SerialPort.open()`.
    #[serde(rename_all = "camelCase")]
    Open {
        /// The device.
        device_id: DeviceId,
        /// The options passed to `open()`, e.g. `{ "baudRate": 9600 }`.
        #[serde(default)]
        options: serde_json::Value,
    },
    /// `SerialPort.close()`.
    #[serde(rename_all = "camelCase")]
    Close {
        /// The device.
        device_id: DeviceId,
    },
    /// `SerialPort.setSignals()`: output signals set by the app (PRD section 8.1).
    #[serde(rename_all = "camelCase")]
    Signals {
        /// The device.
        device_id: DeviceId,
        /// The signals that were set.
        signals: SerialSignals,
    },
    /// Bytes written by the app.
    #[serde(rename_all = "camelCase")]
    Write {
        /// The device.
        device_id: DeviceId,
        /// The bytes, base64 on the wire.
        #[serde(with = "crate::base64::serde_bytes")]
        #[cfg_attr(feature = "ts", ts(type = "string"))]
        data: Bytes,
    },
}

/// Output signals from `SerialPort.setSignals()`; only the ones the app set are present.
///
/// ```
/// use hardless_core::bridge::SerialSignals;
/// let s: SerialSignals = serde_json::from_str(r#"{ "dataTerminalReady": true }"#).unwrap();
/// assert_eq!(s.data_terminal_ready, Some(true));
/// assert_eq!(s.request_to_send, None);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SerialSignals {
    /// Data Terminal Ready (DTR).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub data_terminal_ready: Option<bool>,
    /// Request To Send (RTS).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub request_to_send: Option<bool>,
    /// Break.
    #[serde(rename = "break", default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(rename = "break", optional))]
    pub brk: Option<bool>,
}

/// A message from the server to the shim.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ServerFrame {
    /// Always [`BRIDGE_VERSION`].
    pub v: u8,
    /// The message.
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub message: ServerMessage,
}

impl ServerFrame {
    /// A frame with the current protocol version.
    pub fn new(message: ServerMessage) -> Self {
        Self {
            v: BRIDGE_VERSION,
            message,
        }
    }
}

/// The body of a [`ServerFrame`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ServerMessage {
    /// The full device list, for the port picker and `getPorts()`. Sent after `hello` and
    /// whenever a device is added, removed or changes state.
    Devices {
        /// Every device.
        devices: Vec<DeviceSummary>,
    },
    /// A client message with an `id` succeeded.
    #[serde(rename_all = "camelCase")]
    Ack {
        /// The client message's `id`.
        reply_to: String,
    },
    /// Device -> host bytes.
    #[serde(rename_all = "camelCase")]
    Data {
        /// The device.
        device_id: DeviceId,
        /// The bytes, base64 on the wire.
        #[serde(with = "crate::base64::serde_bytes")]
        #[cfg_attr(feature = "ts", ts(type = "string"))]
        data: Bytes,
    },
    /// The device became reachable.
    #[serde(rename_all = "camelCase")]
    Connect {
        /// The device.
        device_id: DeviceId,
    },
    /// The device stopped being reachable.
    #[serde(rename_all = "camelCase")]
    Disconnect {
        /// The device.
        device_id: DeviceId,
    },
    /// Something failed; `replyTo` names the client message, if any.
    #[serde(rename_all = "camelCase")]
    Error {
        /// A machine-readable code, see [`BridgeErrorCode`].
        code: BridgeErrorCode,
        /// A human-readable message.
        message: String,
        /// The client message's `id`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        reply_to: Option<String>,
    },
}

/// Error codes in bridge `error` messages. The shim maps them to `DOMException` names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum BridgeErrorCode {
    /// No such device (`NotFoundError`).
    NotFound,
    /// The device is disconnected or went away (`NetworkError`).
    Network,
    /// The port is not open, or already open (`InvalidStateError`).
    InvalidState,
    /// The message could not be understood.
    BadMessage,
    /// The token is missing or wrong.
    Unauthorized,
    /// Anything else.
    Internal,
}
