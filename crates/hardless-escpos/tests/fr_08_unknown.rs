//! FR-08: unknown commands become `Unknown` nodes with the raw bytes and a warning; the
//! parser never panics.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use hardless_escpos::{parse, Node, Parser};
use proptest::prelude::*;

fn unknown(node: &Node) -> (&[u8], Option<&str>, &str) {
    match node {
        Node::Unknown {
            bytes,
            command,
            warning,
        } => (bytes, command.as_deref(), warning),
        other => panic!("expected unknown, got {other:?}"),
    }
}

#[test]
fn fr_08_unknown_command_yields_unknown_node() {
    let nodes = parse(b"\x1b\xf0after\n");
    let (bytes, command, warning) = unknown(&nodes[0]);
    assert_eq!(bytes, b"\x1b\xf0");
    assert_eq!(command, None);
    assert!(!warning.is_empty());
    assert_eq!(
        nodes[1],
        Node::Text {
            text: "after".into()
        }
    );
}

#[test]
fn fr_08_known_but_unsupported_commands_are_skipped_by_length() {
    // ESC 3 n (line spacing) takes one parameter: "24" must not print as text.
    let nodes = parse(b"\x1b3\x18spaced\n");
    let (bytes, command, _) = unknown(&nodes[0]);
    assert_eq!(bytes, b"\x1b3\x18");
    assert_eq!(command, Some("ESC 3"));
    assert_eq!(
        nodes[1],
        Node::Text {
            text: "spaced".into()
        }
    );

    // Variable-length commands too.
    let nodes = parse(b"\x1b*\x21\x02\x00\x01\x02\x03\x04\x05\x06after\n");
    assert_eq!(unknown(&nodes[0]).1, Some("ESC *"));
    assert_eq!(unknown(&nodes[0]).0.len(), 5 + 6);
    assert_eq!(
        nodes[1],
        Node::Text {
            text: "after".into()
        }
    );

    let nodes = parse(b"\x1bD\x08\x10\x00after\n");
    assert_eq!(unknown(&nodes[0]).1, Some("ESC D"));
    assert_eq!(
        nodes[1],
        Node::Text {
            text: "after".into()
        }
    );
}

#[test]
fn fr_08_unsupported_symbols_are_unknown_with_their_name() {
    let pdf417 = parse(b"\x1d(k\x05\x000Ppdf");
    assert_eq!(unknown(&pdf417[0]).1, Some("GS ( k PDF417"));
    let upca = parse(b"\x1dkA\x0b01234567890x");
    assert_eq!(unknown(&upca[0]).1, Some("GS k UPC-A"));
    assert_eq!(upca[1], Node::Text { text: "x".into() });
}

#[test]
fn fr_08_unknown_control_bytes() {
    let nodes = parse(b"a\x07b");
    assert_eq!(nodes[0], Node::Text { text: "a".into() });
    assert_eq!(unknown(&nodes[1]).0, b"\x07");
    assert_eq!(nodes[2], Node::Text { text: "b".into() });
}

#[test]
fn fr_08_truncated_command_at_finish_is_unknown() {
    let mut parser = Parser::new();
    assert!(parser.push(b"\x1d(k\x04\x001").is_empty());
    let nodes = parser.finish();
    let (bytes, _, warning) = unknown(&nodes[0]);
    assert_eq!(bytes, b"\x1d(k\x04\x001");
    assert!(warning.contains("incomplete"), "{warning}");
}

#[test]
fn fr_08_oversized_images_are_skipped_without_buffering_them() {
    // Declares 65535 x 65535 bytes (4 GB): the parser must not try to hold it.
    let mut parser = Parser::new();
    let mut nodes = parser.push(b"\x1dv0\x00\xff\xff\xff\xff");
    nodes.extend(parser.push(&[0u8; 10_000]));
    nodes.extend(parser.finish());
    assert_eq!(nodes.len(), 1);
    let (bytes, command, warning) = unknown(&nodes[0]);
    assert_eq!(command, Some("GS v 0"));
    assert!(
        bytes.len() <= 8,
        "only the header is kept, got {} bytes",
        bytes.len()
    );
    assert!(warning.contains("too large"), "{warning}");
}

#[test]
fn fr_08_barcode_without_terminator_is_cut_off() {
    // Function A data ends at NUL; a missing NUL must not swallow the rest forever.
    let mut data = b"\x1dk\x04".to_vec();
    data.extend(std::iter::repeat_n(b'A', 300));
    let nodes = parse(&data);
    assert!(matches!(&nodes[0], Node::Unknown { .. }));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn fr_08_parser_never_panics_on_any_input(data in proptest::collection::vec(any::<u8>(), 0..2_000)) {
        let _ = parse(&data);
    }

    #[test]
    fn fr_08_parser_never_panics_on_command_heavy_input(
        data in proptest::collection::vec(prop_oneof![
            Just(0x1bu8), Just(0x1d), Just(0x10), Just(0x1c), Just(b'('), Just(b'k'),
            Just(b'v'), Just(b'0'), Just(0x00), Just(0xff), any::<u8>()
        ], 0..500)
    ) {
        let _ = parse(&data);
    }
}
