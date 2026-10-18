//! FR-07: the phase 1 command set, one test per command.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use hardless_escpos::{
    parse, Align, Font, HriPosition, Node, QrErrorCorrection, StatusKind, Symbology,
};

fn one(bytes: &[u8]) -> Node {
    let nodes = parse(bytes);
    assert_eq!(nodes.len(), 1, "{nodes:?}");
    nodes.into_iter().next().unwrap()
}

#[test]
fn fr_07_esc_at_initializes() {
    assert_eq!(one(b"\x1b@"), Node::Init);
}

#[test]
fn fr_07_lf_prints_and_feeds() {
    assert_eq!(
        parse(b"x\n"),
        [Node::Text { text: "x".into() }, Node::LineFeed]
    );
}

#[test]
fn fr_07_esc_d_feeds_n_lines() {
    assert_eq!(one(b"\x1bd\x05"), Node::Feed { lines: 5 });
}

#[test]
fn fr_07_esc_a_aligns_with_binary_and_ascii_parameters() {
    assert_eq!(one(b"\x1ba\x00"), Node::Align { align: Align::Left });
    assert_eq!(
        one(b"\x1ba1"),
        Node::Align {
            align: Align::Center
        }
    );
    assert_eq!(
        one(b"\x1ba\x02"),
        Node::Align {
            align: Align::Right
        }
    );
}

#[test]
fn fr_07_esc_e_uses_the_lowest_bit() {
    assert_eq!(one(b"\x1bE\x01"), Node::Bold { on: true });
    assert_eq!(one(b"\x1bE\x00"), Node::Bold { on: false });
    assert_eq!(one(b"\x1bE\xfe"), Node::Bold { on: false });
}

#[test]
fn fr_07_esc_minus_sets_underline_dots() {
    assert_eq!(one(b"\x1b-\x00"), Node::Underline { dots: 0 });
    assert_eq!(one(b"\x1b-1"), Node::Underline { dots: 1 });
    assert_eq!(one(b"\x1b-\x02"), Node::Underline { dots: 2 });
}

#[test]
fn fr_07_gs_bang_sets_width_and_height_multipliers() {
    assert_eq!(
        one(b"\x1d!\x00"),
        Node::Size {
            width: 1,
            height: 1
        }
    );
    assert_eq!(
        one(b"\x1d!\x10"),
        Node::Size {
            width: 2,
            height: 1
        }
    );
    assert_eq!(
        one(b"\x1d!\x01"),
        Node::Size {
            width: 1,
            height: 2
        }
    );
    assert_eq!(
        one(b"\x1d!\x77"),
        Node::Size {
            width: 8,
            height: 8
        }
    );
}

#[test]
fn fr_07_esc_t_selects_code_page() {
    assert_eq!(
        one(b"\x1bt\x13"),
        Node::CodePage {
            table: 19,
            name: Some("CP858".into())
        }
    );
}

#[test]
fn fr_07_gs_v_cuts_full_and_partial() {
    assert_eq!(
        one(b"\x1dV\x00"),
        Node::Cut {
            partial: false,
            feed: None
        }
    );
    assert_eq!(
        one(b"\x1dV1"),
        Node::Cut {
            partial: true,
            feed: None
        }
    );
    assert_eq!(
        one(b"\x1dVA\x03"),
        Node::Cut {
            partial: false,
            feed: Some(3)
        }
    );
    assert_eq!(
        one(b"\x1dVB\x00"),
        Node::Cut {
            partial: true,
            feed: Some(0)
        }
    );
}

#[test]
fn fr_07_esc_p_kicks_the_drawer_with_2ms_units() {
    assert_eq!(
        one(b"\x1bp\x00\x19\xfa"),
        Node::Pulse {
            pin: 2,
            on_ms: 50,
            off_ms: 500,
            realtime: false
        }
    );
    assert_eq!(
        one(b"\x1bp1\x32\x32"),
        Node::Pulse {
            pin: 5,
            on_ms: 100,
            off_ms: 100,
            realtime: false
        }
    );
}

