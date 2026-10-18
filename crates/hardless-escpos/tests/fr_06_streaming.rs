//! FR-06: streaming parser that accepts chunks of any size.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use hardless_escpos::{parse, Node, Parser, StatusKind};
use proptest::prelude::*;

#[test]
fn fr_06_split_command_is_emitted_once_complete() {
    let mut parser = Parser::new();
    assert!(parser.push(b"\x1d").is_empty());
    assert!(parser.push(b"!").is_empty());
    assert_eq!(
        parser.push(b"\x11"),
        [Node::Size {
            width: 2,
            height: 2
        }]
    );
}

#[test]
fn fr_06_status_request_is_answered_without_waiting_for_more_bytes() {
    // A device must be able to reply to DLE EOT straight away.
    let mut parser = Parser::new();
    assert_eq!(
        parser.push(b"\x10\x04\x01"),
        [Node::Status {
            kind: StatusKind::Printer
        }]
    );
}

#[test]
fn fr_06_text_is_held_until_it_ends_so_chunks_never_split_it() {
    let mut parser = Parser::new();
    assert!(parser.push(b"Tot").is_empty());
    assert!(parser.push(b"al").is_empty());
    assert_eq!(
        parser.push(b"\n"),
        [
            Node::Text {
                text: "Total".into()
            },
            Node::LineFeed
        ]
    );
    assert!(parser.push(b"tail").is_empty());
    assert_eq!(
        parser.finish(),
        [Node::Text {
            text: "tail".into()
        }]
    );
}

#[test]
fn fr_06_large_raster_image_split_across_many_chunks() {
    let mut data = vec![0x1d, b'v', b'0', 0, 48, 0, 100, 0];
    data.extend(std::iter::repeat_n(0xAA, 48 * 100));
    let mut parser = Parser::new();
    let mut nodes = Vec::new();
    for chunk in data.chunks(100) {
        nodes.extend(parser.push(chunk));
    }
    nodes.extend(parser.finish());
    assert_eq!(nodes.len(), 1);
    assert!(
        matches!(&nodes[0], Node::RasterImage { width_bytes: 48, height: 100, data, .. } if data.len() == 4800)
    );
}

#[test]
fn fr_06_parser_can_be_reused_after_finish() {
    let mut parser = Parser::new();
    parser.push(b"a");
    parser.finish();
    assert_eq!(
        parser.push(b"b\n"),
        [Node::Text { text: "b".into() }, Node::LineFeed]
    );
}

proptest! {
    #[test]
    fn fr_06_chunking_never_changes_the_result(
        data in proptest::collection::vec(any::<u8>(), 0..400),
        cuts in proptest::collection::vec(0usize..400, 0..12),
    ) {
        let whole = parse(&data);
        let mut cuts: Vec<usize> = cuts.into_iter().map(|c| c.min(data.len())).collect();
        cuts.sort_unstable();
        let mut parser = Parser::new();
        let mut nodes = Vec::new();
        let mut start = 0;
        for cut in cuts {
            nodes.extend(parser.push(&data[start..cut]));
            start = cut;
        }
        nodes.extend(parser.push(&data[start..]));
        nodes.extend(parser.finish());
        prop_assert_eq!(nodes, whole);
    }
}
