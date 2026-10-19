//! SVG output (FR-11): the same layout as the HTML output, drawn in printer dots.
//!
//! One SVG unit is one printer dot (576 across an 80 mm roll, 384 across 58 mm). Each run of
//! text is stretched to its exact column width with `textLength`, so columns line up whatever
//! monospace font the viewer has; the PNG output uses the bundled Go Mono font.

use std::fmt::Write as _;

use hardless_escpos::{Align, Font, HriPosition, Node};

use crate::codes;
use crate::html::{escape_into, symbology_name};
use crate::layout::{layout, Block, Line, Segment};
use crate::{PaperWidth, RenderOptions};

/// Line pitch of normal-height text in dots (the ESC/POS default line spacing).
const LINE_DOTS: u32 = 30;
/// Paper margin left and right of the printable area, in dots (1.5 Font A columns).
const MARGIN_X: u32 = 18;
/// Paper margin above and below the receipt, in dots.
const MARGIN_Y: u32 = 12;
/// Space above and below a code or image, in dots.
const BLOCK_PAD: u32 = 6;
/// Space below the baseline for descenders, in dots at normal height.
const DESCENT: u32 = 7;
/// Glyph advance of a monospace font as a fraction of its size (Go Mono: 0.6 em).
const ADVANCE_EM: f64 = 0.6;
/// Height of an HRI or error text line, in dots.
const SMALL_LINE_DOTS: u32 = 22;
/// Font for receipt text: the bundled Go Mono first (used for PNG), then common monospace fonts.
pub(crate) const FONT_FAMILY: &str = "'Go Mono', ui-monospace, Menlo, Consolas, monospace";
const INK: &str = "#111";

/// Renders an AST to a standalone SVG document with the same layout as [`crate::render_html`].
///
/// The image is 1 unit per printer dot: 612 × height for 80 mm paper (576 printable dots plus
/// margins) and 420 × height for 58 mm. All receipt text is XML-escaped.
///
/// ```
/// use hardless_escpos::parse;
/// use hardless_render::{render_svg, RenderOptions};
/// let svg = render_svg(&parse(b"Total: 1,250\n\x1dV\x00"), &RenderOptions::default());
/// assert!(svg.starts_with("<svg") && svg.contains("Total: 1,250"));
/// assert!(svg.contains(r#"width="612""#));
/// ```
pub fn render_svg(nodes: &[Node], options: &RenderOptions) -> String {
    let paper = options.paper;
    let width = paper.dots() + 2 * MARGIN_X;
    let mut body = String::new();
    let mut y = MARGIN_Y;
    for block in layout(nodes, paper, options.show_unknown) {
        match block {
            Block::Line(line) => y += line_svg(&mut body, &line, y, paper),
            Block::Cut { partial } => {
                let mid = y + LINE_DOTS / 2;
                let dashes = if partial { "2 4" } else { "8 6" };
                let label = if partial { "partial cut" } else { "cut" };
                let _ = write!(
                    body,
                    r##"<line x1="0" y1="{mid}" x2="{width}" y2="{mid}" stroke="#999" stroke-width="2" stroke-dasharray="{dashes}" aria-label="{label}"/>"##
                );
                y += LINE_DOTS;
            }
            Block::Qr {
                align,
                data,
                text,
                module_dots,
                level,
            } => {
                y += BLOCK_PAD;
                y += match codes::qr(&data, module_dots, level) {
                    Ok(svg) => place(&mut body, &svg, align, y, paper),
                    Err(e) => error(&mut body, &format!("{e}: {text}"), y),
                };
                y += BLOCK_PAD;
            }
            Block::Barcode {
                align,
                symbology,
                data,
                height_dots,
                module_dots,
                hri,
            } => {
                y += BLOCK_PAD;
                let label = codes::barcode_text(symbology, &data);
                match codes::barcode(symbology, &data, module_dots, height_dots) {
                    Ok(svg) => {
                        let center = f64::from(left_of(svg.width, align, paper))
                            + f64::from(svg.width) / 2.0;
                        if matches!(hri, HriPosition::Above | HriPosition::Both) {
                            y += hri_svg(&mut body, &label, center, y, paper);
                        }
                        y += place(&mut body, &svg, align, y, paper);
                        if matches!(hri, HriPosition::Below | HriPosition::Both) {
                            y += hri_svg(&mut body, &label, center, y, paper);
                        }
                    }
                    Err(e) => {
                        y += error(&mut body, &format!("{} {e}", symbology_name(symbology)), y);
                    }
                }
                y += BLOCK_PAD;
            }
            Block::Image {
                align,
                width_bytes,
                height,
                double_width,
                double_height,
                data,
            } => {
                y += BLOCK_PAD;
                let svg = codes::image(width_bytes, height, double_width, double_height, &data);
                y += place(&mut body, &svg, align, y, paper);
                y += BLOCK_PAD;
            }
        }
    }
    let height = y + MARGIN_Y;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" xml:space="preserve" font-family="{FONT_FAMILY}" role="img" aria-label="Receipt"><rect width="{width}" height="{height}" fill="#fff"/><g fill="{INK}">{body}</g></svg>"##
    )
}