#[test]
fn fr_07_gs_paren_k_qr_code_functions() {
    let qr = |cn_fn_params: &[u8]| {
        let mut b = vec![0x1d, b'(', b'k', cn_fn_params.len() as u8, 0];
        b.extend_from_slice(cn_fn_params);
        one(&b)
    };
    assert_eq!(qr(&[49, 65, 50, 0]), Node::QrModel { model: 2 });
    assert_eq!(qr(&[49, 67, 6]), Node::QrModuleSize { dots: 6 });
    assert_eq!(
        qr(&[49, 69, 51]),
        Node::QrErrorCorrection {
            level: QrErrorCorrection::H
        }
    );
    assert_eq!(
        qr(b"\x31\x50\x30hello"),
        Node::QrStore {
            text: "hello".into(),
            data: b"hello".to_vec()
        }
    );
    assert_eq!(qr(&[49, 81, 48]), Node::QrPrint);
}

#[test]
fn fr_07_gs_k_barcodes_code128_ean13_code39() {
    assert_eq!(
        one(b"\x1dkI\x05{B123"),
        Node::Barcode {
            symbology: Symbology::Code128,
            data: "{B123".into()
        }
    );
    assert_eq!(
        one(b"\x1dk\x02400638133393\x00"),
        Node::Barcode {
            symbology: Symbology::Ean13,
            data: "400638133393".into()
        }
    );
    assert_eq!(
        one(b"\x1dkC\x0d4006381333931"),
        Node::Barcode {
            symbology: Symbology::Ean13,
            data: "4006381333931".into()
        }
    );
    assert_eq!(
        one(b"\x1dk\x04ABC\x00"),
        Node::Barcode {
            symbology: Symbology::Code39,
            data: "ABC".into()
        }
    );
    assert_eq!(
        one(b"\x1dkE\x03ABC"),
        Node::Barcode {
            symbology: Symbology::Code39,
            data: "ABC".into()
        }
    );
}

#[test]
fn fr_07_barcode_settings() {
    assert_eq!(one(b"\x1dh\x50"), Node::BarcodeHeight { dots: 80 });
    assert_eq!(one(b"\x1dw\x03"), Node::BarcodeWidth { dots: 3 });
    assert_eq!(
        one(b"\x1dH2"),
        Node::HriPosition {
            position: HriPosition::Below
        }
    );
}

#[test]
fn fr_07_gs_v_0_raster_image() {
    let node = one(b"\x1dv0\x01\x01\x00\x02\x00\xff\x00");
    assert_eq!(
        node,
        Node::RasterImage {
            width_bytes: 1,
            height: 2,
            double_width: true,
            double_height: false,
            data: vec![0xff, 0x00],
        }
    );
}

#[test]
fn fr_07_dle_eot_status_requests() {
    let kinds: Vec<_> = parse(b"\x10\x04\x01\x10\x04\x02\x10\x04\x03\x10\x04\x04")
        .into_iter()
        .map(|n| match n {
            Node::Status { kind } => kind,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            StatusKind::Printer,
            StatusKind::OfflineCause,
            StatusKind::ErrorCause,
            StatusKind::PaperSensor
        ]
    );
}

#[test]
fn fr_07_common_extras() {
    assert_eq!(parse(b"\r\t"), [Node::CarriageReturn, Node::Tab]);
    assert_eq!(
        one(b"\x1b!\x39"),
        Node::PrintMode {
            font: Font::B,
            bold: true,
            double_height: true,
            double_width: true,
            underline: false
        }
    );
    assert_eq!(one(b"\x1bM\x01"), Node::Font { font: Font::B });
    assert_eq!(
        one(b"\x1bi"),
        Node::Cut {
            partial: true,
            feed: None
        }
    );
    assert_eq!(
        one(b"\x10\x14\x01\x00\x03"),
        Node::Pulse {
            pin: 2,
            on_ms: 300,
            off_ms: 300,
            realtime: true
        }
    );
}

#[test]
fn fr_07_json_wire_format() {
    let json = serde_json::to_value(parse(b"\x1bp\x00\x19\xfa\x1d(k\x04\x001P0A")).unwrap();
    assert_eq!(
        json,
        serde_json::json!([
            { "type": "pulse", "pin": 2, "onMs": 50, "offMs": 500 },
            { "type": "qr-store", "text": "A", "dataHex": "41" }
        ])
    );
    let back: Vec<Node> = serde_json::from_value(json).unwrap();
    assert_eq!(back.len(), 2);
}
