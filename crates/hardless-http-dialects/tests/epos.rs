//! FR-31 and FR-34: the ePOS-Print-compatible dialect.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use hardless_escpos::{parse, HriPosition, Node, QrErrorCorrection, Symbology};
use hardless_http_dialects::{epos, Outcome, PrinterStatus};
use hardless_render::to_text;

fn envelope(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Body>
    <epos-print xmlns="http://www.epson-pos.com/schemas/2011/03/epos-print">{body}</epos-print>
  </s:Body>
</s:Envelope>"#
    )
}

fn escpos(body: &str) -> Vec<u8> {
    epos::parse(&envelope(body)).unwrap().print.escpos
}

fn ast(body: &str) -> Vec<Node> {
    parse(&escpos(body))
}

#[test]
fn fr_31_a_receipt_prints_the_same_as_over_web_serial() {
    let body = r#"<text align="center"/><text dw="true" dh="true">HARDLESS CAFE&#10;</text>
        <text dw="false" dh="false"/><text align="left"/>
        <text>2 x Flat white   900&#10;</text>
        <text em="true">Total:  1,250&#10;</text><text em="false"/>
        <cut type="feed"/>"#;
    let via_http = ast(body);
    // The same receipt written directly in ESC/POS.
    let direct = parse(
        b"\x1b@\x1ba\x01\x1d!\x11HARDLESS CAFE\n\x1d!\x00\x1ba\x00\
          2 x Flat white   900\n\x1bE\x01Total:  1,250\n\x1bE\x00\x1dVB\x00",
    );
    assert_eq!(via_http, direct);
    assert_eq!(
        to_text(&via_http),
        format!(
            "{}HARDLESS CAFE\n2 x Flat white   900\nTotal:  1,250\n",
            " ".repeat(11)
        )
    );
}

#[test]
fn fr_31_soap_header_parameters_and_bare_documents() {
    let doc = r#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
      <s:Header><parameter xmlns="http://www.epson-pos.com/schemas/2011/03/epos-print">
        <devid>local_printer</devid><timeout>60000</timeout><printjobid>ABC123</printjobid>
      </parameter></s:Header>
      <s:Body><epos-print xmlns="http://www.epson-pos.com/schemas/2011/03/epos-print"><text>x</text></epos-print></s:Body>
    </s:Envelope>"#;
    let req = epos::parse(doc).unwrap();
    assert_eq!(req.devid.as_deref(), Some("local_printer"));
    assert_eq!(req.timeout_ms, Some(60_000));
    assert_eq!(req.print_job_id.as_deref(), Some("ABC123"));

    let bare = epos::parse(
        r#"<epos-print xmlns="http://www.epson-pos.com/schemas/2011/03/epos-print"><text>x</text></epos-print>"#,
    )
    .unwrap();
    assert_eq!(bare.print.escpos, b"\x1b@x");
}

#[test]
fn fr_31_barcodes_qr_codes_images_and_drawer_kicks() {
    let nodes = ast(
        r#"<barcode type="ean13" hri="below" width="2" height="60">4006381333931</barcode>
        <barcode type="code128">ORDER-1</barcode>
        <symbol type="qrcode_model_2" level="level_q" width="4">https://example.com</symbol>
        <image width="8" height="2">/wA=</image>
        <pulse drawer="drawer_2" time="pulse_200"/>
        <feed line="3"/>"#,
    );
    assert!(nodes.contains(&Node::HriPosition {
        position: HriPosition::Below
    }));
    assert!(nodes.contains(&Node::Barcode {
        symbology: Symbology::Ean13,
        data: "4006381333931".into()
    }));
    assert!(nodes.contains(&Node::Barcode {
        symbology: Symbology::Code128,
        data: "{BORDER-1".into()
    }));
    assert!(nodes.contains(&Node::QrErrorCorrection {
        level: QrErrorCorrection::Q
    }));
    assert!(nodes
        .iter()
        .any(|n| matches!(n, Node::QrStore { text, .. } if text == "https://example.com")));
    assert!(nodes.contains(&Node::QrPrint));
    assert!(nodes.iter().any(|n| matches!(
        n,
        Node::RasterImage { width_bytes: 1, height: 2, data, .. } if data == &[0xFF, 0x00]
    )));
    assert!(nodes.contains(&Node::Pulse {
        pin: 5,
        on_ms: 200,
        off_ms: 200,
        realtime: false
    }));
    assert!(nodes.contains(&Node::Feed { lines: 3 }));
}

