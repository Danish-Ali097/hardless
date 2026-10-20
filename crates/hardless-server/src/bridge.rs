//! The WebSocket bridge at `/bridge` (FR-18, PRD section 11.2): the browser shim's
//! connection to the emulated devices.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use hardless_core::bridge::{
    BridgeErrorCode, ClientFrame, ClientMessage, ServerFrame, ServerMessage, BRIDGE_VERSION,
};
use hardless_core::{
    ControlCommand, DeviceEvent, DeviceId, FaultId, LogLevel, NfcEvent, NfcReading, NfcRejection,
};
use serde_json::Value;
use tokio::sync::broadcast::error::RecvError;

use crate::engine::{Engine, Event, SessionId};

/// Default `msPerChunk` for `slow-write` (PRD section 10).
const DEFAULT_MS_PER_CHUNK: u64 = 200;

struct Session {
    id: SessionId,
    engine: Arc<Engine>,
    open: HashSet<DeviceId>,
    /// Whether the shim's `hello` listed `wedge`: only then does it get scans.
    wedge: bool,
    /// Whether the shim's `hello` listed `nfc`: only then does it get card reads.
    nfc: bool,
}

/// Runs one bridge connection until it closes.
pub async fn run(socket: WebSocket, engine: Arc<Engine>) {
    let id = engine.new_session();
    let mut session = Session {
        id,
        engine: engine.clone(),
        open: HashSet::new(),
        wedge: false,
        nfc: false,
    };
    let mut events = engine.subscribe();
    let (mut tx, mut rx) = futures_util::StreamExt::split(socket);
    use futures_util::{SinkExt, StreamExt};

    loop {
        let outgoing: Vec<ServerMessage> = tokio::select! {
            incoming = rx.next() => match incoming {
                Some(Ok(Message::Text(text))) => session.handle(text.as_str()).await,
                Some(Ok(Message::Binary(_))) => vec![bad_message("binary frames are not supported", None)],
                Some(Ok(_)) => continue,
                Some(Err(_)) | None => break,
            },
            event = events.recv() => match event {
                Ok(event) => session.on_event(&event),
                // Missed events: resend the device list so the shim can catch up.
                Err(RecvError::Lagged(_)) => vec![devices(&engine)],
                Err(RecvError::Closed) => break,
            },
        };
        for message in outgoing {
            let text = serde_json::to_string(&ServerFrame::new(message)).unwrap_or_default();
            if tx.send(Message::Text(text.into())).await.is_err() {
                engine.release_ports(id, None);
                return;
            }
        }
    }
    engine.release_ports(id, None);
}

fn devices(engine: &Engine) -> ServerMessage {
    ServerMessage::Devices {
        devices: engine.list(),
    }
}

fn error(
    code: BridgeErrorCode,
    message: impl Into<String>,
    reply_to: Option<String>,
) -> ServerMessage {
    ServerMessage::Error {
        code,
        message: message.into(),
        reply_to,
    }
}

fn bad_message(message: impl Into<String>, reply_to: Option<String>) -> ServerMessage {
    error(BridgeErrorCode::BadMessage, message, reply_to)
}

fn ack(id: Option<String>) -> Vec<ServerMessage> {
    id.map(|reply_to| ServerMessage::Ack { reply_to })
        .into_iter()
        .collect()
}

