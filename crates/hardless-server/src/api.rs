//! REST control API at `/api/v1` (FR-20, PRD section 11.1).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use hardless_core::{
    serial_number, validate_ndef_message, ControlCommand, DeviceId, DeviceSummary, EventRecord,
    FaultId, FaultOptions, Job, JobId, NdefRecord, Progress, Scenario, ScenarioReport, ScenarioRun,
    Step, Terminator, MAX_SCENARIO_STEPS,
};
use hardless_escpos::Node;
use hardless_render::{render_html_document, render_png, render_svg, PaperWidth, RenderOptions};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::AppState;

/// The `/api/v1` routes (the WebSocket `/stream` is added by the caller).
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/devices", get(list_devices).post(create_device))
        .route("/devices/{id}", get(get_device).delete(remove_device))
        .route("/devices/{id}/reset", post(reset_device))
        .route("/devices/{id}/close", post(close_drawer))
        .route("/devices/{id}/scan", post(scan))
        .route("/devices/{id}/tap", post(tap))
        .route("/scenarios/run", post(run_scenario))
        .route("/devices/{id}/jobs", get(list_jobs))
        .route("/devices/{id}/jobs/last", get(last_job))
        .route("/devices/{id}/jobs/{job_id}/render", get(render_job))
        .route("/devices/{id}/faults/{fault_id}", put(set_fault))
        .route("/devices/{id}/events", get(events))
}

