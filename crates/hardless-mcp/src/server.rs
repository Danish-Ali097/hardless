//! The MCP server: tools (FR-45) and resources (FR-46) over the REST API.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use hardless_core::{base64, catalogue, EventRecord, FakeClock, NdefRecord, Registry};
use hardless_render::{render_png, to_text_with, PaperWidth, RenderOptions};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, ListResourceTemplatesResult, ListResourcesResult,
    PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult,
    Resource, ResourceContents, ResourceTemplate, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::{tool, tool_handler, tool_router, ErrorData, RoleServer, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::docs;
use crate::rest::{Method, Rest, RestResponse};

/// Tools that change something; `--read-only` hides them (FR-48).
pub const MUTATING_TOOLS: [&str; 7] = [
    "create_device",
    "remove_device",
    "reset_device",
    "scan_barcode",
    "tap_nfc",
    "set_fault",
    "run_scenario",
];

/// Default `timeoutMs` of `wait_for_job`.
const DEFAULT_WAIT_MS: u64 = 10_000;
const POLL: Duration = Duration::from_millis(100);

// ----- inputs ------------------------------------------------------------------------------

/// `create_device` input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateDeviceIn {
    /// Device kind: `printer-escpos`, `cash-drawer`, `scanner-wedge` or `nfc-reader`.
    pub kind: String,
    /// Device ID; generated (`<kind>-<n>`) if left out.
    #[serde(default)]
    pub id: Option<String>,
    /// Device config, e.g. `{ "transport": "usb" }` or `{ "attachedTo": "printer-1" }`.
    #[serde(default)]
    pub config: Option<Value>,
}

/// Input of tools that take only a device ID.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdIn {
    /// The device ID, e.g. `printer-1`.
    pub id: String,
}

/// What a receipt tool returns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ReceiptFormat {
    /// The plain-text receipt only.
    Text,
    /// The PNG only (with job metadata).
    Image,
    /// Text and PNG (the default).
    #[default]
    Both,
}

/// `get_last_receipt` input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReceiptIn {
    /// The printer, e.g. `printer-1`.
    pub device_id: String,
    /// `text`, `image` or `both` (default).
    #[serde(default)]
    pub format: ReceiptFormat,
    /// Paper width for the image, 58 or 80 mm (default: the printer's).
    #[serde(default)]
    pub width: Option<u16>,
}

/// `list_jobs` input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListJobsIn {
    /// The printer.
    pub device_id: String,
    /// Most jobs to list (default 20).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// `get_job` input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetJobIn {
    /// The printer.
    pub device_id: String,
    /// The job ID from `list_jobs`.
    pub job_id: u64,
    /// `text`, `image` or `both` (default).
    #[serde(default)]
    pub format: ReceiptFormat,
    /// Paper width for the image, 58 or 80 mm.
    #[serde(default)]
    pub width: Option<u16>,
}

/// `wait_for_job` input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WaitForJobIn {
    /// The printer.
    pub device_id: String,
    /// How long to wait in milliseconds (default 10000).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// `text`, `image` or `both` (default).
    #[serde(default)]
    pub format: ReceiptFormat,
    /// Paper width for the image, 58 or 80 mm.
    #[serde(default)]
    pub width: Option<u16>,
}

/// `decode_escpos` input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecodeIn {
    /// ESC/POS bytes as hex (`1b 40 48 69 0a`) or base64.
    pub bytes: String,
    /// Paper width, 58 or 80 mm (default 80).
    #[serde(default)]
    pub width: Option<u16>,
    /// `text`, `image` or `both` (default).
    #[serde(default)]
    pub format: ReceiptFormat,
}

/// `scan_barcode` input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScanIn {
    /// The keyboard-wedge scanner, e.g. `scanner-1`.
    pub device_id: String,
    /// The barcode content.
    pub code: String,
    /// Key after the code: `enter`, `tab` or `none` (default: the scanner's).
    #[serde(default)]
    pub terminator: Option<String>,
    /// Milliseconds between keys (default: the scanner's).
    #[serde(default)]
    pub delay_ms: Option<u64>,
}

/// `tap_nfc` input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TapIn {
    /// The NFC reader, e.g. `nfc-1`.
    pub device_id: String,
    /// Card UID as hex, e.g. `04a2b3c4d5e6f7`.
    pub uid: String,
    /// NDEF records (Web NFC field names); without them the reader uses what it remembers.
    #[serde(default)]
    pub records: Option<Vec<NdefRecord>>,
}

