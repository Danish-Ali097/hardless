//! Golden fixtures: every `fixtures/escpos/*.bin` parses to its snapshot AST, and the AST
//! is the same however the bytes are chunked (FR-06, FR-07, FR-08, FR-09).
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use std::fs;
use std::path::{Path, PathBuf};

use hardless_escpos::{parse, Node, Parser};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/escpos")
}

fn fixtures() -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = fs::read_dir(fixtures_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "bin"))
        .map(|p| {
            let name = p.file_stem().unwrap().to_string_lossy().into_owned();
            (name, fs::read(&p).unwrap())
        })
        .collect();
    out.sort();
    out
}

fn parse_chunked(data: &[u8], chunk: usize) -> Vec<Node> {
    let mut parser = Parser::new();
    let mut nodes = Vec::new();
    for piece in data.chunks(chunk.max(1)) {
        nodes.extend(parser.push(piece));
    }
    nodes.extend(parser.finish());
    nodes
}

#[test]
fn fr_07_golden_fixtures_match_their_snapshots() {
    let all = fixtures();
    assert!(
        all.len() >= 30,
        "PRD M1 asks for at least 30 golden cases, found {}",
        all.len()
    );
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(fixtures_dir().join("snapshots"));
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        for (name, data) in &all {
            insta::assert_json_snapshot!(format!("{name}.ast"), parse(data));
        }
    });
}

#[test]
fn fr_06_every_fixture_parses_the_same_however_it_is_chunked() {
    for (name, data) in fixtures() {
        let whole = parse(&data);
        for chunk in [1, 2, 3, 5, 7, 64] {
            assert_eq!(
                parse_chunked(&data, chunk),
                whole,
                "{name}, chunks of {chunk}"
            );
        }
        // Every single split point, so each command is split in every possible place.
        for split in 0..=data.len() {
            let mut parser = Parser::new();
            let mut nodes = parser.push(&data[..split]);
            nodes.extend(parser.push(&data[split..]));
            nodes.extend(parser.finish());
            assert_eq!(nodes, whole, "{name}, split at {split}");
        }
    }
}
