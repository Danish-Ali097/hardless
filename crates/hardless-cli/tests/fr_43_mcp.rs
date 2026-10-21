//! FR-43 to FR-50 and the M3 acceptance criterion: an MCP client (the SDK's own) talks to
//! `hardless mcp` over stdio against a live server, calls every tool, checks the output
//! schemas, gets a PNG from `get_last_receipt`, and sees only read tools with `--read-only`.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use std::collections::BTreeSet;

use hardless_server::{spawn, RunningServer, ServerConfig};
use rmcp::model::{CallToolRequestParams, CallToolResult, ReadResourceRequestParams, Tool};
use rmcp::service::RunningService;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use serde_json::{json, Value};

const READ_TOOLS: [&str; 8] = [
    "decode_escpos",
    "get_events",
    "get_job",
    "get_last_receipt",
    "list_devices",
    "list_faults",
    "list_jobs",
    "wait_for_job",
];
const WRITE_TOOLS: [&str; 7] = [
    "create_device",
    "remove_device",
    "reset_device",
    "run_scenario",
    "scan_barcode",
    "set_fault",
    "tap_nfc",
];

async fn server() -> RunningServer {
    spawn(ServerConfig {
        port: 0,
        https: false,
        ..ServerConfig::default()
    })
    .await
    .unwrap()
}

async fn client(server: &RunningServer, extra: &[&str]) -> RunningService<RoleClient, ()> {
    let url = format!("http://{}", server.addr);
    let extra: Vec<String> = extra.iter().map(|s| (*s).to_owned()).collect();
    let transport = TokioChildProcess::new(
        tokio::process::Command::new(env!("CARGO_BIN_EXE_hardless")).configure(|cmd| {
            cmd.args(["mcp", "--url", &url]).args(&extra);
        }),
    )
    .unwrap();
    ().serve(transport).await.unwrap()
}

async fn call(client: &RunningService<RoleClient, ()>, name: &str, args: Value) -> CallToolResult {
    let params = CallToolRequestParams::new(name.to_owned())
        .with_arguments(args.as_object().cloned().unwrap_or_default());
    client.call_tool(params).await.unwrap()
}

/// The structured content has every property the output schema requires, and no property
/// the schema does not know.
fn check_schema(tool: &Tool, result: &CallToolResult) {
    let schema = tool.output_schema.as_ref().expect("an output schema");
    let content = result
        .structured_content
        .as_ref()
        .unwrap_or_else(|| panic!("{}: no structured content", tool.name));
    let object = content
        .as_object()
        .expect("structured content is an object");
    let props = schema.get("properties").and_then(Value::as_object);
    for required in schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = required.as_str().unwrap();
        assert!(object.contains_key(name), "{}: missing `{name}`", tool.name);
    }
    if let Some(props) = props {
        for key in object.keys() {
            assert!(
                props.contains_key(key),
                "{}: `{key}` is not in the schema",
                tool.name
            );
        }
    }
}

fn text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn png(result: &CallToolResult) -> Option<Vec<u8>> {
    result.content.iter().find_map(|c| {
        c.as_image().map(|image| {
            assert_eq!(image.mime_type, "image/png");
            hardless_core::base64::decode(&image.data).unwrap()
        })
    })
}