#[test]
fn fr_31_escapes_in_barcode_data_and_raw_commands() {
    let bytes = escpos(r#"<barcode type="code128">{A\x41\\</barcode><command>1b6401</command>"#);
    assert!(bytes.windows(4).any(|w| w == b"{AA\\"), "{bytes:?}");
    assert!(bytes.ends_with(b"\x1bd\x01"));
}

#[test]
fn fr_31_unsupported_features_print_what_they_can_and_warn() {
    let req = epos::parse(&envelope(
        r#"<sound pattern="pattern_a"/><page><area x="0" y="0" width="100" height="100"/><text>in page</text></page>
           <logo key1="48" key2="48"/><text reverse="true">r</text>
           <symbol type="pdf417_standard">PDF</symbol>"#,
    ))
    .unwrap();
    assert_eq!(req.print.warnings.len(), 4, "{:?}", req.print.warnings);
    let nodes = parse(&req.print.escpos);
    assert!(nodes
        .iter()
        .any(|n| matches!(n, Node::Text { text } if text == "in page")));
    let unknown = nodes
        .iter()
        .filter(|n| matches!(n, Node::Unknown { .. }))
        .count();
    assert_eq!(unknown, 3, "logo and PDF417 show as unknown: {nodes:?}");
}

#[test]
fn fr_31_euro_sign_switches_to_cp858() {
    let text = to_text(&ast("<text>Total €5&#10;</text>"));
    assert_eq!(text, "Total €5\n");
}

#[test]
fn fr_31_bad_documents_are_schema_errors() {
    for doc in [
        "not xml".to_owned(),
        "<html/>".to_owned(),
        envelope("<blink>x</blink>"),
        envelope(r#"<text align="middle">x</text>"#),
        envelope(r#"<cut type="sideways"/>"#),
        envelope(r#"<barcode type="code11">1</barcode>"#),
        envelope("<command>1b4</command>"),
        r#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body/></s:Envelope>"#
            .to_owned(),
    ] {
        assert!(epos::parse(&doc).is_err(), "{doc}");
    }
}

#[test]
fn fr_34_responses_carry_success_code_and_status_bits() {
    let ok = PrinterStatus {
        online: true,
        ..Default::default()
    };
    let printed = epos::response(&Outcome::Printed, &ok, Some("A&B"));
    assert!(
        printed.contains(r#"success="true" code="" status="2" battery="0""#),
        "{printed}"
    );
    assert!(printed.contains("<printjobid>A&amp;B</printjobid>"));

    let cover = PrinterStatus {
        cover_open: true,
        drawer_pin3_high: true,
        ..Default::default()
    };
    let xml = epos::response(&Outcome::CoverOpen, &cover, None);
    let bits = epos::status::COVER_OPEN | epos::status::OFFLINE | epos::status::DRAWER_KICK;
    assert!(xml.contains(&format!(
        r#"success="false" code="EPTR_COVER_OPEN" status="{bits}""#
    )));

    for (outcome, code) in [
        (Outcome::PaperEnd, "EPTR_REC_EMPTY"),
        (Outcome::Timeout, "EX_TIMEOUT"),
        (Outcome::BadDocument("x".into()), "SchemaError"),
        (Outcome::DeviceNotFound, "DeviceNotFound"),
    ] {
        assert_eq!(epos::error_code(&outcome), code);
    }
    assert_eq!(
        epos::status_bits(&Outcome::DeviceNotFound, &ok),
        0,
        "no printer, no status"
    );
}

#[test]
fn fr_34_status_comes_from_the_printers_dle_eot_replies() {
    // Paper near end only: still online, DLE EOT 4 bits 2-3.
    let status = PrinterStatus::from_dle_eot(0x12, 0x12, 0x1E);
    assert!(status.online && status.paper_near_end && !status.paper_end);
    assert_eq!(Outcome::after_print(&status), Outcome::Printed);
    assert_eq!(
        epos::status_bits(&Outcome::Printed, &status),
        epos::status::PRINT_SUCCESS | epos::status::RECEIPT_NEAR_END
    );
    let cover = PrinterStatus::from_dle_eot(0x1A, 0x16, 0x12);
    assert_eq!(Outcome::after_print(&cover), Outcome::CoverOpen);
}