/// `set_fault` input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetFaultIn {
    /// The device.
    pub device_id: String,
    /// The fault, e.g. `paper-out` (see `list_faults`).
    pub fault_id: String,
    /// On or off.
    pub active: bool,
    /// Fault options, e.g. `{ "afterBytes": 32 }`.
    #[serde(default)]
    pub options: Option<Map<String, Value>>,
}

/// `list_faults` input.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListFaultsIn {
    /// Only faults a device kind supports, e.g. `printer-escpos`.
    #[serde(default)]
    pub kind: Option<String>,
}

/// `get_events` input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetEventsIn {
    /// The device.
    pub device_id: String,
    /// Only records after this `seq`.
    #[serde(default)]
    pub since: Option<u64>,
    /// Only the newest this many records (default 100).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// `run_scenario` input: a scenario (FR-04).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScenarioIn {
    /// A name for the report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Steps in order: `scan`, `tap`, `wait`, `setFault`, `clearFault`, `expect`, e.g.
    /// `{ "type": "scan", "device": "scanner-1", "code": "123" }`.
    pub steps: Vec<Value>,
}

// ----- outputs -----------------------------------------------------------------------------

/// A device in tool output.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeviceOut {
    /// Device ID.
    pub id: String,
    /// Device kind.
    pub kind: String,
    /// Device state.
    pub state: Value,
    /// Active faults.
    pub faults: Vec<String>,
    /// The device it is attached to, e.g. a drawer's printer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attached_to: Option<String>,
}

/// `list_devices` output.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DevicesOut {
    /// Every device.
    pub devices: Vec<DeviceOut>,
}

/// Output of tools that only succeed or fail.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OkOut {
    /// Always `true` on success.
    pub ok: bool,
}

/// How many commands of one kind a receipt has.
#[derive(Debug, Serialize, JsonSchema)]
pub struct NodeCount {
    /// AST node type, e.g. `text`, `cut`, `qr-print`.
    pub node: String,
    /// How many.
    pub count: usize,
}

/// A receipt: job metadata and an AST summary (FR-47).
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptOut {
    /// The printer (absent for `decode_escpos`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    /// The job ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<u64>,
    /// When the job closed (milliseconds since the server started).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<u64>,
    /// The plain-text receipt.
    pub text: String,
    /// Lines of text.
    pub lines: usize,
    /// Bytes the app sent.
    pub bytes: usize,
    /// Whether the receipt was cut.
    pub cut: bool,
    /// Commands in the receipt, by type, in order of first use.
    pub ast_summary: Vec<NodeCount>,
    /// Commands the parser did not know (`unknown` nodes).
    pub unknown_commands: usize,
}

/// A job in `list_jobs`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobSummary {
    /// The job ID.
    pub id: u64,
    /// When the job closed.
    pub at: u64,
    /// The first non-empty line of text.
    pub first_line: String,
    /// Lines of text.
    pub lines: usize,
    /// Whether the receipt was cut.
    pub cut: bool,
}

/// `list_jobs` output.
#[derive(Debug, Serialize, JsonSchema)]
pub struct JobsOut {
    /// Jobs, newest first.
    pub jobs: Vec<JobSummary>,
}

/// One fault option.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FaultOptionOut {
    /// Option name.
    pub name: String,
    /// Default value.
    pub default: Value,
    /// What it controls.
    pub description: String,
}

/// One fault of the catalogue.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FaultOut {
    /// Fault ID.
    pub id: String,
    /// Which devices it applies to.
    pub applies_to: String,
    /// What it does.
    pub effect: String,
    /// Its options.
    pub options: Vec<FaultOptionOut>,
}

/// `list_faults` output.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FaultsOut {
    /// The faults (PRD section 10), filtered by kind if asked.
    pub faults: Vec<FaultOut>,
}

/// One event log record.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventOut {
    /// Sequence number, for `since`.
    pub seq: u64,
    /// When (milliseconds since the server started).
    pub at: u64,
    /// Record type, e.g. `bytes-in`, `state-changed`, `nfc`.
    #[serde(rename = "type")]
    pub kind: String,
    /// `host-to-device` or `device-to-host` for bytes, keys and card traffic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    /// The bytes, for `bytes-in` and `bytes-out`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_hex: Option<String>,
    /// ESC/POS commands in the bytes, for `bytes-in`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decoded: Option<Vec<String>>,
    /// The whole record.
    pub record: Value,
}

