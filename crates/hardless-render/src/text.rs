//! Plain-text output for test assertions (FR-13).

use hardless_escpos::{Align, Node};

use crate::codes;
use crate::html::symbology_name;
use crate::layout::{layout, Block, Segment};
use crate::RenderOptions;

/// Lays out `nodes` as plain text in fixed columns, as an 80 mm printer with Font A
/// (48 columns) would print them. See [`to_text_with`] for other widths.
///
/// Each printed line becomes a line of text, aligned with spaces and with trailing spaces
/// removed; wide characters (`GS !`) count as several columns for alignment but are written
/// once. QR codes, barcodes and images appear as `[QR: …]`, `[BARCODE …: …]` and
/// `[IMAGE WxH]`. Cuts, drawer pulses, settings and unknown commands produce no text.
///
/// ```
/// use hardless_escpos::parse;
/// use hardless_render::to_text;
/// let text = to_text(&parse(b"\x1ba\x02Total: 1,250\n"));
/// assert_eq!(text, format!("{}Total: 1,250\n", " ".repeat(48 - 12)));
/// ```
pub fn to_text(nodes: &[Node]) -> String {
    to_text_with(nodes, &RenderOptions::default())
}

/// [`to_text`] for a given paper width.
pub fn to_text_with(nodes: &[Node], options: &RenderOptions) -> String {
    let (columns_a, columns_b) = options.paper.columns();
    let mut out = String::new();
    for block in layout(nodes, options.paper, false) {
        match block {
            Block::Line(line) => {
                let text: String = line
                    .segments
                    .iter()
                    .filter_map(|s| match s {
                        Segment::Text { text, .. } => Some(text.as_str()),
                        Segment::Unknown { .. } => None,
                    })
                    .collect();
                // Line width is measured in units of columns_a * columns_b; one Font A
                // column is columns_b units.
                let used = line.columns.div_ceil(columns_b.max(1));
                push_aligned(&mut out, line.align, &text, used, columns_a);
            }
            Block::Cut { .. } => {}
            Block::Qr { align, text, .. } => {
                let label = format!("[QR: {}]", printable(&text));
                push_label(&mut out, align, &label, columns_a);
            }
            Block::Barcode {
                align,
                symbology,
                data,
                ..
            } => {
                let label = format!(
                    "[BARCODE {}: {}]",
                    symbology_name(symbology),
                    printable(&codes::barcode_text(symbology, &data))
                );
                push_label(&mut out, align, &label, columns_a);
            }
            Block::Image {
                align,
                width_bytes,
                height,
                double_width,
                double_height,
                ..
            } => {
                let w = u32::from(width_bytes) * 8 * if double_width { 2 } else { 1 };
                let h = u32::from(height) * if double_height { 2 } else { 1 };
                push_label(&mut out, align, &format!("[IMAGE {w}x{h}]"), columns_a);
            }
        }
    }
    out
}

fn push_label(out: &mut String, align: Align, label: &str, columns: u32) {
    let used = u32::try_from(label.chars().count()).unwrap_or(u32::MAX);
    push_aligned(out, align, label, used, columns);
}

fn push_aligned(out: &mut String, align: Align, text: &str, used: u32, columns: u32) {
    let free = columns.saturating_sub(used);
    let pad = match align {
        Align::Left => 0,
        Align::Center => free / 2,
        Align::Right => free,
    };
    let line = format!("{}{}", " ".repeat(pad as usize), text);
    out.push_str(line.trim_end());
    out.push('\n');
}

/// Replaces control characters, which have no printed form, with U+FFFD.
fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '\u{fffd}' } else { c })
        .collect()
}
