//! HTTP printer endpoints (FR-19, FR-31 to FR-35, PRD section 9.1): ePOS-Print-compatible
//! and WebPRNT-compatible. These are what a POS app on any browser, including iOS Safari,
//! talks to, so they answer like a printer on the network: no token, any origin (FR-32).

use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use hardless_core::{ControlCommand, DeviceId, DeviceSummary, FaultId, LogEntry, LogLevel};
use hardless_devices::printer::CLOSE_JOB;
use hardless_http_dialects::{
    epos, webprnt, Outcome, PrintRequest, PrinterStatus, MAX_DOCUMENT_BYTES,
};
use serde::Deserialize;
use serde_json::Value;

use crate::AppState;

/// The ePOS-Print device ID apps use by default. With no printer of that ID, the first HTTP
/// printer answers it.
pub const DEFAULT_DEVID: &str = "local_printer";
/// ePOS-Print `timeout` when a request gives none, in milliseconds.
const DEFAULT_EPOS_TIMEOUT_MS: u64 = 10_000;
/// How long webPRNT clients wait by default before giving up, in milliseconds.
const WEBPRNT_TIMEOUT_MS: u64 = 90_000;
/// How far past the client's timeout the `timeout` fault holds a response.
const TIMEOUT_MARGIN_MS: u64 = 500;
/// Request headers a cross-origin print may carry (the ePOS samples set all three).
const ALLOW_HEADERS: &str = "Content-Type, SOAPAction, If-Modified-Since";

/// The routes, outside `/api/v1`.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/cgi-bin/epos/service.cgi",
            post(epos_print).options(epos_preflight),
        )
        .route(
            "/StarWebPRNT/SendMessage",
            post(webprnt_first).options(webprnt_first_preflight),
        )
        .route(
            "/p/{id}/StarWebPRNT/SendMessage",
            post(webprnt_named).options(webprnt_named_preflight),
        )
        .layer(DefaultBodyLimit::max(MAX_DOCUMENT_BYTES))
}

/// Printers reached over HTTP, in ID order.
fn http_printers(state: &AppState) -> Vec<DeviceSummary> {
    state
        .engine
        .list()
        .into_iter()
        .filter(|d| {
            d.kind == hardless_devices::printer::KIND
                && d.state.get("transport").and_then(Value::as_str) == Some("http")
        })
        .collect()
}

/// The ePOS-Print printer for `devid` (FR-35).
fn epos_printer(state: &AppState, devid: Option<&str>) -> Option<DeviceSummary> {
    let devid = devid.unwrap_or(DEFAULT_DEVID);
    let printers = http_printers(state);
    let exact = printers.iter().find(|d| d.id.as_str() == devid).cloned();
    exact.or_else(|| {
        (devid == DEFAULT_DEVID)
            .then(|| printers.first().cloned())
            .flatten()
    })
}

/// The webPRNT printer at `/p/<id>/...`, or the first HTTP printer (FR-35).
fn webprnt_printer(state: &AppState, id: Option<&str>) -> Option<DeviceSummary> {
    let printers = http_printers(state);
    match id {
        Some(id) => printers.into_iter().find(|d| d.id.as_str() == id),
        None => printers.into_iter().next(),
    }
}

fn has_fault(device: &DeviceSummary, fault: &FaultId) -> bool {
    device.faults.iter().any(|f| &f.id == fault)
}

fn connected(device: &DeviceSummary) -> bool {
    device.state.get("connected").and_then(Value::as_bool) != Some(false)
}

/// FR-32: echo the origin and allow the print request, unless `cors-blocked` is on.
fn with_cors(
    mut response: Response,
    headers: &HeaderMap,
    device: Option<&DeviceSummary>,
) -> Response {
    if device.is_some_and(|d| has_fault(d, &FaultId::CORS_BLOCKED)) {
        return response;
    }
    let Some(origin) = headers.get(header::ORIGIN).cloned() else {
        return response;
    };
    let out = response.headers_mut();
    out.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    out.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST, OPTIONS"),
    );
    out.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(ALLOW_HEADERS),
    );
    // Chrome's Private Network Access: a public site may call a printer on the LAN.
    if headers.contains_key("access-control-request-private-network") {
        out.insert(
            "access-control-allow-private-network",
            HeaderValue::from_static("true"),
        );
    }
    out.insert(header::VARY, HeaderValue::from_static("Origin"));
    response
}

fn preflight(headers: &HeaderMap, device: Option<&DeviceSummary>) -> Response {
    with_cors(StatusCode::NO_CONTENT.into_response(), headers, device)
}

fn xml(body: String) -> Response {
    ([(header::CONTENT_TYPE, "text/xml; charset=utf-8")], body).into_response()
}

/// The `offline` fault: the printer is not there. A real printer refuses the connection; on
/// a shared port the nearest thing is to drop it without an answer, which the browser
/// reports the same way (a network error).
fn dropped() -> Response {
    let stream = futures_util::stream::once(async {
        Err::<Bytes, std::io::Error>(std::io::Error::new(
            std::io::ErrorKind::ConnectionAborted,
            "printer offline",
        ))
    });
    Response::new(Body::from_stream(stream))
}

