//! FR-31 and FR-34: the WebPRNT-compatible dialect.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use hardless_escpos::{parse, Node, Symbology};
use hardless_http_dialects::{webprnt, Outcome, PrinterStatus};
use hardless_render::to_text;
use proptest::prelude::*;

fn ast(body: &str) -> Vec<Node> {
    parse(
        &webprnt::parse(&format!("<root>{body}</root>"))
            .unwrap()
            .escpos,
    )
}

#[test]
fn fr_31_a_receipt_prints_the_same_as_over_web_serial() {
    let via_http = ast(r#"<alignment position="center"/>
        <text width="2" height="2">HARDLESS CAFE\x0a</text>
        <text width="1" height="1"/><alignment position="left"/>
        <text>2 x Flat white   900\x0a</text>
        <text emphasis="true">Total:  1,250\x0a</text><text emphasis="false"/>
        <cutpaper type="partial" feed="true"/>"#);
    let direct = parse(
        b"\x1b@\x1ba\x01\x1d!\x11HARDLESS CAFE\n\x1d!\x00\x1ba\x00\
          2 x Flat white   900\n\x1bE\x01Total:  1,250\n\x1bE\x00\x1dVB\x00",
    );
    // XML whitespace between elements is not text; only the escapes make line feeds.
    assert_eq!(via_http, direct);
    assert!(to_text(&via_http).contains("Total:  1,250"));
}

#[test]
fn fr_31_barcodes_qr_codes_images_and_drawer_kicks() {
    let nodes = ast(
        r#"<barcode symbology="JAN13" width="width2" height="60">4006381333931</barcode>
        <barcode symbology="Code128" hri="false">ORDER-1</barcode>
        <qrcode model="model2" level="level_h" cell="4">https://example.com</qrcode>
        <bitimage width="8" height="1">gA==</bitimage>
        <peripheral channel="1" on="100" off="300"/>
        <rawdata>G2QC</rawdata>"#,
    );
    assert!(nodes.contains(&Node::Barcode {
        symbology: Symbology::Ean13,
        data: "4006381333931".into()
    }));
    assert!(nodes.contains(&Node::Barcode {
        symbology: Symbology::Code128,
        data: "{BORDER-1".into()
    }));
    assert!(nodes.contains(&Node::QrPrint));
    assert!(nodes.iter().any(
        |n| matches!(n, Node::RasterImage { width_bytes: 1, height: 1, data, .. } if data == &[0x80])
    ));
    assert!(nodes.contains(&Node::Pulse {
        pin: 2,
        on_ms: 100,
        off_ms: 300,
        realtime: false
    }));
    assert!(
        nodes.contains(&Node::Feed { lines: 2 }),
        "raw ESC d 2: {nodes:?}"
    );
}

#[test]
fn fr_31_initialization_and_the_alignment_typo() {
    let nodes = ast(r#"<initialization reset="true"/><alignment poistion="right"/>"#);
    assert_eq!(nodes.iter().filter(|n| **n == Node::Init).count(), 2);
}

#[test]
fn fr_31_bad_documents_are_errors() {
    for doc in [
        "<html/>",
        "<root><marquee/></root>",
        r#"<root><alignment position="middle"/></root>"#,
        r#"<root><barcode symbology="Code11">1</barcode></root>"#,
        r#"<root><bitimage width="8" height="1">%%%</bitimage></root>"#,
    ] {
        assert!(webprnt::parse(doc).is_err(), "{doc}");
    }
}

#[test]
fn fr_34_responses_carry_star_status_bytes() {
    let paper_out = PrinterStatus {
        paper_end: true,
        paper_near_end: true,
        ..Default::default()
    };
    let asb = webprnt::asb(&paper_out);
    assert_eq!(&asb[..2], &[0x23, 0x86]);
    assert_eq!(asb[2] & 0x08, 0x08, "offline");
    assert_eq!(asb[5] & 0x0C, 0x0C, "near end and paper end");
    let xml = webprnt::response(&Outcome::PaperEnd, &paper_out);
    assert!(
        xml.contains("<success>false</success><code>1100</code>"),
        "{xml}"
    );

    let cover = webprnt::asb(&PrinterStatus {
        online: false,
        cover_open: true,
        drawer_pin3_high: true,
        ..Default::default()
    });
    assert_eq!(cover[2], 0x08 | 0x20 | 0x04);
}

proptest! {
    #[test]
    fn parsers_never_panic(doc in "\\PC{0,400}") {
        let _ = webprnt::parse(&doc);
        let _ = hardless_http_dialects::epos::parse(&doc);
    }

    #[test]
    fn element_soup_never_panics(
        names in prop::collection::vec(
            prop::sample::select(vec!["text", "barcode", "qrcode", "bitimage", "feed", "cutpaper", "peripheral", "symbol", "image", "pulse", "cut", "command"]),
            0..8,
        ),
        attr in "[a-z_]{1,8}",
        value in "\\PC{0,12}",
        content in "\\PC{0,40}",
    ) {
        let body: String = names
            .iter()
            .map(|n| format!("<{n} {attr}=\"{}\">{}</{n}>", escape(&value), escape(&content)))
            .collect();
        let _ = webprnt::parse(&format!("<root>{body}</root>"));
        let _ = hardless_http_dialects::epos::parse(&format!(
            "<epos-print xmlns=\"http://www.epson-pos.com/schemas/2011/03/epos-print\">{body}</epos-print>"
        ));
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}