/// Draws one printed line whose top is at `top`; returns its height in dots.
fn line_svg(out: &mut String, line: &Line, top: u32, paper: PaperWidth) -> u32 {
    let (cols_a, cols_b) = paper.columns();
    let line_units = cols_a * cols_b;
    let dots_per_unit = f64::from(paper.dots()) / f64::from(line_units);
    let pitch = LINE_DOTS * u32::from(line.height().max(1));
    let bottom = top + pitch;
    let free = f64::from(line_units.saturating_sub(line.columns)) * dots_per_unit;
    let mut x = f64::from(MARGIN_X)
        + match line.align {
            Align::Left => 0.0,
            Align::Center => free / 2.0,
            Align::Right => free,
        };
    for segment in &line.segments {
        match segment {
            Segment::Text { text, style } => {
                let columns = match style.font {
                    Font::A => cols_a,
                    Font::B | Font::C => cols_b,
                };
                let char_dots = f64::from(line_units / columns) * dots_per_unit;
                let (w, h) = (style.width.max(1), style.height.max(1));
                let n = text.chars().count() as f64;
                let run = char_dots * n;
                let baseline = bottom - DESCENT * u32::from(h);
                if style.underline > 0 {
                    let _ = write!(
                        out,
                        r#"<rect x="{x:.2}" y="{}" width="{:.2}" height="{}"/>"#,
                        baseline + 2,
                        run * f64::from(w),
                        style.underline
                    );
                }
                if text.chars().any(|c| c != ' ') {
                    let _ = write!(
                        out,
                        r#"<text transform="translate({x:.2} {bottom}) scale({w} {h})" y="-{DESCENT}" font-size="{:.2}" textLength="{run:.2}" lengthAdjust="spacingAndGlyphs""#,
                        char_dots / ADVANCE_EM
                    );
                    if style.bold {
                        out.push_str(r#" font-weight="bold""#);
                    }
                    out.push('>');
                    escape_into(out, text);
                    out.push_str("</text>");
                }
                x += run * f64::from(w);
            }
            Segment::Unknown { label, title } => {
                let badge = label.chars().count() as f64 * 7.0 + 8.0;
                out.push_str("<g><title>");
                escape_into(out, title);
                let _ = write!(
                    out,
                    r##"</title><rect x="{x:.2}" y="{}" width="{badge:.2}" height="18" rx="3" fill="#fde68a"/><text x="{:.2}" y="{}" font-size="11" fill="#78350f">"##,
                    bottom - 22,
                    x + 4.0,
                    bottom - 9
                );
                escape_into(out, label);
                out.push_str("</text></g>");
                x += badge + 4.0;
            }
        }
    }
    pitch
}

/// The left edge, in dots, of something `width` dots wide aligned on the paper.
fn left_of(width: u32, align: Align, paper: PaperWidth) -> u32 {
    let free = paper.dots().saturating_sub(width);
    MARGIN_X
        + match align {
            Align::Left => 0,
            Align::Center => free / 2,
            Align::Right => free,
        }
}

/// Places a code or image drawing at `y`; returns its height in dots.
fn place(out: &mut String, svg: &codes::Svg, align: Align, y: u32, paper: PaperWidth) -> u32 {
    let x = left_of(svg.width, align, paper);
    let position = format!(
        r#"<svg x="{x}" y="{y}" width="{}" height="{}" "#,
        svg.width, svg.height
    );
    out.push_str(&svg.body.replacen("<svg ", &position, 1));
    svg.height
}

/// Writes a barcode's human-readable text centred on `center`; returns its height in dots.
fn hri_svg(out: &mut String, label: &str, center: f64, y: u32, paper: PaperWidth) -> u32 {
    // Font B size, like the HTML output's smaller HRI text.
    let (_, cols_b) = paper.columns();
    let size = f64::from(paper.dots()) / f64::from(cols_b) / ADVANCE_EM;
    let _ = write!(
        out,
        r#"<text x="{center:.2}" y="{}" font-size="{size:.2}" text-anchor="middle">"#,
        y + 16
    );
    escape_into(out, label);
    out.push_str("</text>");
    SMALL_LINE_DOTS
}

/// Writes an error message in place of a code that cannot be drawn; returns its height.
fn error(out: &mut String, message: &str, y: u32) -> u32 {
    let _ = write!(
        out,
        r##"<text x="{MARGIN_X}" y="{}" font-size="14" fill="#b91c1c">"##,
        y + 16
    );
    escape_into(out, message);
    out.push_str("</text>");
    SMALL_LINE_DOTS
}
