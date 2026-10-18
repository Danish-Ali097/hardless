//! FR-09: CP437 and CP858, and a pluggable table for others.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use hardless_escpos::{parse, CodePages, Node, Parser, TableCodePage};

fn text(nodes: &[Node]) -> String {
    nodes
        .iter()
        .filter_map(|n| match n {
            Node::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn fr_09_cp437_is_the_default() {
    assert_eq!(text(&parse(b"Caf\x82 \x9c5 \xc9\xcd\xbb")), "Café £5 ╔═╗");
}

#[test]
fn fr_09_cp858_has_the_euro_sign() {
    assert_eq!(text(&parse(b"\x1bt\x13\xd5 12,50")), "€ 12,50");
    // In CP437 the same byte is a box-drawing character.
    assert_eq!(text(&parse(b"\xd5")), "╒");
}

#[test]
fn fr_09_init_resets_the_code_page() {
    assert_eq!(text(&parse(b"\x1bt\x13\xd5\x1b@\xd5")), "€╒");
}

#[test]
fn fr_09_unknown_table_falls_back_to_cp437_and_says_so() {
    let nodes = parse(b"\x1bt\x11\x80");
    assert_eq!(
        nodes[0],
        Node::CodePage {
            table: 17,
            name: None
        }
    );
    assert_eq!(text(&nodes), "Ç");
}

#[test]
fn fr_09_other_tables_can_be_plugged_in() {
    let mut upper = ['?'; 128];
    upper[0] = 'А'; // Cyrillic A at 0x80, as in CP866
    let pages = CodePages::default().with(17, TableCodePage::new("CP866", upper));
    let mut parser = Parser::with_code_pages(pages);
    let mut nodes = parser.push(b"\x1bt\x11\x80");
    nodes.extend(parser.finish());
    assert_eq!(
        nodes[0],
        Node::CodePage {
            table: 17,
            name: Some("CP866".into())
        }
    );
    assert_eq!(text(&nodes), "А");
}
