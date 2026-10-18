# hardless-escpos

Streaming ESC/POS parser for [Hardless](https://github.com/Danish-Ali097/hardless): it turns
the bytes an app sends to a receipt printer into a list of commands (an AST) that the
renderer, the emulated printer and test assertions all share.

- **Streaming (FR-06).** Feed chunks of any size; a command split across chunks is parsed
  once it is complete. The result never depends on how the bytes were chunked.
- **Phase 1 command set (FR-07)**, plus a few very common extras listed below.
- **Never panics (FR-08).** Anything it does not understand becomes an `Unknown` node with
  the raw bytes and a warning. Commands it recognises but does not emulate are skipped by
  their documented length, so their parameters are not printed as text.
- **Code pages (FR-09).** CP437 (default) and CP858 built in; add others with
  `CodePages::with`.
- **Sans-IO.** No I/O, no clock, builds for `wasm32-unknown-unknown`.

Implemented from the publicly available ESC/POS command reference. No vendor SDK code.

## Usage

```rust
use hardless_escpos::{parse, Align, Node, Parser};

// One shot.
let nodes = parse(b"\x1b@\x1ba\x01Total: 1,250\n\x1dV\x00");
assert_eq!(nodes[0], Node::Init);
assert_eq!(nodes[1], Node::Align { align: Align::Center });
assert_eq!(nodes[2], Node::Text { text: "Total: 1,250".into() });
assert_eq!(nodes[3], Node::LineFeed);
assert_eq!(nodes[4], Node::Cut { partial: false, feed: None });

// Streaming: a command split across chunks is emitted once complete.
let mut parser = Parser::new();
assert!(parser.push(b"\x1b").is_empty());
assert_eq!(parser.push(b"E\x01"), [Node::Bold { on: true }]);
// Text is held until something ends it, so chunking never splits a text node.
assert!(parser.push(b"Hel").is_empty());
assert_eq!(parser.push(b"lo\n"), [Node::Text { text: "Hello".into() }, Node::LineFeed]);
assert!(parser.finish().is_empty());
```

## Commands

| Command | Bytes | Node |
| --- | --- | --- |
| Initialize | `ESC @` | `Init` |
| Print and line feed | `LF` | `LineFeed` |
| Print and feed n lines | `ESC d n` | `Feed { lines }` |
| Justification | `ESC a n` | `Align` |
| Emphasized | `ESC E n` | `Bold` |
| Underline | `ESC - n` | `Underline { dots }` |
| Character size | `GS ! n` | `Size { width, height }` |
| Character code table | `ESC t n` | `CodePage` |
| Cut | `GS V m` / `GS V m n` | `Cut { partial, feed }` |
| Drawer kick pulse | `ESC p m t1 t2` | `Pulse { pin, onMs, offMs }` |
| QR code | `GS ( k` (cn = 49) | `QrModel`, `QrModuleSize`, `QrErrorCorrection`, `QrStore`, `QrPrint` |
| Barcode | `GS k` (CODE128, EAN13, CODE39) | `Barcode` |
| Raster bit image | `GS v 0` | `RasterImage` |
| Real-time status | `DLE EOT n` | `Status { kind }` |

Extras beyond the phase 1 list, because apps send them all the time: `CR` (`CarriageReturn`),
`HT` (`Tab`), `ESC !` (`PrintMode`), `ESC M` (`Font`), `GS h` / `GS w` / `GS H` (barcode
height, module width, HRI position), `ESC i` / `ESC m` (old partial-cut commands) and
`DLE DC4 1` (real-time drawer pulse).

Other barcode symbologies, other `GS ( k` symbols (PDF417, ...) and other known commands are
skipped by length and reported as `Unknown` with the command name.

## License

Apache-2.0
