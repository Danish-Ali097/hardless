//! Lays an AST out the way a printer would: lines of styled text, plus cuts, codes and
//! images. Shared by the HTML and text renderers so both show the same layout.

use hardless_escpos::{Align, Font, HriPosition, Node, QrErrorCorrection, Symbology};

use crate::PaperWidth;

/// Dots per Font A character: 576-dot (80 mm) and 384-dot (58 mm) heads print 48 and 32
/// columns.
pub(crate) const DOTS_PER_COLUMN: u32 = 12;

/// Text style of a run of characters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Style {
    pub bold: bool,
    pub underline: u8,
    pub width: u8,
    pub height: u8,
    pub font: Font,
}

impl Style {
    fn reset() -> Self {
        Self {
            width: 1,
            height: 1,
            ..Self::default()
        }
    }
}

/// Characters in one style.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Segment {
    Text { text: String, style: Style },
    Unknown { label: String, title: String },
}

/// One printed line.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Line {
    pub align: Align,
    pub segments: Vec<Segment>,
    /// Columns used, in Font A columns of the current font.
    pub columns: u32,
}

impl Line {
    pub fn height(&self) -> u8 {
        self.segments
            .iter()
            .filter_map(|s| match s {
                Segment::Text { style, .. } => Some(style.height),
                Segment::Unknown { .. } => None,
            })
            .max()
            .unwrap_or(1)
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }
}

/// Something printed on the receipt.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Block {
    Line(Line),
    Cut {
        partial: bool,
    },
    Qr {
        align: Align,
        data: Vec<u8>,
        text: String,
        module_dots: u8,
        level: QrErrorCorrection,
    },
    Barcode {
        align: Align,
        symbology: Symbology,
        data: String,
        height_dots: u8,
        module_dots: u8,
        hri: HriPosition,
    },
    Image {
        align: Align,
        width_bytes: u16,
        height: u16,
        double_width: bool,
        double_height: bool,
        data: Vec<u8>,
    },
}

/// Printer state while laying out.
struct Layout {
    columns_font_a: u32,
    columns_font_b: u32,
    show_unknown: bool,
    align: Align,
    style: Style,
    line: Line,
    blocks: Vec<Block>,
    qr_data: Option<(Vec<u8>, String)>,
    qr_module: u8,
    qr_level: QrErrorCorrection,
    barcode_height: u8,
    barcode_module: u8,
    hri: HriPosition,
}

/// Lays out `nodes` for `paper`.
pub(crate) fn layout(nodes: &[Node], paper: PaperWidth, show_unknown: bool) -> Vec<Block> {
    let (columns_font_a, columns_font_b) = paper.columns();
    let mut l = Layout {
        columns_font_a,
        columns_font_b,
        show_unknown,
        align: Align::Left,
        style: Style::reset(),
        line: Line::default(),
        blocks: Vec::new(),
        qr_data: None,
        qr_module: 3,
        qr_level: QrErrorCorrection::L,
        barcode_height: 162,
        barcode_module: 3,
        hri: HriPosition::None,
    };
    for node in nodes {
        l.node(node);
    }
    l.flush_if_used();
    l.blocks
}

impl Layout {
    /// Columns available on a line for a character in `font`.
    fn columns(&self, font: Font) -> u32 {
        match font {
            Font::A => self.columns_font_a,
            Font::B | Font::C => self.columns_font_b,
        }
    }