fn print(server: &RunningServer, bytes: &[u8]) {
    let id: hardless_core::DeviceId = "printer-1".parse().unwrap();
    server
        .engine
        .drive(&id, |r| r.host_bytes(&id, bytes))
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn fr_45_every_tool_works_against_a_live_server_with_its_output_schema() {
    let server = server().await;
    let client = client(&server, &[]).await;
    let tools = client.list_all_tools().await.unwrap();
    let names: BTreeSet<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    let expected: BTreeSet<&str> = READ_TOOLS
        .iter()
        .chain(WRITE_TOOLS.iter())
        .copied()
        .collect();
    assert_eq!(names, expected);
    let tool = |name: &str| tools.iter().find(|t| t.name == name).unwrap().clone();

    // FR-48 and FR-50: hints, and short descriptions with an example.
    for t in &tools {
        let description = t.description.as_deref().unwrap_or_default();
        assert!(
            description.chars().count() < 300,
            "{}: {description}",
            t.name
        );
        assert!(description.contains("Example"), "{}", t.name);
        let annotations = t.annotations.clone().unwrap_or_default();
        assert_eq!(
            annotations.read_only_hint == Some(true),
            READ_TOOLS.contains(&t.name.as_ref()),
            "{}",
            t.name
        );
        assert_eq!(
            annotations.destructive_hint == Some(true),
            ["remove_device", "reset_device"].contains(&t.name.as_ref()),
            "{}",
            t.name
        );
    }

    let check = |name: &str, result: &CallToolResult| {
        assert_ne!(result.is_error, Some(true), "{name}: {}", text(result));
        check_schema(&tool(name), result);
    };

    let devices = call(&client, "list_devices", json!({})).await;
    check("list_devices", &devices);
    assert!(text(&devices).contains("printer-1"));

    let created = call(
        &client,
        "create_device",
        json!({ "kind": "printer-escpos", "id": "kitchen", "config": { "paperWidth": 58 } }),
    )
    .await;
    check("create_device", &created);
    check(
        "remove_device",
        &call(&client, "remove_device", json!({ "id": "kitchen" })).await,
    );
    check(
        "reset_device",
        &call(&client, "reset_device", json!({ "id": "printer-1" })).await,
    );

    // wait_for_job returns the next print.
    let waiting = {
        let params = CallToolRequestParams::new("wait_for_job".to_owned()).with_arguments(
            json!({ "deviceId": "printer-1", "timeoutMs": 5000 })
                .as_object()
                .cloned()
                .unwrap(),
        );
        client.call_tool(params)
    };
    let printing = async {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        print(&server, b"\x1b@CORNER CAFE\nTotal: 1,250\n\x1dV\x00");
    };
    let (waited, ()) = tokio::join!(waiting, printing);
    let waited: CallToolResult = waited.unwrap();
    check("wait_for_job", &waited);
    assert!(text(&waited).contains("Total: 1,250"));

    // FR-47: text, structured content and a PNG.
    let last = call(
        &client,
        "get_last_receipt",
        json!({ "deviceId": "printer-1" }),
    )
    .await;
    check("get_last_receipt", &last);
    assert!(png(&last).unwrap().starts_with(b"\x89PNG"));
    let structured = last.structured_content.clone().unwrap();
    assert_eq!(
        (structured["jobId"].as_u64(), structured["cut"].as_bool()),
        (Some(1), Some(true))
    );
    let text_only = call(
        &client,
        "get_last_receipt",
        json!({ "deviceId": "printer-1", "format": "text" }),
    )
    .await;
    assert!(png(&text_only).is_none());

    check(
        "list_jobs",
        &call(&client, "list_jobs", json!({ "deviceId": "printer-1" })).await,
    );
    check(
        "get_job",
        &call(
            &client,
            "get_job",
            json!({ "deviceId": "printer-1", "jobId": 1 }),
        )
        .await,
    );
    let decoded = call(
        &client,
        "decode_escpos",
        json!({ "bytes": "1b40 48690a 1d5600" }),
    )
    .await;
    check("decode_escpos", &decoded);
    assert!(text(&decoded).starts_with("Hi"));
    assert!(png(&decoded).is_some());

    check(
        "scan_barcode",
        &call(
            &client,
            "scan_barcode",
            json!({ "deviceId": "scanner-1", "code": "4006381333931" }),
        )
        .await,
    );
    check("tap_nfc", &call(&client, "tap_nfc", json!({ "deviceId": "nfc-1", "uid": "04a2b3c4", "records": [{ "recordType": "text", "data": "Hi" }] })).await);
    let fault = call(
        &client,
        "set_fault",
        json!({ "deviceId": "printer-1", "faultId": "paper-out", "active": true }),
    )
    .await;
    check("set_fault", &fault);
    assert_eq!(
        fault.structured_content.unwrap()["faults"],
        json!(["paper-out"])
    );
    let faults = call(&client, "list_faults", json!({ "kind": "nfc-reader" })).await;
    check("list_faults", &faults);
    assert_eq!(
        faults.structured_content.unwrap()["faults"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let events = call(&client, "get_events", json!({ "deviceId": "printer-1" })).await;
    check("get_events", &events);
    let decoded_events = events.structured_content.unwrap()["events"].clone();
    assert!(decoded_events
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["decoded"]
            .as_array()
            .is_some_and(|d| d.contains(&json!("cut")))));
    let scenario = call(
        &client,
        "run_scenario",
        json!({ "steps": [{ "type": "clearFault", "device": "printer-1", "fault": "paper-out" }] }),
    )
    .await;
    check("run_scenario", &scenario);

    // FR-49: errors carry the code and a next step.
    let missing = call(&client, "get_last_receipt", json!({ "deviceId": "nope" })).await;
    assert_eq!(missing.is_error, Some(true));
    assert!(
        text(&missing).contains("list_devices"),
        "{}",
        text(&missing)
    );

    // FR-46: resources.
    let resources = client.list_all_resources().await.unwrap();
    let uris: BTreeSet<String> = resources.iter().map(|r| r.uri.clone()).collect();
    for uri in [
        "hardless://devices",
        "hardless://docs/escpos-commands",
        "hardless://docs/faults",
        "hardless://docs/http-dialects",
    ] {
        assert!(uris.contains(uri), "{uri}");
    }
    let receipt = client
        .read_resource(ReadResourceRequestParams::new(
            "hardless://devices/printer-1/receipts/last",
        ))
        .await
        .unwrap();
    assert!(format!("{receipt:?}").contains("Total: 1,250"));

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn fr_48_read_only_offers_only_read_tools() {
    let server = server().await;
    let client = client(&server, &["--read-only"]).await;
    let names: BTreeSet<String> = client
        .list_all_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.name.into_owned())
        .collect();
    let expected: BTreeSet<String> = READ_TOOLS.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(names, expected);
    client.cancel().await.unwrap();
}