impl Session {
    async fn handle(&mut self, text: &str) -> Vec<ServerMessage> {
        let frame: ClientFrame = match serde_json::from_str(text) {
            Ok(frame) => frame,
            Err(e) => {
                let id = serde_json::from_str::<Value>(text)
                    .ok()
                    .and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_owned));
                return vec![bad_message(format!("cannot parse message: {e}"), id)];
            }
        };
        if frame.v != BRIDGE_VERSION {
            return vec![bad_message(
                format!(
                    "unsupported protocol version {} (expected {BRIDGE_VERSION})",
                    frame.v
                ),
                frame.id,
            )];
        }
        let reply_to = frame.id;
        match frame.message {
            ClientMessage::Hello { apis, .. } => {
                self.wedge = apis.iter().any(|api| api == "wedge");
                self.nfc = apis.iter().any(|api| api == "nfc");
                let mut out = vec![devices(&self.engine)];
                out.extend(ack(reply_to));
                out
            }
            ClientMessage::Open { device_id, .. } => self.open(device_id, reply_to),
            ClientMessage::Close { device_id } => {
                if self.open.remove(&device_id) {
                    self.engine.release_ports(self.id, Some(&device_id));
                    ack(reply_to)
                } else {
                    vec![error(
                        BridgeErrorCode::InvalidState,
                        format!("port `{device_id}` is not open"),
                        reply_to,
                    )]
                }
            }
            ClientMessage::Signals { device_id, signals } => {
                if !self.open.contains(&device_id) {
                    return vec![error(
                        BridgeErrorCode::InvalidState,
                        format!("port `{device_id}` is not open"),
                        reply_to,
                    )];
                }
                match self.engine.with(|r| r.host_signals(&device_id, signals)) {
                    Ok(()) => ack(reply_to),
                    Err(e) => vec![error(BridgeErrorCode::NotFound, e.to_string(), reply_to)],
                }
            }
            ClientMessage::Write { device_id, data } => {
                self.write(device_id, &data, reply_to).await
            }
            ClientMessage::NfcWrite {
                device_id,
                records,
                overwrite,
            } => self.nfc_command(
                &device_id,
                ControlCommand::NfcWrite {
                    records,
                    overwrite: overwrite.unwrap_or(true),
                },
                reply_to,
            ),
            ClientMessage::NfcMakeReadOnly { device_id } => {
                self.nfc_command(&device_id, ControlCommand::NfcMakeReadOnly, reply_to)
            }
        }
    }

    /// `nfc-write` and `nfc-make-read-only`: acknowledged once the card has changed.
    fn nfc_command(
        &mut self,
        device_id: &DeviceId,
        command: ControlCommand,
        reply_to: Option<String>,
    ) -> Vec<ServerMessage> {
        let events = match self
            .engine
            .drive_from(device_id, Some(self.id), |r| r.control(device_id, command))
        {
            Ok(events) => events,
            Err(e) => return vec![error(BridgeErrorCode::NotFound, e.to_string(), reply_to)],
        };
        for event in events {
            match event {
                DeviceEvent::Nfc(NfcEvent::Written { .. } | NfcEvent::MadeReadOnly { .. }) => {
                    return ack(reply_to)
                }
                DeviceEvent::Nfc(NfcEvent::Rejected { reason, message }) => {
                    let code = match reason {
                        NfcRejection::NoCard => BridgeErrorCode::InvalidState,
                        NfcRejection::ReadOnly | NfcRejection::NotEmpty => {
                            BridgeErrorCode::NotAllowed
                        }
                    };
                    return vec![error(code, message, reply_to)];
                }
                // Invalid records, or a device that is not an NFC reader.
                DeviceEvent::Log(entry) if entry.level >= LogLevel::Warn => {
                    return vec![bad_message(entry.message, reply_to)]
                }
                _ => {}
            }
        }
        vec![bad_message(
            format!("device `{device_id}` did not answer the NFC request"),
            reply_to,
        )]
    }

    fn open(&mut self, device_id: DeviceId, reply_to: Option<String>) -> Vec<ServerMessage> {
        let Some(summary) = self.engine.summary(&device_id) else {
            return vec![error(
                BridgeErrorCode::NotFound,
                format!("device `{device_id}` not found"),
                reply_to,
            )];
        };
        if self.open.contains(&device_id) {
            return vec![error(
                BridgeErrorCode::InvalidState,
                format!("port `{device_id}` is already open"),
                reply_to,
            )];
        }
        if summary.state.get("connected").and_then(Value::as_bool) == Some(false) {
            return vec![error(
                BridgeErrorCode::Network,
                format!("device `{device_id}` is disconnected"),
                reply_to,
            )];
        }
        if !self.engine.claim_port(&device_id, self.id) {
            return vec![error(
                BridgeErrorCode::Network,
                format!("port `{device_id}` is open in another tab"),
                reply_to,
            )];
        }
        self.open.insert(device_id);
        ack(reply_to)
    }

    async fn write(
        &mut self,
        device_id: DeviceId,
        data: &[u8],
        reply_to: Option<String>,
    ) -> Vec<ServerMessage> {
        if !self.open.contains(&device_id) {
            return vec![error(
                BridgeErrorCode::InvalidState,
                format!("port `{device_id}` is not open"),
                reply_to,
            )];
        }
        let delay = self.engine.summary(&device_id).and_then(|s| {
            s.faults
                .iter()
                .find(|f| f.id == FaultId::SLOW_WRITE)
                .map(|f| {
                    f.options
                        .get("msPerChunk")
                        .and_then(Value::as_u64)
                        .unwrap_or(DEFAULT_MS_PER_CHUNK)
                })
        });
        let result = self.engine.drive_from(&device_id, Some(self.id), |r| {
            r.host_bytes(&device_id, data)
        });
        match result {
            Err(e) => vec![error(BridgeErrorCode::NotFound, e.to_string(), reply_to)],
            Ok(events) if events.contains(&DeviceEvent::Disconnected) => vec![error(
                BridgeErrorCode::Network,
                format!("device `{device_id}` disconnected during the write"),
                reply_to,
            )],
            Ok(events) => {
                // Status replies go out before the acknowledgement, in order.
                let mut out: Vec<ServerMessage> = events
                    .into_iter()
                    .filter_map(|event| match event {
                        DeviceEvent::ToHost(data) => Some(ServerMessage::Data {
                            device_id: device_id.clone(),
                            data,
                        }),
                        _ => None,
                    })
                    .collect();
                if let Some(ms) = delay {
                    // slow-write: hold the acknowledgement, which back-pressures the shim.
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                }
                out.extend(ack(reply_to));
                out
            }
        }
    }

    fn on_event(&mut self, event: &Event) -> Vec<ServerMessage> {
        match event {
            Event::Bus(_) => vec![devices(&self.engine)],
            Event::Device {
                device_id,
                event,
                origin,
            } => match event {
                DeviceEvent::ToHost(data)
                    if self.open.contains(device_id) && *origin != Some(self.id) =>
                {
                    vec![ServerMessage::Data {
                        device_id: device_id.clone(),
                        data: data.clone(),
                    }]
                }
                // A wedge scanner types into every page that shims the wedge, like a keyboard
                // types into whichever window has focus.
                DeviceEvent::Wedge(scan) if self.wedge => vec![ServerMessage::WedgeScan {
                    device_id: device_id.clone(),
                    scan: scan.clone(),
                }],
                // Card reads reach every page that shims NFC, as a phone's reader serves
                // whichever page is scanning.
                DeviceEvent::Nfc(NfcEvent::Reading(reading)) if self.nfc => {
                    vec![ServerMessage::NfcReading {
                        device_id: device_id.clone(),
                        reading: reading.clone(),
                    }]
                }
                DeviceEvent::Nfc(NfcEvent::ReadError { serial_number }) if self.nfc => {
                    vec![ServerMessage::NfcError {
                        device_id: device_id.clone(),
                        reading: NfcReading {
                            serial_number: serial_number.clone(),
                            records: Vec::new(),
                        },
                    }]
                }
                DeviceEvent::Connected => vec![ServerMessage::Connect {
                    device_id: device_id.clone(),
                }],
                DeviceEvent::Disconnected => {
                    // The port closes when its device goes away.
                    if self.open.remove(device_id) {
                        self.engine.release_ports(self.id, Some(device_id));
                    }
                    vec![ServerMessage::Disconnect {
                        device_id: device_id.clone(),
                    }]
                }
                _ => Vec::new(),
            },
        }
    }
}