/// `get_events` output.
#[derive(Debug, Serialize, JsonSchema)]
pub struct EventsOut {
    /// Records, oldest first.
    pub events: Vec<EventOut>,
}

// ----- the server --------------------------------------------------------------------------

/// The Hardless MCP server: one per session, over a [`Rest`] connection to the server.
#[derive(Clone)]
pub struct HardlessMcp {
    rest: Arc<dyn Rest>,
    tool_router: ToolRouter<Self>,
}

impl std::fmt::Debug for HardlessMcp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HardlessMcp").finish_non_exhaustive()
    }
}

/// A failed tool call (FR-49): `isError`, the REST error code and a one-line next step.
fn failure(code: &str, message: &str) -> CallToolResult {
    let next = match code {
        "not_found" => "Call `list_devices` to see the device IDs.",
        "unsupported_fault" => "Call `list_faults` with the device's kind to see its faults.",
        "unknown_kind" => "Kinds are printer-escpos, cash-drawer, scanner-wedge and nfc-reader.",
        "duplicate_id" => "Choose another id, or call `list_devices`.",
        "invalid_config" => "Check the config fields for the device kind (see `list_devices`).",
        "unreachable" => "Start Hardless with `hardless start` (or run `hardless mcp --embedded`).",
        "timeout" => "Trigger a print in the app, then call `wait_for_job` again.",
        "no_jobs" => "Print something first, or call `wait_for_job`.",
        _ => "Check the input and try again.",
    };
    let mut result = CallToolResult::error(vec![ContentBlock::text(format!(
        "{message} ({code}). {next}"
    ))]);
    result.structured_content =
        Some(json!({ "error": { "code": code, "message": message, "next": next } }));
    result
}

fn structured<T: Serialize>(value: &T) -> Result<CallToolResult, ErrorData> {
    let json =
        serde_json::to_value(value).map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::structured(json))
}