fn device_id(raw: &str) -> Result<DeviceId, ApiError> {
    raw.parse()
        .map_err(|_| ApiError::not_found("not_found", format!("device `{raw}` not found")))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

async fn list_devices(State(state): State<AppState>) -> Json<Vec<DeviceSummary>> {
    Json(state.engine.list())
}

async fn get_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DeviceSummary>, ApiError> {
    let id = device_id(&id)?;
    state
        .engine
        .summary(&id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found("not_found", format!("device `{id}` not found")))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateDevice {
    kind: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    config: Value,
}

async fn create_device(
    State(state): State<AppState>,
    body: Result<Json<CreateDevice>, axum::extract::rejection::JsonRejection>,
) -> Result<(StatusCode, Json<DeviceSummary>), ApiError> {
    let Json(body) = body.map_err(|e| ApiError::bad_request("bad_request", e.body_text()))?;
    let id = match body.id {
        Some(raw) => Some(
            raw.parse::<DeviceId>()
                .map_err(|e| ApiError::bad_request("invalid_id", e.to_string()))?,
        ),
        None => None,
    };
    let created = state
        .engine
        .with(|r| r.create(&body.kind, id, body.config))?;
    let summary = state
        .engine
        .summary(&created)
        .ok_or_else(|| ApiError::not_found("not_found", format!("device `{created}` not found")))?;
    Ok((StatusCode::CREATED, Json(summary)))
}

async fn remove_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id = device_id(&id)?;
    state.engine.with(|r| r.remove(&id))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn reset_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DeviceSummary>, ApiError> {
    let id = device_id(&id)?;
    state.engine.drive(&id, |r| r.reset(&id))?;
    summary(&state, &id)
}

async fn close_drawer(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DeviceSummary>, ApiError> {
    let id = device_id(&id)?;
    state
        .engine
        .drive(&id, |r| r.control(&id, ControlCommand::CloseDrawer))?;
    summary(&state, &id)
}

/// `POST /devices/:id/scan` body.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScanBody {
    code: String,
    #[serde(default)]
    terminator: Option<Terminator>,
    #[serde(default)]
    delay_ms: Option<u64>,
}

/// Scans a barcode on a keyboard-wedge scanner (FR-26): every page with the wedge shim types it.
async fn scan(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Result<Json<ScanBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<DeviceSummary>, ApiError> {
    let id = device_id(&id)?;
    let Json(body) = body.map_err(|e| ApiError::bad_request("bad_request", e.body_text()))?;
    let command = ControlCommand::Scan {
        code: body.code,
        terminator: body.terminator,
        delay_ms: body.delay_ms,
    };
    state.engine.drive(&id, |r| r.control(&id, command))?;
    summary(&state, &id)
}

/// `POST /devices/:id/tap` body.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TapBody {
    uid: String,
    #[serde(default)]
    records: Option<Vec<NdefRecord>>,
}

/// Taps an NFC card on a reader (FR-27): pages with the NFC shim fire `reading`. Without
/// `records` the reader uses what it remembers of the card. Devices other than NFC readers
/// refuse it in their event log.
async fn tap(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Result<Json<TapBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<DeviceSummary>, ApiError> {
    let id = device_id(&id)?;
    let Json(body) = body.map_err(|e| ApiError::bad_request("bad_request", e.body_text()))?;
    serial_number(&body.uid).map_err(|e| ApiError::bad_request("invalid_uid", e))?;
    if let Some(records) = &body.records {
        validate_ndef_message(records).map_err(|e| ApiError::bad_request("invalid_records", e))?;
    }
    let command = ControlCommand::Tap {
        uid: body.uid,
        records: body.records,
    };
    state.engine.drive(&id, |r| r.control(&id, command))?;
    summary(&state, &id)
}

/// Longest scenario accepted, counting its waits and `withinMs`, in milliseconds.
const MAX_SCENARIO_MS: u64 = 10 * 60 * 1000;

/// `POST /scenarios/run` (FR-04): runs the scenario on the server's devices, in real time, and
/// answers with the report when it ends.
async fn run_scenario(
    State(state): State<AppState>,
    body: Result<Json<Scenario>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<ScenarioReport>, ApiError> {
    let Json(scenario) = body.map_err(|e| ApiError::bad_request("bad_request", e.body_text()))?;
    if scenario.steps.len() > MAX_SCENARIO_STEPS {
        return Err(ApiError::bad_request(
            "scenario_too_long",
            format!("a scenario has at most {MAX_SCENARIO_STEPS} steps"),
        ));
    }
    let planned: u64 = scenario
        .steps
        .iter()
        .map(|step| match step {
            Step::Wait { ms } => *ms,
            Step::Expect { within_ms, .. } => within_ms.unwrap_or(0),
            _ => 0,
        })
        .fold(0, u64::saturating_add);
    if planned > MAX_SCENARIO_MS {
        return Err(ApiError::bad_request(
            "scenario_too_long",
            format!("a scenario may wait at most {MAX_SCENARIO_MS} ms in total"),
        ));
    }
    let mut run = ScenarioRun::new(scenario);
    loop {
        let (progress, events, now) = state.engine.with(|r| {
            let (progress, events) = run.advance(r);
            (progress, events, r.now())
        });
        state.engine.broadcast(&events);
        match progress {
            Progress::Done(report) => return Ok(Json(report)),
            Progress::WaitUntil(until) => {
                tokio::time::sleep(std::time::Duration::from_millis(until.since(now))).await;
            }
        }
    }
}

fn summary(state: &AppState, id: &DeviceId) -> Result<Json<DeviceSummary>, ApiError> {
    state
        .engine
        .summary(id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found("not_found", format!("device `{id}` not found")))
}

#[derive(Deserialize)]
struct JobsQuery {
    limit: Option<usize>,
}

async fn list_jobs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<JobsQuery>,
) -> Result<Json<Vec<Job>>, ApiError> {
    let id = device_id(&id)?;
    let limit = query.limit.unwrap_or(50);
    let jobs = state
        .engine
        .with(|r| r.jobs(&id).map(|jobs| jobs.take(limit).cloned().collect()))?;
    Ok(Json(jobs))
}

async fn last_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Job>, ApiError> {
    let id = device_id(&id)?;
    let job = state
        .engine
        .with(|r| r.last_job(&id).map(Option::<&Job>::cloned))?;
    job.map(Json).ok_or_else(|| {
        ApiError::not_found(
            "no_jobs",
            format!("device `{id}` has not printed anything yet"),
        )
    })
}

#[derive(Deserialize)]
struct RenderQuery {
    format: Option<String>,
    width: Option<u16>,
    /// `false` (or `0`) hides the badges for unknown commands (FR-12).
    unknown: Option<String>,
}

async fn render_job(
    State(state): State<AppState>,
    Path((id, job_id)): Path<(String, u64)>,
    Query(query): Query<RenderQuery>,
) -> Result<Response, ApiError> {
    let id = device_id(&id)?;
    let job = state
        .engine
        .with(|r| {
            r.jobs(&id)
                .map(|mut jobs| jobs.find(|j| j.id == JobId(job_id)).cloned())
        })?
        .ok_or_else(|| ApiError::not_found("job_not_found", format!("job {job_id} not found")))?;
    let paper = match query.width {
        Some(mm) => PaperWidth::try_from(mm).map_err(|e| ApiError::bad_request("bad_width", e))?,
        None => device_paper(&state, &id),
    };
    let nodes: Vec<Node> = serde_json::from_value(job.ast.clone()).map_err(|e| {
        ApiError::bad_request("not_escpos", format!("job is not an ESC/POS job: {e}"))
    })?;
    let options = RenderOptions {
        paper,
        show_unknown: !matches!(query.unknown.as_deref(), Some("0" | "false")),
    };
    match query.format.as_deref().unwrap_or("html") {
        "html" => {
            let html = render_html_document(&nodes, &options);
            Ok(([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response())
        }
        "svg" => {
            let svg = render_svg(&nodes, &options);
            Ok((
                [(header::CONTENT_TYPE, "image/svg+xml; charset=utf-8")],
                svg,
            )
                .into_response())
        }
        "png" => {
            // Rasterising takes milliseconds of CPU; keep it off the async workers.
            let png = tokio::task::spawn_blocking(move || render_png(&nodes, &options))
                .await
                .map_err(|e| ApiError::internal(format!("PNG rendering stopped: {e}")))?
                .map_err(|e| ApiError::bad_request("receipt_too_long", e.to_string()))?;
            Ok(([(header::CONTENT_TYPE, "image/png")], png).into_response())
        }
        other => Err(ApiError::bad_request(
            "bad_format",
            format!("format must be html, svg or png, not `{other}`"),
        )),
    }
}

fn device_paper(state: &AppState, id: &DeviceId) -> PaperWidth {
    state
        .engine
        .summary(id)
        .and_then(|s| s.state.get("paperWidth").and_then(Value::as_u64))
        .and_then(|mm| u16::try_from(mm).ok())
        .and_then(|mm| PaperWidth::try_from(mm).ok())
        .unwrap_or_default()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FaultBody {
    active: bool,
    #[serde(default)]
    options: Option<FaultOptions>,
}

async fn set_fault(
    State(state): State<AppState>,
    Path((id, fault)): Path<(String, String)>,
    body: Result<Json<FaultBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<DeviceSummary>, ApiError> {
    let id = device_id(&id)?;
    let fault: FaultId = fault.parse().map_err(|e: hardless_core::InvalidId| {
        ApiError::bad_request("invalid_fault", e.to_string())
    })?;
    let Json(body) = body.map_err(|e| ApiError::bad_request("bad_request", e.body_text()))?;
    let options = body.options.unwrap_or_default();
    state
        .engine
        .drive(&id, |r| r.set_fault(&id, fault, body.active, options))?;
    summary(&state, &id)
}

#[derive(Deserialize)]
struct EventsQuery {
    since: Option<u64>,
    limit: Option<usize>,
}

async fn events(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<EventsQuery>,
) -> Result<Json<Vec<EventRecord>>, ApiError> {
    let id = device_id(&id)?;
    let records = state.engine.with(|r| {
        r.events(&id, query.since).map(|records| {
            let skip = query
                .limit
                .map_or(0, |limit| records.len().saturating_sub(limit));
            records.into_iter().skip(skip).cloned().collect::<Vec<_>>()
        })
    })?;
    Ok(Json(records))
}

/// Shared state type check: the API only needs the engine.
#[allow(dead_code)]
fn _assert_state(state: AppState) -> Arc<crate::engine::Engine> {
    state.engine
}
