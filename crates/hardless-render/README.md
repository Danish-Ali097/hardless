# hardless-render

Receipt renderer for [Hardless](https://github.com/Danish-Ali097/hardless): turns the ESC/POS
command AST from `hardless-escpos` into what a thermal printer would print.

- **HTML (FR-10)** at 58 mm (32 columns Font A / 42 Font B) or 80 mm (48 / 64 columns).
  Alignment, bold, underline, character size, fonts and line wrapping follow the printer.
- **Cuts, QR codes, barcodes and raster images (FR-12)** are drawn as inline SVG. Unknown
  commands show as a small visible badge, which can be turned off.
- **Plain text (FR-13)**: `to_text` lays the receipt out in fixed columns for test
  assertions, e.g. `Total:\s+1,250`.
- **Safe to embed**: all receipt text is HTML-escaped.
- **Sans-IO**: builds for `wasm32-unknown-unknown`. SVG and PNG output (FR-11) arrive in M2.

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
```

## Output reference

| AST node                               | HTML                                                       | `to_text`                          |
| -------------------------------------- | ---------------------------------------------------------- | ---------------------------------- |
| Text, line feed, feed                  | `.hl-line` per printed line                                | one line each, aligned with spaces |
| Cut                                    | `.hl-cut` (`.hl-cut-partial` for partial)                  | nothing                            |
| QR print                               | inline `<svg class="hl-qr">`                               | `[QR: <data>]`                     |
| Barcode                                | inline `<svg class="hl-barcode">` plus HRI text if enabled | `[BARCODE <symbology>: <data>]`    |
| Raster image                           | inline `<svg class="hl-image">`                            | `[IMAGE <w>x<h>]`                  |
| Unknown                                | `<span class="hl-unknown">` badge (if `show_unknown`)      | nothing                            |
| Drawer pulse, status request, settings | nothing (not printed)                                      | nothing                            |

## License

Apache-2.0
