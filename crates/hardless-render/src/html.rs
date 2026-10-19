//! HTML output (FR-10, FR-12).

use std::fmt::Write as _;

use hardless_escpos::{Align, Font, HriPosition, Node, Symbology};

use crate::codes;
use crate::layout::{layout, Block, Line, Segment, DOTS_PER_COLUMN};
use crate::RenderOptions;

/// Styles for [`render_html`] output. Everything is scoped under `.hl-receipt`; one Font A
/// column is `1ch` of the receipt font, so text, codes and images keep the printer's
/// proportions at any font size. The receipt is white paper in both light and dark themes.
pub const STYLESHEET: &str = r#".hl-receipt{--hl-ink:#111;--hl-paper:#fff;box-sizing:content-box;width:calc(var(--hl-cols)*1ch);padding:1ch 1.5ch;background:var(--hl-paper);color:var(--hl-ink);font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,"Liberation Mono",monospace;font-size:14px;line-height:1.25;overflow:hidden;box-shadow:0 1px 3px rgba(0,0,0,.25)}
.hl-receipt *{box-sizing:border-box}
.hl-line{white-space:pre;min-height:calc(var(--hl-lh,1)*1.25em);display:flex;align-items:flex-end}
.hl-align-left{justify-content:flex-start}.hl-align-center{justify-content:center}.hl-align-right{justify-content:flex-end}
.hl-b{font-weight:700}.hl-u1{text-decoration:underline;text-decoration-thickness:1px}.hl-u2{text-decoration:underline;text-decoration-thickness:2px}
.hl-fb{font-size:.75em}
.hl-s{display:inline-block;transform:scale(var(--w),var(--h));transform-origin:0 100%;margin-right:calc((var(--w) - 1)*var(--n)*1ch)}
.hl-block{display:flex;padding:.25em 0}
.hl-block svg{display:block;fill:var(--hl-ink)}
.hl-code{display:flex;flex-direction:column;align-items:center}
.hl-hri{font-size:.75em;white-space:pre}
.hl-cut{margin:.6em -1.5ch;border-top:2px dashed #999;height:0}
.hl-cut-partial{border-top-style:dotted}
.hl-unknown{display:inline-block;margin:0 .15em;padding:0 .3em;border-radius:3px;background:#fde68a;color:#78350f;font:600 10px/1.4 ui-sans-serif,system-ui,sans-serif;white-space:nowrap;cursor:help}
.hl-error{font:11px/1.3 ui-sans-serif,system-ui,sans-serif;color:#b91c1c}
"#;

/// Renders an AST to an HTML fragment (`<div class="hl-receipt">`). Include
/// [`STYLESHEET`] once on the page, or use [`render_html_document`].
///
/// All receipt text is escaped, so the output is safe to insert into a page.
///
/// ```
/// use hardless_escpos::parse;
/// use hardless_render::{render_html, RenderOptions};
/// let html = render_html(&parse(b"<b>Hi</b>\n"), &RenderOptions::default());
/// assert!(html.contains("&lt;b&gt;Hi&lt;/b&gt;"));
/// ```
pub fn render_html(nodes: &[Node], options: &RenderOptions) -> String {
    let (cols, _) = options.paper.columns();
    let mut out = format!(
        r#"<div class="hl-receipt hl-paper-{}" style="--hl-cols:{cols}">"#,
        options.paper.millimetres()
    );
    for block in layout(nodes, options.paper, options.show_unknown) {
        match block {
            Block::Line(line) => line_html(&mut out, &line),
            Block::Cut { partial } => {
                let class = if partial {
                    "hl-cut hl-cut-partial"
                } else {
                    "hl-cut"
                };
                let label = if partial { "partial cut" } else { "cut" };
                let _ = write!(
                    out,
                    r#"<div class="{class}" role="separator" aria-label="{label}"></div>"#
                );
            }
            Block::Qr {
                align,
                data,
                text,
                module_dots,
                level,
            } => {
                open_block(&mut out, align);
                match codes::qr(&data, module_dots, level) {
                    Ok(svg) => sized(&mut out, &svg),
                    Err(e) => error(&mut out, &format!("{e}: {text}")),
                }
                out.push_str("</div>");
            }
            Block::Barcode {
                align,
                symbology,
                data,
                height_dots,
                module_dots,
                hri,
            } => {
                open_block(&mut out, align);
                out.push_str(r#"<div class="hl-code">"#);
                let label = codes::barcode_text(symbology, &data);
                if matches!(hri, HriPosition::Above | HriPosition::Both) {
                    hri_html(&mut out, &label);
                }
                match codes::barcode(symbology, &data, module_dots, height_dots) {
                    Ok(svg) => sized(&mut out, &svg),
                    Err(e) => error(&mut out, &format!("{} {e}", symbology_name(symbology))),
                }
                if matches!(hri, HriPosition::Below | HriPosition::Both) {
                    hri_html(&mut out, &label);
                }
                out.push_str("</div></div>");
            }
            Block::Image {
                align,
                width_bytes,
                height,
                double_width,
                double_height,
                data,
            } => {
                open_block(&mut out, align);
                sized(
                    &mut out,
                    &codes::image(width_bytes, height, double_width, double_height, &data),
                );
                out.push_str("</div>");
            }
        }
    }
    out.push_str("</div>");
    out
}

/// Renders an AST to a complete HTML document with [`STYLESHEET`], e.g. for
/// `GET /devices/:id/jobs/:jobId/render?format=html`.
pub fn render_html_document(nodes: &[Node], options: &RenderOptions) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Receipt</title><style>body{{margin:0;padding:16px;background:#e5e7eb}}{STYLESHEET}</style></head><body>{}</body></html>",
        render_html(nodes, options)
    )
}

fn line_html(out: &mut String, line: &Line) {
    let height = line.height();
    let _ = write!(out, r#"<div class="hl-line {}""#, align_class(line.align));
    if height > 1 {
        let _ = write!(out, r#" style="--hl-lh:{height}""#);
    }
    out.push('>');
    for segment in &line.segments {
        match segment {
            Segment::Text { text, style } => {
                let mut classes = Vec::new();
                if style.bold {
                    classes.push("hl-b");
                }
                match style.underline {
                    1 => classes.push("hl-u1"),
                    2 => classes.push("hl-u2"),
                    _ => {}
                }
                if style.font != Font::A {
                    classes.push("hl-fb");
                }
                let scaled = style.width > 1 || style.height > 1;
                if scaled {
                    classes.push("hl-s");
                }
                let _ = write!(out, r#"<span class="{}""#, classes.join(" "));
                if scaled {
                    let _ = write!(
                        out,
                        r#" style="--w:{};--h:{};--n:{}""#,
                        style.width,
                        style.height,
                        text.chars().count()
                    );
                }
                out.push('>');
                escape_into(out, text);
                out.push_str("</span>");
            }
            Segment::Unknown { label, title } => {
                out.push_str(r#"<span class="hl-unknown" title=""#);
                escape_into(out, title);
                out.push_str(r#"">"#);
                escape_into(out, label);
                out.push_str("</span>");
            }
        }
    }
    out.push_str("</div>");
}

fn open_block(out: &mut String, align: Align) {
    let _ = write!(out, r#"<div class="hl-block {}">"#, align_class(align));
}

/// Writes an SVG sized in Font A columns (12 dots per column).
fn sized(out: &mut String, svg: &codes::Svg) {
    let width = f64::from(svg.width) / f64::from(DOTS_PER_COLUMN);
    let height = f64::from(svg.height) / f64::from(DOTS_PER_COLUMN);
    let style = format!(r#" style="width:{width:.3}ch;height:{height:.3}ch""#);
    out.push_str(&svg.body.replacen("<svg ", &format!("<svg{style} "), 1));
}

fn hri_html(out: &mut String, label: &str) {
    out.push_str(r#"<div class="hl-hri">"#);
    escape_into(out, label);
    out.push_str("</div>");
}

fn error(out: &mut String, message: &str) {
    out.push_str(r#"<div class="hl-error">"#);
    escape_into(out, message);
    out.push_str("</div>");
}

fn align_class(align: Align) -> &'static str {
    match align {
        Align::Left => "hl-align-left",
        Align::Center => "hl-align-center",
        Align::Right => "hl-align-right",
    }
}

pub(crate) fn symbology_name(symbology: Symbology) -> &'static str {
    match symbology {
        Symbology::Code128 => "CODE128",
        Symbology::Ean13 => "EAN13",
        Symbology::Code39 => "CODE39",
    }
}

/// HTML-escapes text, and replaces control characters (which have no glyph) with U+FFFD.
pub(crate) fn escape_into(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if c.is_control() => out.push('\u{fffd}'),
            c => out.push(c),
        }
    }
}
