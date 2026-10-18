//! FR-08: the parser must never panic on any input, however it is chunked.
#![no_main]

use hardless_escpos::{parse, Parser};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let whole = parse(data);
    // Use the first byte to pick a chunk size, so chunking is fuzzed too.
    let chunk = usize::from(data.first().copied().unwrap_or(1)).max(1);
    let mut parser = Parser::new();
    let mut nodes = Vec::new();
    for piece in data.chunks(chunk) {
        nodes.extend(parser.push(piece));
    }
    nodes.extend(parser.finish());
    assert_eq!(nodes, whole);
});