    fn node(&mut self, node: &Node) {
        match node {
            Node::Init => {
                // ESC @ clears the print buffer and resets formatting.
                self.line = Line::default();
                self.align = Align::Left;
                self.style = Style::reset();
                self.qr_data = None;
                self.qr_module = 3;
                self.qr_level = QrErrorCorrection::L;
                self.barcode_height = 162;
                self.barcode_module = 3;
                self.hri = HriPosition::None;
            }
            Node::Text { text } => {
                for ch in text.chars() {
                    self.push_char(ch);
                }
            }
            Node::LineFeed => self.flush(),
            Node::CarriageReturn | Node::CodePage { .. } | Node::Status { .. } => {}
            Node::Pulse { .. } | Node::QrModel { .. } => {}
            Node::Tab => {
                let font = self.style.font;
                let per_column = self.column_weight(font);
                let next = (self.line.columns / (8 * per_column) + 1) * 8 * per_column;
                while self.line.columns < next && self.line.columns < self.width_units() {
                    self.push_char(' ');
                }
            }
            Node::Feed { lines } => {
                self.flush_if_used();
                for _ in 0..*lines {
                    self.flush();
                }
            }
            Node::Align { align } => {
                self.align = *align;
                if self.line.is_empty() {
                    self.line.align = *align;
                }
            }
            Node::Bold { on } => self.style.bold = *on,
            Node::Underline { dots } => self.style.underline = *dots,
            Node::Size { width, height } => {
                self.style.width = *width;
                self.style.height = *height;
            }
            Node::PrintMode {
                font,
                bold,
                double_height,
                double_width,
                underline,
            } => {
                self.style.font = *font;
                self.style.bold = *bold;
                self.style.height = if *double_height { 2 } else { 1 };
                self.style.width = if *double_width { 2 } else { 1 };
                self.style.underline = u8::from(*underline);
            }
            Node::Font { font } => self.style.font = *font,
            Node::Cut { partial, .. } => {
                self.flush_if_used();
                self.blocks.push(Block::Cut { partial: *partial });
            }
            Node::QrModuleSize { dots } => self.qr_module = (*dots).clamp(1, 16),
            Node::QrErrorCorrection { level } => self.qr_level = *level,
            Node::QrStore { text, data } => self.qr_data = Some((data.clone(), text.clone())),
            Node::QrPrint => {
                self.flush_if_used();
                match self.qr_data.clone() {
                    Some((data, text)) => self.blocks.push(Block::Qr {
                        align: self.align,
                        data,
                        text,
                        module_dots: self.qr_module,
                        level: self.qr_level,
                    }),
                    None => self.unknown("QR", "QR print without stored data"),
                }
            }
            Node::BarcodeHeight { dots } => self.barcode_height = (*dots).max(1),
            Node::BarcodeWidth { dots } => self.barcode_module = (*dots).clamp(1, 6),
            Node::HriPosition { position } => self.hri = *position,
            Node::Barcode { symbology, data } => {
                self.flush_if_used();
                self.blocks.push(Block::Barcode {
                    align: self.align,
                    symbology: *symbology,
                    data: data.clone(),
                    height_dots: self.barcode_height,
                    module_dots: self.barcode_module,
                    hri: self.hri,
                });
            }
            Node::RasterImage {
                width_bytes,
                height,
                double_width,
                double_height,
                data,
            } => {
                self.flush_if_used();
                self.blocks.push(Block::Image {
                    align: self.align,
                    width_bytes: *width_bytes,
                    height: *height,
                    double_width: *double_width,
                    double_height: *double_height,
                    data: data.clone(),
                });
            }
            Node::Unknown {
                bytes,
                command,
                warning,
            } => {
                let label = command.clone().unwrap_or_else(|| "?".into());
                let hex: Vec<String> = bytes.iter().take(16).map(|b| format!("{b:02x}")).collect();
                let title = format!("{warning} ({})", hex.join(" "));
                self.unknown(&label, &title);
            }
        }
    }

    /// Width units per column for `font`: the line is measured in units so Font A and
    /// Font B characters can share it (Font B columns are narrower).
    fn column_weight(&self, font: Font) -> u32 {
        // Units: lcm-like scale so that both column counts divide the line width exactly.
        self.width_units() / self.columns(font)
    }

    /// The line width in units.
    fn width_units(&self) -> u32 {
        self.columns_font_a * self.columns_font_b
    }

    fn push_char(&mut self, ch: char) {
        let style = self.style;
        let units = self.column_weight(style.font) * u32::from(style.width.max(1));
        if self.line.columns + units > self.width_units() && !self.line.is_empty() {
            // The printer wraps when a character does not fit.
            self.flush();
        }
        if self.line.is_empty() {
            self.line.align = self.align;
        }
        self.line.columns += units;
        match self.line.segments.last_mut() {
            Some(Segment::Text { text, style: s }) if *s == style => text.push(ch),
            _ => self.line.segments.push(Segment::Text {
                text: ch.to_string(),
                style,
            }),
        }
    }

    fn unknown(&mut self, label: &str, title: &str) {
        if self.show_unknown {
            if self.line.is_empty() {
                self.line.align = self.align;
            }
            self.line.segments.push(Segment::Unknown {
                label: label.to_owned(),
                title: title.to_owned(),
            });
        }
    }

    /// Prints the current line (an empty line if nothing is buffered).
    fn flush(&mut self) {
        let mut line = std::mem::take(&mut self.line);
        if line.is_empty() {
            line.align = self.align;
        }
        self.blocks.push(Block::Line(line));
        self.line.align = self.align;
    }

    /// Prints the current line only if something is buffered.
    fn flush_if_used(&mut self) {
        if !self.line.is_empty() {
            self.flush();
        }
    }
}