fn device_out(value: &Value) -> DeviceOut {
    DeviceOut {
        id: value["id"].as_str().unwrap_or_default().to_owned(),
        kind: value["kind"].as_str().unwrap_or_default().to_owned(),
        state: value["state"].clone(),
        faults: value["faults"]
            .as_array()
            .map(|faults| {
                faults
                    .iter()
                    .filter_map(|f| f["id"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        attached_to: value["attachedTo"].as_str().map(str::to_owned),
    }
}

/// Percent-encodes a path segment (device IDs are `[A-Za-z0-9_-]`, but be safe).
fn segment(raw: &str) -> String {
    raw.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// The AST summary of a job's `ast` (a list of nodes with a `type`).
fn summarize(ast: &Value) -> (Vec<NodeCount>, bool, usize) {
    let mut counts: Vec<NodeCount> = Vec::new();
    for node in ast.as_array().into_iter().flatten() {
        let kind = node["type"].as_str().unwrap_or("?").to_owned();
        match counts.iter_mut().find(|c| c.node == kind) {
            Some(c) => c.count += 1,
            None => counts.push(NodeCount {
                node: kind,
                count: 1,
            }),
        }
    }
    let cut = counts.iter().any(|c| c.node == "cut");
    let unknown = counts
        .iter()
        .find(|c| c.node == "unknown")
        .map_or(0, |c| c.count);
    (counts, cut, unknown)
}

fn receipt_out(device_id: Option<&str>, job: &Value) -> ReceiptOut {
    let text = job["text"].as_str().unwrap_or_default().to_owned();
    let (ast_summary, cut, unknown_commands) = summarize(&job["ast"]);
    ReceiptOut {
        device_id: device_id.map(str::to_owned),
        job_id: job["id"].as_u64(),
        at: job["at"].as_u64(),
        lines: text.lines().count(),
        bytes: job["bytesHex"].as_str().map_or(0, |h| h.len() / 2),
        text,
        cut,
        ast_summary,
        unknown_commands,
    }
}

/// Text block, structured content and (unless `text`) a PNG image block (FR-47).
fn receipt_result(
    out: &ReceiptOut,
    png: Option<Vec<u8>>,
    format: ReceiptFormat,
) -> Result<CallToolResult, ErrorData> {
    let mut content = Vec::new();
    if format != ReceiptFormat::Image {
        content.push(ContentBlock::text(out.text.clone()));
    } else {
        content.push(ContentBlock::text(format!(
            "Receipt{} with {} lines (image below).",
            out.job_id.map(|id| format!(" {id}")).unwrap_or_default(),
            out.lines
        )));
    }
    if let Some(png) = png {
        content.push(ContentBlock::image(base64::encode(&png), "image/png"));
    }
    let mut result = CallToolResult::success(content);
    result.structured_content = Some(
        serde_json::to_value(out).map_err(|e| ErrorData::internal_error(e.to_string(), None))?,
    );
    Ok(result)
}

/// Parses ESC/POS bytes given as hex (spaces allowed) or base64.
fn decode_bytes(input: &str) -> Result<Vec<u8>, String> {
    let compact: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if !compact.is_empty()
        && compact.len().is_multiple_of(2)
        && compact.chars().all(|c| c.is_ascii_hexdigit())
    {
        return (0..compact.len())
            .step_by(2)
            .map(|i| {
                compact
                    .get(i..i + 2)
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                    .ok_or_else(|| "bad hex".to_owned())
            })
            .collect();
    }
    base64::decode(&compact).map_err(|e| format!("bytes are neither hex nor base64: {e}"))
}

fn paper(width: Option<u16>) -> Result<PaperWidth, String> {
    match width {
        None | Some(80) => Ok(PaperWidth::Mm80),
        Some(58) => Ok(PaperWidth::Mm58),
        Some(other) => Err(format!("width must be 58 or 80, not {other}")),
    }
}

fn node_type(node: &hardless_escpos::Node) -> String {
    serde_json::to_value(node)
        .ok()
        .and_then(|v| v["type"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "?".to_owned())
}

#[tool_router]
impl HardlessMcp {
    /// A server over `rest`. With `read_only`, mutating tools are hidden (FR-48).
    pub fn new(rest: Arc<dyn Rest>, read_only: bool) -> Self {
        let mut tool_router = Self::tool_router();
        if read_only {
            for name in MUTATING_TOOLS {
                tool_router.remove_route(name);
            }
        }
        Self { rest, tool_router }
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<RestResponse, CallToolResult> {
        match self.rest.send(method, path, body).await {
            Ok(response) if response.ok() => Ok(response),
            Ok(response) => {
                let (code, message) = response.error();
                Err(failure(&code, &message))
            }
            Err(reason) => Err(failure(
                "unreachable",
                &format!("cannot reach Hardless: {reason}"),
            )),
        }
    }

    /// A job's PNG from `/render`.
    async fn png(
        &self,
        device_id: &str,
        job_id: u64,
        width: Option<u16>,
    ) -> Result<Vec<u8>, CallToolResult> {
        let width = match width {
            Some(w) => format!("&width={w}"),
            None => String::new(),
        };
        let path = format!(
            "/devices/{}/jobs/{job_id}/render?format=png{width}",
            segment(device_id)
        );
        Ok(self.call(Method::Get, &path, None).await?.body)
    }

    async fn receipt(
        &self,
        device_id: &str,
        job: &Value,
        format: ReceiptFormat,
        width: Option<u16>,
    ) -> Result<CallToolResult, ErrorData> {
        let out = receipt_out(Some(device_id), job);
        let png = if format == ReceiptFormat::Text {
            None
        } else {
            match self
                .png(device_id, out.job_id.unwrap_or_default(), width)
                .await
            {
                Ok(png) => Some(png),
                Err(failed) => return Ok(failed),
            }
        };
        receipt_result(&out, png, format)
    }

    #[tool(
        description = "List every emulated device with its kind, state and active faults. Use first to learn device IDs. Example input: {}",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<DevicesOut>()
    )]
    async fn list_devices(&self) -> Result<CallToolResult, ErrorData> {
        match self.call(Method::Get, "/devices", None).await {
            Ok(response) => structured(&DevicesOut {
                devices: response
                    .json()
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(device_out)
                    .collect(),
            }),
            Err(failed) => Ok(failed),
        }
    }

    #[tool(
        description = "Create a device from a kind and config. Use to add a second printer, a USB printer or a drawer. Example: {\"kind\":\"printer-escpos\",\"id\":\"kitchen\",\"config\":{\"paperWidth\":58}}",
        output_schema = schema_for_output::<DeviceOut>()
    )]
    async fn create_device(
        &self,
        Parameters(input): Parameters<CreateDeviceIn>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut body =
            json!({ "kind": input.kind, "config": input.config.unwrap_or_else(|| json!({})) });
        if let Some(id) = input.id {
            body["id"] = json!(id);
        }
        match self.call(Method::Post, "/devices", Some(body)).await {
            Ok(response) => structured(&device_out(&response.json())),
            Err(failed) => Ok(failed),
        }
    }

    #[tool(
        description = "Remove a device with its receipts and event log. Use to clean up devices you created. Example: {\"id\":\"kitchen\"}",
        annotations(destructive_hint = true),
        output_schema = schema_for_output::<OkOut>()
    )]
    async fn remove_device(
        &self,
        Parameters(input): Parameters<IdIn>,
    ) -> Result<CallToolResult, ErrorData> {
        match self
            .call(
                Method::Delete,
                &format!("/devices/{}", segment(&input.id)),
                None,
            )
            .await
        {
            Ok(_) => structured(&OkOut { ok: true }),
            Err(failed) => Ok(failed),
        }
    }

    #[tool(
        description = "Reset a device: clear its state, receipts and faults. Use before a test run for a clean start. Example: {\"id\":\"printer-1\"}",
        annotations(destructive_hint = true),
        output_schema = schema_for_output::<OkOut>()
    )]
    async fn reset_device(
        &self,
        Parameters(input): Parameters<IdIn>,
    ) -> Result<CallToolResult, ErrorData> {
        match self
            .call(
                Method::Post,
                &format!("/devices/{}/reset", segment(&input.id)),
                None,
            )
            .await
        {
            Ok(_) => structured(&OkOut { ok: true }),
            Err(failed) => Ok(failed),
        }
    }

    #[tool(
        description = "Get a printer's latest receipt as text, a PNG image and job metadata. Use to check what the app printed. Example: {\"deviceId\":\"printer-1\",\"format\":\"both\"}",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<ReceiptOut>()
    )]
    async fn get_last_receipt(
        &self,
        Parameters(input): Parameters<ReceiptIn>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = format!("/devices/{}/jobs/last", segment(&input.device_id));
        match self.rest.send(Method::Get, &path, None).await {
            Ok(response)
                if response.status == 404
                    && response.error().0 == "not_found"
                    && response.error().1.contains("job") =>
            {
                Ok(failure(
                    "no_jobs",
                    &format!("`{}` has not printed anything yet", input.device_id),
                ))
            }
            Ok(response) if response.ok() => {
                self.receipt(
                    &input.device_id,
                    &response.json(),
                    input.format,
                    input.width,
                )
                .await
            }
            Ok(response) => {
                let (code, message) = response.error();
                Ok(failure(&code, &message))
            }
            Err(reason) => Ok(failure(
                "unreachable",
                &format!("cannot reach Hardless: {reason}"),
            )),
        }
    }

    #[tool(
        description = "List a printer's receipts, newest first, with ID, time, first line and whether it was cut. Example: {\"deviceId\":\"printer-1\",\"limit\":5}",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<JobsOut>()
    )]
    async fn list_jobs(
        &self,
        Parameters(input): Parameters<ListJobsIn>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = format!(
            "/devices/{}/jobs?limit={}",
            segment(&input.device_id),
            input.limit.unwrap_or(20)
        );
        match self.call(Method::Get, &path, None).await {
            Ok(response) => structured(&JobsOut {
                jobs: response
                    .json()
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|job| {
                        let out = receipt_out(None, job);
                        JobSummary {
                            id: out.job_id.unwrap_or_default(),
                            at: out.at.unwrap_or_default(),
                            first_line: out
                                .text
                                .lines()
                                .map(str::trim)
                                .find(|l| !l.is_empty())
                                .unwrap_or_default()
                                .to_owned(),
                            lines: out.lines,
                            cut: out.cut,
                        }
                    })
                    .collect(),
            }),
            Err(failed) => Ok(failed),
        }
    }

    #[tool(
        description = "Get one receipt by job ID as text, PNG and metadata, like get_last_receipt. Example: {\"deviceId\":\"printer-1\",\"jobId\":3}",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<ReceiptOut>()
    )]
    async fn get_job(
        &self,
        Parameters(input): Parameters<GetJobIn>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = format!("/devices/{}/jobs?limit=1000", segment(&input.device_id));
        let jobs = match self.call(Method::Get, &path, None).await {
            Ok(response) => response.json(),
            Err(failed) => return Ok(failed),
        };
        let job = jobs
            .as_array()
            .into_iter()
            .flatten()
            .find(|j| j["id"].as_u64() == Some(input.job_id))
            .cloned();
        match job {
            Some(job) => {
                self.receipt(&input.device_id, &job, input.format, input.width)
                    .await
            }
            None => Ok(failure(
                "not_found",
                &format!(
                    "`{}` has no job {}; call `list_jobs`",
                    input.device_id, input.job_id
                ),
            )),
        }
    }

    #[tool(
        description = "Wait for the next receipt a printer prints, then return it like get_last_receipt. Call before triggering a print. Example: {\"deviceId\":\"printer-1\",\"timeoutMs\":10000}",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<ReceiptOut>()
    )]
    async fn wait_for_job(
        &self,
        Parameters(input): Parameters<WaitForJobIn>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = format!("/devices/{}/jobs?limit=1", segment(&input.device_id));
        let latest = |response: &RestResponse| response.json()[0].clone();
        let before = match self.call(Method::Get, &path, None).await {
            Ok(response) => latest(&response),
            Err(failed) => return Ok(failed),
        };
        let key = |job: &Value| (job["id"].as_u64(), job["at"].as_u64());
        let deadline = tokio::time::Instant::now()
            + Duration::from_millis(input.timeout_ms.unwrap_or(DEFAULT_WAIT_MS));
        loop {
            tokio::time::sleep(POLL).await;
            let now = match self.call(Method::Get, &path, None).await {
                Ok(response) => latest(&response),
                Err(failed) => return Ok(failed),
            };
            if now.is_object() && key(&now) != key(&before) {
                return self
                    .receipt(&input.device_id, &now, input.format, input.width)
                    .await;
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(failure(
                    "timeout",
                    &format!(
                        "`{}` printed nothing within {} ms",
                        input.device_id,
                        input.timeout_ms.unwrap_or(DEFAULT_WAIT_MS)
                    ),
                ));
            }
        }
    }

    #[tool(
        description = "Decode ESC/POS bytes (hex or base64) into text, a PNG and an AST summary, without any device. Use to check bytes your code builds. Example: {\"bytes\":\"1b40 48690a 1d5600\"}",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<ReceiptOut>()
    )]
    async fn decode_escpos(
        &self,
        Parameters(input): Parameters<DecodeIn>,
    ) -> Result<CallToolResult, ErrorData> {
        let bytes = match decode_bytes(&input.bytes) {
            Ok(bytes) => bytes,
            Err(message) => return Ok(failure("bad_request", &message)),
        };
        let paper = match paper(input.width) {
            Ok(paper) => paper,
            Err(message) => return Ok(failure("bad_request", &message)),
        };
        let nodes = hardless_escpos::parse(&bytes);
        let options = RenderOptions {
            paper,
            show_unknown: true,
        };
        let ast = serde_json::to_value(&nodes).unwrap_or(Value::Null);
        let job = json!({ "ast": ast, "text": to_text_with(&nodes, &options), "bytesHex": "00".repeat(bytes.len()) });
        let out = receipt_out(None, &job);
        let png = if input.format == ReceiptFormat::Text {
            None
        } else {
            match render_png(&nodes, &options) {
                Ok(png) => Some(png),
                Err(e) => return Ok(failure("receipt_too_long", &e.to_string())),
            }
        };
        receipt_result(&out, png, input.format)
    }

    #[tool(
        description = "Scan a barcode on a keyboard-wedge scanner: pages with the Hardless shim get the keys in the focused field. Example: {\"deviceId\":\"scanner-1\",\"code\":\"4006381333931\"}",
        output_schema = schema_for_output::<OkOut>()
    )]
    async fn scan_barcode(
        &self,
        Parameters(input): Parameters<ScanIn>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut body = json!({ "code": input.code });
        if let Some(t) = input.terminator {
            body["terminator"] = json!(t);
        }
        if let Some(d) = input.delay_ms {
            body["delayMs"] = json!(d);
        }
        match self
            .call(
                Method::Post,
                &format!("/devices/{}/scan", segment(&input.device_id)),
                Some(body),
            )
            .await
        {
            Ok(_) => structured(&OkOut { ok: true }),
            Err(failed) => Ok(failed),
        }
    }

    #[tool(
        description = "Tap an NFC card on a reader: pages scanning with NDEFReader get a reading. Example: {\"deviceId\":\"nfc-1\",\"uid\":\"04a2b3c4\",\"records\":[{\"recordType\":\"text\",\"data\":\"Member 1234\"}]}",
        output_schema = schema_for_output::<OkOut>()
    )]
    async fn tap_nfc(
        &self,
        Parameters(input): Parameters<TapIn>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut body = json!({ "uid": input.uid });
        if let Some(records) = input.records {
            body["records"] = serde_json::to_value(records).unwrap_or(Value::Null);
        }
        match self
            .call(
                Method::Post,
                &format!("/devices/{}/tap", segment(&input.device_id)),
                Some(body),
            )
            .await
        {
            Ok(_) => structured(&OkOut { ok: true }),
            Err(failed) => Ok(failed),
        }
    }

    #[tool(
        description = "Turn a fault on or off, e.g. paper-out, offline, read-error. Use to test the app's error handling. Example: {\"deviceId\":\"printer-1\",\"faultId\":\"paper-out\",\"active\":true}",
        output_schema = schema_for_output::<DeviceOut>()
    )]
    async fn set_fault(
        &self,
        Parameters(input): Parameters<SetFaultIn>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut body = json!({ "active": input.active });
        if let Some(options) = input.options {
            body["options"] = Value::Object(options);
        }
        let path = format!(
            "/devices/{}/faults/{}",
            segment(&input.device_id),
            segment(&input.fault_id)
        );
        match self.call(Method::Put, &path, Some(body)).await {
            Ok(response) => structured(&device_out(&response.json())),
            Err(failed) => Ok(failed),
        }
    }

    #[tool(
        description = "List the fault catalogue with effects and options, optionally only a device kind's faults. Example: {\"kind\":\"printer-escpos\"}",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<FaultsOut>()
    )]
    async fn list_faults(
        &self,
        Parameters(input): Parameters<ListFaultsIn>,
    ) -> Result<CallToolResult, ErrorData> {
        let allowed: Option<Vec<String>> = match &input.kind {
            None => None,
            Some(kind) => {
                let mut registry = Registry::new(FakeClock::new());
                let _ = hardless_devices::register_all(&mut registry);
                match registry.plugin(kind) {
                    Some(plugin) => Some(plugin.faults().iter().map(|f| f.to_string()).collect()),
                    None => {
                        return Ok(failure("unknown_kind", &format!("no device kind `{kind}`")))
                    }
                }
            }
        };
        let faults = catalogue()
            .iter()
            .filter(|spec| {
                allowed
                    .as_ref()
                    .is_none_or(|a| a.iter().any(|id| id == spec.id.as_str()))
            })
            .map(|spec| FaultOut {
                id: spec.id.to_string(),
                applies_to: spec.applies_to.to_owned(),
                effect: spec.effect.to_owned(),
                options: spec
                    .options
                    .iter()
                    .map(|o| FaultOptionOut {
                        name: o.name.to_owned(),
                        default: serde_json::to_value(o.default).unwrap_or(Value::Null),
                        description: o.description.to_owned(),
                    })
                    .collect(),
            })
            .collect();
        structured(&FaultsOut { faults })
    }

    #[tool(
        description = "Read a device's event log: bytes in and out (hex, with decoded ESC/POS commands), state changes, faults, scans and taps. Example: {\"deviceId\":\"printer-1\",\"limit\":50}",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<EventsOut>()
    )]
    async fn get_events(
        &self,
        Parameters(input): Parameters<GetEventsIn>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut path = format!(
            "/devices/{}/events?limit={}",
            segment(&input.device_id),
            input.limit.unwrap_or(100)
        );
        if let Some(since) = input.since {
            path.push_str(&format!("&since={since}"));
        }
        let records = match self.call(Method::Get, &path, None).await {
            Ok(response) => response.json(),
            Err(failed) => return Ok(failed),
        };
        let events = records
            .as_array()
            .into_iter()
            .flatten()
            .map(|record| {
                let typed: Option<EventRecord> = serde_json::from_value(record.clone()).ok();
                let direction = typed
                    .as_ref()
                    .and_then(EventRecord::direction)
                    .and_then(|d| serde_json::to_value(d).ok())
                    .and_then(|d| d.as_str().map(str::to_owned));
                let kind = record["type"].as_str().unwrap_or_default().to_owned();
                let data_hex = record["dataHex"].as_str().map(str::to_owned);
                let decoded = if kind == "bytes-in" {
                    data_hex
                        .as_deref()
                        .and_then(|hex| decode_bytes(hex).ok())
                        .map(|bytes| {
                            hardless_escpos::parse(&bytes)
                                .iter()
                                .map(node_type)
                                .collect()
                        })
                } else {
                    None
                };
                EventOut {
                    seq: record["seq"].as_u64().unwrap_or_default(),
                    at: record["at"].as_u64().unwrap_or_default(),
                    kind,
                    direction,
                    data_hex,
                    decoded,
                    record: record.clone(),
                }
            })
            .collect();
        structured(&EventsOut { events })
    }

    #[tool(
        description = "Run a scenario: timed steps (scan, tap, wait, setFault, clearFault, expect) on the server's devices; returns step results. Example: {\"steps\":[{\"type\":\"scan\",\"device\":\"scanner-1\",\"code\":\"123\"}]}",
        output_schema = schema_for_output::<Map<String, Value>>()
    )]
    async fn run_scenario(
        &self,
        Parameters(input): Parameters<ScenarioIn>,
    ) -> Result<CallToolResult, ErrorData> {
        let body = serde_json::to_value(&input).unwrap_or(Value::Null);
        match self.call(Method::Post, "/scenarios/run", Some(body)).await {
            Ok(response) => {
                let report = response.json();
                let mut result = CallToolResult::structured(report.clone());
                if report["ok"] == Value::Bool(false) {
                    result.is_error = Some(true);
                }
                Ok(result)
            }
            Err(failed) => Ok(failed),
        }
    }
}

