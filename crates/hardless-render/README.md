# hardless-render

Receipt renderer for [Hardless](https://github.com/hardlessdev/hardless): turns the ESC/POS
command AST from `hardless-escpos` into what a thermal printer would print.

- **HTML (FR-10)** at 58 mm (32 columns Font A / 42 Font B) or 80 mm (48 / 64 columns).
  Alignment, bold, underline, character size, fonts and line wrapping follow the printer.
- **Cuts, QR codes, barcodes and raster images (FR-12)** are drawn as inline SVG. Unknown
  commands show as a small visible badge, which can be turned off.
- **Plain text (FR-13)**: `to_text` lays the receipt out in fixed columns for test
  assertions, e.g. `Total:\s+1,250`.
- **SVG and PNG (FR-11)** with the same layout as the HTML: the same line breaks, columns,
  alignment, sizes, codes and cuts, drawn in printer dots (576 across 80 mm, 384 across
  58 mm). PNG output needs the `png` feature.
- **Safe to embed**: all receipt text is HTML- or XML-escaped.
- **Sans-IO**: builds for `wasm32-unknown-unknown` (without the `png` feature).

## Usage

```rust
use hardless_escpos::parse;
use hardless_render::{render_html, to_text, PaperWidth, RenderOptions};

let ast = parse(b"\x1ba\x01HARDLESS\n\x1ba\x00Total:   1,250\n\x1dV\x00");

// Plain text, 48 columns (80 mm, Font A).
let text = to_text(&ast);
assert!(text.contains("Total:   1,250"));
assert!(text.starts_with("                    HARDLESS")); // centered

// HTML fragment; include STYLESHEET once on the page (or use render_html_document).
let html = render_html(&ast, &RenderOptions { paper: PaperWidth::Mm58, ..Default::default() });
assert!(html.contains("hl-cut"));

// Standalone SVG, one unit per printer dot: 576 dots plus margins for 80 mm.
let svg = hardless_render::render_svg(&ast, &RenderOptions::default());
assert!(svg.contains(r#"width="612""#));
```

### SVG and PNG

`render_svg` returns a standalone SVG document. Each run of text is stretched to its exact
column width (`textLength`), so columns line up with any monospace font; it asks for Go Mono
first, then common system monospace fonts.

`render_png` (feature `png`) rasterises that SVG with [`resvg`](https://crates.io/crates/resvg),
one pixel per printer dot, using only the bundled **Go Mono** font, so a receipt looks the
same on every OS and in a container with no fonts. Receipts taller than `MAX_PNG_HEIGHT`
(20,000 dots) are refused rather than allocating hundreds of megabytes.

```toml
hardless-render = { version = "0.0.0", features = ["png"] }
```

Try it on a fixture:

```sh
cargo run -p hardless-render --features png --example render -- fixtures/escpos/33-full-receipt.bin 80 png > receipt.png
```

### Bundled font

`fonts/` holds Go Mono and Go Mono Bold by Bigelow & Holmes, made for the Go project and
licensed under BSD-3-Clause (`fonts/LICENSE`, also exported as `FONT_LICENSE`). A binary built
with the `png` feature must ship that notice.

## Output reference

| AST node                               | HTML                                                       | SVG / PNG                                       | `to_text`                          |
| -------------------------------------- | ---------------------------------------------------------- | ----------------------------------------------- | ---------------------------------- |
| Text, line feed, feed                  | `.hl-line` per printed line                                | `<text>` runs, 30 dots per line (× height)      | one line each, aligned with spaces |
| Cut                                    | `.hl-cut` (`.hl-cut-partial` for partial)                  | dashed line (dotted for partial)                | nothing                            |
| QR print                               | inline `<svg class="hl-qr">`                               | nested `<svg class="hl-qr">`                    | `[QR: <data>]`                     |
| Barcode                                | inline `<svg class="hl-barcode">` plus HRI text if enabled | nested `<svg class="hl-barcode">` plus HRI text | `[BARCODE <symbology>: <data>]`    |
| Raster image                           | inline `<svg class="hl-image">`                            | nested `<svg class="hl-image">`                 | `[IMAGE <w>x<h>]`                  |
| Unknown                                | `<span class="hl-unknown">` badge (if `show_unknown`)      | yellow badge with the details as a tooltip      | nothing                            |
| Drawer pulse, status request, settings | nothing (not printed)                                      | nothing                                         | nothing                            |

## License

Apache-2.0