/// Prints `request` and returns the printer's status afterwards, read from its `DLE EOT`
/// replies (the `statusBytes` in its state).
fn print(state: &AppState, id: &DeviceId, request: &PrintRequest) -> Option<PrinterStatus> {
    let result = state.engine.drive(id, |r| {
        for warning in &request.warnings {
            r.log(id, LogEntry::new(LogLevel::Warn, warning.clone()))?;
        }
        let mut events = r.host_bytes(id, &request.escpos)?;
        events.extend(r.control(
            id,
            ControlCommand::Custom {
                name: CLOSE_JOB.into(),
                payload: Value::Null,
            },
        )?);
        Ok(events)
    });
    result.ok()?;
    let summary = state.engine.summary(id)?;
    let bytes = summary.state.get("statusBytes")?.as_array()?;
    let byte = |i: usize| {
        bytes
            .get(i)
            .and_then(Value::as_u64)
            .and_then(|b| u8::try_from(b).ok())
            .unwrap_or(0)
    };
    Some(PrinterStatus::from_dle_eot(byte(0), byte(1), byte(3)))
}

/// The request body as text.
fn utf8(body: &Bytes) -> Result<&str, String> {
    std::str::from_utf8(body).map_err(|_| "the document is not UTF-8".to_owned())
}

/// Logs a rejected request in the printer's event log.
fn log_rejected(state: &AppState, id: &DeviceId, dialect: &str, error: &str) {
    state.engine.with(|r| {
        let _ = r.log(
            id,
            LogEntry::new(
                LogLevel::Warn,
                format!("{dialect} request rejected: {error}"),
            ),
        );
    });
}

#[derive(Debug, Deserialize)]
struct EposQuery {
    devid: Option<String>,
    timeout: Option<u64>,
}

async fn epos_preflight(
    State(state): State<AppState>,
    Query(query): Query<EposQuery>,
    headers: HeaderMap,
) -> Response {
    let device = epos_printer(&state, query.devid.as_deref());
    preflight(&headers, device.as_ref())
}

async fn epos_print(
    State(state): State<AppState>,
    Query(query): Query<EposQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let parsed = utf8(&body).and_then(|text| epos::parse(text).map_err(|e| e.to_string()));
    let (header_devid, header_timeout, job_id) = match &parsed {
        Ok(req) => (req.devid.clone(), req.timeout_ms, req.print_job_id.clone()),
        Err(_) => (None, None, None),
    };
    let devid = query.devid.or(header_devid);
    let Some(device) = epos_printer(&state, devid.as_deref()) else {
        let body = epos::response(
            &Outcome::DeviceNotFound,
            &PrinterStatus::default(),
            job_id.as_deref(),
        );
        return with_cors(xml(body), &headers, None);
    };
    if !connected(&device) {
        return dropped();
    }
    let request = match parsed {
        Ok(request) => request,
        Err(e) => {
            log_rejected(&state, &device.id, "ePOS-Print", &e);
            let outcome = Outcome::BadDocument(e);
            let body = epos::response(&outcome, &PrinterStatus::default(), job_id.as_deref());
            return with_cors(xml(body), &headers, Some(&device));
        }
    };
    let Some(status) = print(&state, &device.id, &request.print) else {
        return dropped();
    };
    let mut outcome = Outcome::after_print(&status);
    if has_fault(&device, &FaultId::TIMEOUT) {
        let timeout = query
            .timeout
            .or(request.timeout_ms)
            .or(header_timeout)
            .unwrap_or(DEFAULT_EPOS_TIMEOUT_MS);
        tokio::time::sleep(Duration::from_millis(timeout + TIMEOUT_MARGIN_MS)).await;
        outcome = Outcome::Timeout;
    }
    let body = epos::response(&outcome, &status, request.print_job_id.as_deref());
    with_cors(xml(body), &headers, Some(&device))
}

async fn webprnt_first_preflight(State(state): State<AppState>, headers: HeaderMap) -> Response {
    preflight(&headers, webprnt_printer(&state, None).as_ref())
}

async fn webprnt_named_preflight(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    preflight(&headers, webprnt_printer(&state, Some(&id)).as_ref())
}

async fn webprnt_first(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let device = webprnt_printer(&state, None);
    webprnt_print(state, headers, device, body).await
}

async fn webprnt_named(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let device = webprnt_printer(&state, Some(&id));
    webprnt_print(state, headers, device, body).await
}

async fn webprnt_print(
    state: AppState,
    headers: HeaderMap,
    device: Option<DeviceSummary>,
    body: Bytes,
) -> Response {
    let Some(device) = device else {
        let response = (StatusCode::NOT_FOUND, "no HTTP printer at this address").into_response();
        return with_cors(response, &headers, None);
    };
    if !connected(&device) {
        return dropped();
    }
    let parsed = utf8(&body).and_then(|text| webprnt::parse(text).map_err(|e| e.to_string()));
    let request = match parsed {
        Ok(request) => request,
        Err(e) => {
            log_rejected(&state, &device.id, "WebPRNT", &e);
            let body = webprnt::response(&Outcome::BadDocument(e), &PrinterStatus::default());
            return with_cors(xml(body), &headers, Some(&device));
        }
    };
    let Some(status) = print(&state, &device.id, &request) else {
        return dropped();
    };
    let mut outcome = Outcome::after_print(&status);
    if has_fault(&device, &FaultId::TIMEOUT) {
        tokio::time::sleep(Duration::from_millis(
            WEBPRNT_TIMEOUT_MS + TIMEOUT_MARGIN_MS,
        ))
        .await;
        outcome = Outcome::Timeout;
    }
    with_cors(
        xml(webprnt::response(&outcome, &status)),
        &headers,
        Some(&device),
    )
}