const DEVICES_URI: &str = "hardless://devices";
const LAST_RECEIPT_TEMPLATE: &str = "hardless://devices/{id}/receipts/last";

/// A docs resource: its description and the function that writes it.
type DocResource = (&'static str, fn() -> String);

fn docs_resources() -> BTreeMap<&'static str, DocResource> {
    BTreeMap::from([
        (
            "hardless://docs/escpos-commands",
            (
                "Supported ESC/POS commands with byte sequences",
                docs::escpos_commands as fn() -> String,
            ),
        ),
        (
            "hardless://docs/faults",
            ("Fault catalogue", docs::faults as fn() -> String),
        ),
        (
            "hardless://docs/http-dialects",
            (
                "ePOS-Print-style and WebPRNT-style request and response reference",
                docs::http_dialects as fn() -> String,
            ),
        ),
    ])
}

fn text_resource(uri: &str, mime: &str, text: String) -> ReadResourceResponse {
    ReadResourceResult::new(vec![ResourceContents::text(text, uri).with_mime_type(mime)]).into()
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for HardlessMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new("hardless", env!("CARGO_PKG_VERSION")))
        .with_instructions(
            "Hardless emulates POS hardware (receipt printers, cash drawers, barcode scanners, NFC readers) for web apps. Call list_devices first. After the app prints, call wait_for_job or get_last_receipt to see the receipt as text and PNG; use set_fault to test error handling.",
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let mut resources = vec![Resource::new(DEVICES_URI, "devices")
            .with_description("Every device as JSON")
            .with_mime_type("application/json")];
        for (uri, (title, _)) in docs_resources() {
            resources.push(
                Resource::new(uri, uri.trim_start_matches("hardless://docs/"))
                    .with_description(title)
                    .with_mime_type("text/markdown"),
            );
        }
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        Ok(ListResourceTemplatesResult::with_all_items(vec![
            ResourceTemplate::new(LAST_RECEIPT_TEMPLATE, "last-receipt")
                .with_description("A printer's latest receipt as plain text")
                .with_mime_type("text/plain"),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let uri = request.uri.as_str();
        if let Some((_, make)) = docs_resources().get(uri) {
            return Ok(text_resource(uri, "text/markdown", make()));
        }
        let unreachable =
            |e: String| ErrorData::internal_error(format!("cannot reach Hardless: {e}"), None);
        if uri == DEVICES_URI {
            let response = self
                .rest
                .send(Method::Get, "/devices", None)
                .await
                .map_err(unreachable)?;
            return Ok(text_resource(
                uri,
                "application/json",
                String::from_utf8_lossy(&response.body).into_owned(),
            ));
        }
        if let Some(id) = uri
            .strip_prefix("hardless://devices/")
            .and_then(|rest| rest.strip_suffix("/receipts/last"))
        {
            let response = self
                .rest
                .send(
                    Method::Get,
                    &format!("/devices/{}/jobs/last", segment(id)),
                    None,
                )
                .await
                .map_err(unreachable)?;
            if !response.ok() {
                let (code, message) = response.error();
                return Err(ErrorData::resource_not_found(
                    format!("{message} ({code})"),
                    None,
                ));
            }
            let text = response.json()["text"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            return Ok(text_resource(uri, "text/plain", text));
        }
        Err(ErrorData::resource_not_found(
            format!("no resource `{uri}`"),
            None,
        ))
    }
}

/// Every tool's definition as JSON (name, description, annotations, input and output
/// schemas), for the generated tool reference.
pub fn tool_list() -> Vec<Value> {
    HardlessMcp::tool_router()
        .list_all()
        .iter()
        .filter_map(|tool| serde_json::to_value(tool).ok())
        .collect()
}
