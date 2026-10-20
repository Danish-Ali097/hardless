//! FR-19 and FR-31 to FR-35: the HTTP printer endpoints (PRD section 9.1).
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

mod common;

use std::time::{Duration, Instant};

use common::{get_json, http_text, raw_response, send_json, server, server_with};
use hardless_server::ServerConfig;
use serde_json::json;

const EPOS: &str = "/cgi-bin/epos/service.cgi?devid=local_printer&timeout=10000";
const XML: Option<&str> = Some("text/xml; charset=utf-8");

fn envelope(body: &str) -> String {
    format!(
        r#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><epos-print xmlns="http://www.epson-pos.com/schemas/2011/03/epos-print">{body}</epos-print></s:Body></s:Envelope>"#
    )
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

async fn fault(server: &hardless_server::RunningServer, fault: &str, options: serde_json::Value) {
    let (status, body) = send_json(
        server,
        "PUT",
        &format!("/api/v1/devices/local_printer/faults/{fault}"),
        json!({ "active": true, "options": options }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
}

#[tokio::test]
async fn fr_31_an_epos_print_becomes_a_job_with_the_same_text() {
    let server = server().await;
    let doc = envelope(r#"<text em="true">Total: 1,250&#10;</text><cut type="feed"/>"#);
    let (status, headers, body) = http_text(&server, "POST", EPOS, &doc, XML, &[]).await;
    assert_eq!(status, 200, "{body}");
    assert!(header(&headers, "content-type")
        .unwrap()
        .starts_with("text/xml"));
    assert!(
        body.contains(r#"success="true" code="" status="2""#),
        "{body}"
    );
    let (_, job) = get_json(&server, "/api/v1/devices/local_printer/jobs/last").await;
    assert_eq!(job["text"], "Total: 1,250\n");
    assert_eq!(job["ast"][1]["type"], "bold", "same AST as ESC/POS");
}

#[tokio::test]
async fn fr_35_epos_printers_are_addressed_by_devid() {
    let server = server().await;
    let doc = envelope("<text>x</text>");
    // No devid: local_printer answers.
    let (status, _, body) =
        http_text(&server, "POST", "/cgi-bin/epos/service.cgi", &doc, XML, &[]).await;
    assert_eq!(status, 200);
    assert!(body.contains(r#"success="true""#), "{body}");
    // A Web Serial printer is not on HTTP.
    let path = "/cgi-bin/epos/service.cgi?devid=printer-1";
    let (_, _, body) = http_text(&server, "POST", path, &doc, XML, &[]).await;
    assert!(body.contains(r#"code="DeviceNotFound""#), "{body}");
}

#[tokio::test]
async fn fr_31_bad_documents_are_schema_errors_and_logged() {
    let server = server().await;
    let doc = envelope("<blink>x</blink>");
    let (status, _, body) = http_text(&server, "POST", EPOS, &doc, XML, &[]).await;
    assert_eq!(status, 200);
    assert!(
        body.contains(r#"success="false" code="SchemaError""#),
        "{body}"
    );
    let (_, events) = get_json(&server, "/api/v1/devices/local_printer/events").await;
    assert!(events
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["type"] == "log" && e["message"].as_str().unwrap().contains("<blink>")));
}

#[tokio::test]
async fn fr_32_cors_echoes_the_origin_unless_blocked() {
    let server = server().await;
    let origin = ("Origin", "https://pos.example.com");
    let preflight = [
        origin,
        ("Access-Control-Request-Method", "POST"),
        (
            "Access-Control-Request-Headers",
            "content-type,soapaction,if-modified-since",
        ),
    ];
    let (status, headers, _) = http_text(&server, "OPTIONS", EPOS, "", None, &preflight).await;
    assert_eq!(status, 204);
    assert_eq!(
        header(&headers, "access-control-allow-origin"),
        Some("https://pos.example.com")
    );
    assert_eq!(
        header(&headers, "access-control-allow-methods"),
        Some("POST, OPTIONS")
    );
    let allowed = header(&headers, "access-control-allow-headers").unwrap();
    for name in ["Content-Type", "SOAPAction", "If-Modified-Since"] {
        assert!(allowed.contains(name), "{allowed}");
    }
    let (_, headers, _) = http_text(
        &server,
        "POST",
        EPOS,
        &envelope("<text>x</text>"),
        XML,
        &[origin],
    )
    .await;
    assert!(header(&headers, "access-control-allow-origin").is_some());

    fault(&server, "cors-blocked", json!({})).await;
    let (_, headers, _) = http_text(&server, "OPTIONS", EPOS, "", None, &preflight).await;
    assert_eq!(header(&headers, "access-control-allow-origin"), None);
}

#[tokio::test]
async fn fr_32_private_network_access_preflight_is_allowed() {
    let server = server().await;
    let (_, headers, _) = http_text(
        &server,
        "OPTIONS",
        "/StarWebPRNT/SendMessage",
        "",
        None,
        &[
            ("Origin", "https://pos.example.com"),
            ("Access-Control-Request-Private-Network", "true"),
        ],
    )
    .await;
    assert_eq!(
        header(&headers, "access-control-allow-private-network"),
        Some("true")
    );
}

#[tokio::test]
async fn fr_34_paper_out_and_cover_open_fail_with_their_codes() {
    let server = server().await;
    fault(&server, "paper-out", json!({})).await;
    let (_, _, body) =
        http_text(&server, "POST", EPOS, &envelope("<text>x</text>"), XML, &[]).await;
    assert!(
        body.contains(r#"success="false" code="EPTR_REC_EMPTY""#),
        "{body}"
    );
    let (_, jobs) = get_json(&server, "/api/v1/devices/local_printer/jobs").await;
    assert_eq!(jobs, json!([]), "nothing printed");

    let (_, _, body) = http_text(
        &server,
        "POST",
        "/StarWebPRNT/SendMessage",
        "<root><text>x</text></root>",
        XML,
        &[],
    )
    .await;
    assert!(
        body.contains("<success>false</success><code>1100</code>"),
        "{body}"
    );
}

#[tokio::test]
async fn fault_timeout_holds_the_response_past_the_requests_timeout() {
    let server = server().await;
    fault(&server, "timeout", json!({})).await;
    let start = Instant::now();
    let path = "/cgi-bin/epos/service.cgi?devid=local_printer&timeout=200";
    let (_, _, body) =
        http_text(&server, "POST", path, &envelope("<text>x</text>"), XML, &[]).await;
    assert!(
        start.elapsed() >= Duration::from_millis(700),
        "{:?}",
        start.elapsed()
    );
    assert!(body.contains(r#"code="EX_TIMEOUT""#), "{body}");
}

#[tokio::test]
async fn fault_offline_drops_the_connection_without_an_answer() {
    let server = server().await;
    fault(&server, "offline", json!({})).await;
    let raw = raw_response(&server, "POST", EPOS, &envelope("<text>x</text>"), XML, &[]).await;
    let text = String::from_utf8_lossy(&raw);
    assert!(!text.contains("success="), "no ePOS answer: {text:?}");
}

#[tokio::test]
async fn fr_31_a_webprnt_print_becomes_a_job() {
    let server = server().await;
    let doc =
        r#"<root><text emphasis="true">Total: 1,250\x0a</text><cutpaper type="partial"/></root>"#;
    for path in [
        "/StarWebPRNT/SendMessage",
        "/p/local_printer/StarWebPRNT/SendMessage",
    ] {
        let (status, _, body) = http_text(&server, "POST", path, doc, XML, &[]).await;
        assert_eq!(status, 200);
        assert!(
            body.contains("<success>true</success><code>0</code>"),
            "{body}"
        );
    }
    let (_, jobs) = get_json(&server, "/api/v1/devices/local_printer/jobs").await;
    assert_eq!(jobs.as_array().unwrap().len(), 2);
    assert_eq!(jobs[0]["text"], "Total: 1,250\n");

    let (status, _, _) = http_text(
        &server,
        "POST",
        "/p/printer-1/StarWebPRNT/SendMessage",
        doc,
        XML,
        &[],
    )
    .await;
    assert_eq!(status, 404, "printer-1 is a Web Serial printer");
}

#[tokio::test]
async fn fr_32_printer_endpoints_need_no_token() {
    let server = server_with(ServerConfig {
        port: 0,
        https: false,
        token: Some("secret".into()),
        ..ServerConfig::default()
    })
    .await;
    let (status, _, body) =
        http_text(&server, "POST", EPOS, &envelope("<text>x</text>"), XML, &[]).await;
    assert_eq!(status, 200);
    assert!(body.contains(r#"success="true""#), "{body}");
}
