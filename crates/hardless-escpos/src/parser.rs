//! The streaming parser (FR-06, FR-07, FR-08).
//!
//! Command formats follow the publicly available ESC/POS command reference. Every command
//! starts with a control byte (`ESC`, `GS`, `FS`, `DLE` or a single control character);
//! everything else is printable text in the active code page.

use crate::ast::{Align, Font, HriPosition, Node, QrErrorCorrection, StatusKind, Symbology};
use crate::codepage::{CodePages, TABLE_PC437};

const ESC: u8 = 0x1b;
const GS: u8 = 0x1d;
const FS: u8 = 0x1c;
const DLE: u8 = 0x10;

/// Largest raster image kept in the AST (`GS v 0`); bigger ones are skipped.
pub const MAX_IMAGE_BYTES: usize = 4 * 1024 * 1024;
/// Largest unsupported command whose bytes are kept in an `Unknown` node; bigger ones are
/// skipped and only their header is kept.
pub const MAX_UNKNOWN_BYTES: usize = 64 * 1024;
/// Longest NUL-terminated barcode data (`GS k`, function A) before it is cut off.
pub const MAX_BARCODE_A_BYTES: usize = 255;

/// The result of trying to parse one command at the start of the buffer.
enum Step {
    /// A command of `len` bytes.
    Done(Node, usize),
    /// The command is not complete yet.
    NeedMore,
    /// An oversized command: emit `node` for its `len`-byte header, then skip `skip` bytes.
    Skip(Node, usize, u64),
}

/// A streaming ESC/POS parser.
///
/// Feed it bytes with [`Parser::push`] as they arrive; it returns every command completed by
/// those bytes. Call [`Parser::finish`] at the end of the data (for example when a print job
/// closes) to flush pending text and report an incomplete trailing command. The code page
/// selected with `ESC t` persists across `finish` until `ESC @`, like on a printer.
///
/// ```
/// use hardless_escpos::{Node, Parser};
/// let mut parser = Parser::new();
/// let mut nodes = parser.push(b"\x1bE");
/// nodes.extend(parser.push(b"\x01Hi"));
/// nodes.extend(parser.finish());
/// assert_eq!(nodes, [Node::Bold { on: true }, Node::Text { text: "Hi".into() }]);
/// ```
#[derive(Debug)]
pub struct Parser {
    pages: CodePages,
    table: u8,
    buf: Vec<u8>,
    text: String,
    skip: u64,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

/// Parses a complete byte sequence.
///
/// ```
/// use hardless_escpos::{parse, Node};
/// assert_eq!(parse(b"Hi\n"), [Node::Text { text: "Hi".into() }, Node::LineFeed]);
/// ```
pub fn parse(data: &[u8]) -> Vec<Node> {
    let mut parser = Parser::new();
    let mut nodes = parser.push(data);
    nodes.extend(parser.finish());
    nodes
}

impl Parser {
    /// A parser with the built-in code pages (CP437 and CP858).
    pub fn new() -> Self {
        Self::with_code_pages(CodePages::default())
    }

    /// A parser with custom code pages (FR-09).
    pub fn with_code_pages(pages: CodePages) -> Self {
        Self {
            pages,
            table: TABLE_PC437,
            buf: Vec::new(),
            text: String::new(),
            skip: 0,
        }
    }

    /// The `ESC t` table currently used to decode text.
    pub fn code_page(&self) -> u8 {
        self.table
    }

    /// Whether bytes are buffered: pending text, an incomplete command or data being skipped.
    ///
    /// ```
    /// use hardless_escpos::Parser;
    /// let mut parser = Parser::new();
    /// parser.push(b"\x1d");
    /// assert!(parser.has_pending());
    /// ```
    pub fn has_pending(&self) -> bool {
        !self.buf.is_empty() || !self.text.is_empty() || self.skip > 0
    }

    /// Feeds bytes and returns every command they complete. Printable text is returned once
    /// something ends it (a command or [`Parser::finish`]), so the result never depends on
    /// how the data was chunked.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Node> {
        let mut out = Vec::new();
        let mut chunk = chunk;
        if self.skip > 0 {
            let n = usize::try_from(self.skip)
                .unwrap_or(usize::MAX)
                .min(chunk.len());
            self.skip -= n as u64;
            chunk = &chunk[n..];
        }
        self.buf.extend_from_slice(chunk);
        let buf = std::mem::take(&mut self.buf);
        let mut pos = 0;
        while pos < buf.len() {
            if self.skip > 0 {
                let n = usize::try_from(self.skip)
                    .unwrap_or(usize::MAX)
                    .min(buf.len() - pos);
                self.skip -= n as u64;
                pos += n;
                continue;
            }
            let byte = buf[pos];
            if byte >= 0x20 {
                self.text.push(self.pages.decode(self.table, byte));
                pos += 1;
                continue;
            }
            self.flush_text(&mut out);
            match self.command(&buf[pos..]) {
                Step::Done(node, len) => {
                    self.apply(&node);
                    out.push(node);
                    pos += len;
                }
                Step::Skip(node, len, skip) => {
                    out.push(node);
                    pos += len;
                    self.skip = skip;
                }
                Step::NeedMore => break,
            }
        }
        self.buf = buf;
        self.buf.drain(..pos);
        out
    }

    /// Ends the data: returns pending text, and an `Unknown` node for an incomplete command.
    pub fn finish(&mut self) -> Vec<Node> {
        let mut out = Vec::new();
        self.flush_text(&mut out);
        if !self.buf.is_empty() {
            out.push(Node::Unknown {
                bytes: std::mem::take(&mut self.buf),
                command: None,
                warning: "incomplete command at end of data".into(),
            });
        }
        self.skip = 0;
        out
    }

    fn flush_text(&mut self, out: &mut Vec<Node>) {
        if !self.text.is_empty() {
            out.push(Node::Text {
                text: std::mem::take(&mut self.text),
            });
        }
    }

    /// Parser state changes caused by a command.
    fn apply(&mut self, node: &Node) {
        match node {
            Node::Init => self.table = TABLE_PC437,
            Node::CodePage { table, .. } => self.table = *table,
            _ => {}
        }
    }

    /// Parses the command at the start of `data` (which starts with a control byte).
    fn command(&self, data: &[u8]) -> Step {
        match data[0] {
            0x0a => Step::Done(Node::LineFeed, 1),
            0x0d => Step::Done(Node::CarriageReturn, 1),
            0x09 => Step::Done(Node::Tab, 1),
            ESC => self.esc(data),
            GS => self.gs(data),
            FS => fs(data),
            DLE => dle(data),
            byte => Step::Done(
                unknown(
                    &data[..1],
                    control_name(byte),
                    format!("control byte 0x{byte:02x} is not emulated"),
                ),
                1,
            ),
        }
    }

    fn esc(&self, data: &[u8]) -> Step {
        let Some(&cmd) = data.get(1) else {
            return Step::NeedMore;
        };
        let name = || format!("ESC {}", printable(cmd));
        match cmd {
            b'@' => Step::Done(Node::Init, 2),
            b'a' => param(
                data,
                3,
                |n| match n {
                    0 | 48 => Some(Node::Align { align: Align::Left }),
                    1 | 49 => Some(Node::Align {
                        align: Align::Center,
                    }),
                    2 | 50 => Some(Node::Align {
                        align: Align::Right,
                    }),
                    _ => None,
                },
                name,
            ),
            b'E' => param(data, 3, |n| Some(Node::Bold { on: n & 1 == 1 }), name),
            b'-' => param(
                data,
                3,
                |n| match n {
                    0 | 48 => Some(Node::Underline { dots: 0 }),
                    1 | 49 => Some(Node::Underline { dots: 1 }),
                    2 | 50 => Some(Node::Underline { dots: 2 }),
                    _ => None,
                },
                name,
            ),
            b'd' => param(data, 3, |n| Some(Node::Feed { lines: n }), name),
            b't' => param(
                data,
                3,
                |n| {
                    Some(Node::CodePage {
                        table: n,
                        name: self.pages.get(n).map(|p| p.name().to_owned()),
                    })
                },
                name,
            ),
            b'!' => param(
                data,
                3,
                |n| {
                    Some(Node::PrintMode {
                        font: if n & 0x01 == 0 { Font::A } else { Font::B },
                        bold: n & 0x08 != 0,
                        double_height: n & 0x10 != 0,
                        double_width: n & 0x20 != 0,
                        underline: n & 0x80 != 0,
                    })
                },
                name,
            ),
            b'M' => param(
                data,
                3,
                |n| match n {
                    0 | 48 => Some(Node::Font { font: Font::A }),
                    1 | 49 => Some(Node::Font { font: Font::B }),
                    2 | 50 => Some(Node::Font { font: Font::C }),
                    _ => None,
                },
                name,
            ),
            b'p' => {
                if data.len() < 5 {
                    return Step::NeedMore;
                }
                let pin = match data[2] {
                    0 | 48 => 2,
                    1 | 49 => 5,
                    _ => return invalid(&data[..5], name()),
                };
                Step::Done(
                    Node::Pulse {
                        pin,
                        on_ms: u64::from(data[3]) * 2,
                        off_ms: u64::from(data[4]) * 2,
                        realtime: false,
                    },
                    5,
                )
            }
            b'i' | b'm' => Step::Done(
                Node::Cut {
                    partial: true,
                    feed: None,
                },
                2,
            ),
            b'*' => {
                // ESC * m nL nH d1..dk: bit image, k = (nL + nH * 256) columns.
                if data.len() < 5 {
                    return Step::NeedMore;
                }
                let columns = usize::from(data[3]) + usize::from(data[4]) * 256;
                let per_column = match data[2] {
                    0 | 1 => 1,
                    32 | 33 => 3,
                    _ => return invalid(&data[..5], name()),
                };
                sized_unknown(data, 5, 5 + columns * per_column, name())
            }
            b'D' => terminated_unknown(data, 2, 33, name()),
            _ => match esc_fixed_len(cmd) {
                Some(len) => fixed_unknown(data, len, name()),
                None => Step::Done(
                    unknown(&data[..2], None, format!("unknown command {}", name())),
                    2,
                ),
            },
        }
    }

    fn gs(&self, data: &[u8]) -> Step {
        let Some(&cmd) = data.get(1) else {
            return Step::NeedMore;
        };
        let name = || format!("GS {}", printable(cmd));
        match cmd {
            b'!' => param(
                data,
                3,
                |n| {
                    let width = (n >> 4) + 1;
                    let height = (n & 0x0f) + 1;
                    (width <= 8 && height <= 8).then_some(Node::Size { width, height })
                },
                name,
            ),
            b'V' => {
                let Some(&m) = data.get(2) else {
                    return Step::NeedMore;
                };
                match m {
                    0 | 48 => Step::Done(
                        Node::Cut {
                            partial: false,
                            feed: None,
                        },
                        3,
                    ),
                    1 | 49 => Step::Done(
                        Node::Cut {
                            partial: true,
                            feed: None,
                        },
                        3,
                    ),
                    65 | 66 | 97 | 98 | 103 | 104 => match data.get(3) {
                        None => Step::NeedMore,
                        Some(&n) => Step::Done(
                            Node::Cut {
                                partial: m % 2 == 0,
                                feed: Some(n),
                            },
                            4,
                        ),
                    },
                    _ => invalid(&data[..3], name()),
                }
            }
            b'h' => param(data, 3, |n| Some(Node::BarcodeHeight { dots: n }), name),
            b'w' => param(data, 3, |n| Some(Node::BarcodeWidth { dots: n }), name),
            b'H' => param(
                data,
                3,
                |n| match n {
                    0 | 48 => Some(Node::HriPosition {
                        position: HriPosition::None,
                    }),
                    1 | 49 => Some(Node::HriPosition {
                        position: HriPosition::Above,
                    }),
                    2 | 50 => Some(Node::HriPosition {
                        position: HriPosition::Below,
                    }),
                    3 | 51 => Some(Node::HriPosition {
                        position: HriPosition::Both,
                    }),
                    _ => None,
                },
                name,
            ),
            b'k' => barcode(data),
            b'(' => gs_paren(data),
            b'v' => raster(data),
            b'8' => {
                // GS 8 L p1 p2 p3 p4 ...: graphics data with a 32-bit length.
                if data.len() < 7 {
                    return Step::NeedMore;
                }
                if data[2] != b'L' {
                    return Step::Done(unknown(&data[..3], None, "unknown command GS 8".into()), 3);
                }
                let len = u64::from(u32::from_le_bytes([data[3], data[4], data[5], data[6]]));
                let total = 7u64.saturating_add(len);
                match usize::try_from(total) {
                    Ok(total) => sized_unknown(data, 7, total, "GS 8 L".into()),
                    Err(_) => Step::Skip(
                        unknown(&data[..7], Some("GS 8 L".into()), too_large("GS 8 L")),
                        7,
                        len,
                    ),
                }
            }
            b'*' => {
                // GS * x y d1..dk: downloaded bit image, k = x * y * 8.
                if data.len() < 4 {
                    return Step::NeedMore;
                }
                let k = usize::from(data[2]) * usize::from(data[3]) * 8;
                sized_unknown(data, 4, 4 + k, name())
            }
            _ => match gs_fixed_len(cmd) {
                Some(len) => fixed_unknown(data, len, name()),
                None => Step::Done(
                    unknown(&data[..2], None, format!("unknown command {}", name())),
                    2,
                ),
            },
        }
    }
}

/// `GS k`: barcodes. Function A (m = 0..6) is NUL-terminated; function B (m = 65..79) has a
/// length byte.
fn barcode(data: &[u8]) -> Step {
    let Some(&m) = data.get(2) else {
        return Step::NeedMore;
    };
    let (symbology, label) = barcode_kind(m);
    if m <= 6 {
        let body = &data[3..];
        match body.iter().position(|&b| b == 0) {
            Some(end) if end <= MAX_BARCODE_A_BYTES => {
                let total = 3 + end + 1;
                finish_barcode(&data[..total], symbology, label, &body[..end], total)
            }
            Some(_) => cut_off_barcode(data, label),
            None if body.len() > MAX_BARCODE_A_BYTES => cut_off_barcode(data, label),
            None => Step::NeedMore,
        }
    } else if (65..=79).contains(&m) {
        let Some(&n) = data.get(3) else {
            return Step::NeedMore;
        };
        let total = 4 + usize::from(n);
        if data.len() < total {
            return Step::NeedMore;
        }
        finish_barcode(&data[..total], symbology, label, &data[4..total], total)
    } else {
        invalid(&data[..3], "GS k".into())
    }
}

fn finish_barcode(
    raw: &[u8],
    symbology: Option<Symbology>,
    label: &str,
    body: &[u8],
    total: usize,
) -> Step {
    match symbology {
        Some(symbology) => Step::Done(
            Node::Barcode {
                symbology,
                // Latin-1, so every byte (including CODE128 set C values) survives.
                data: body.iter().map(|&b| char::from(b)).collect(),
            },
            total,
        ),
        None => Step::Done(
            unknown(
                raw,
                Some(format!("GS k {label}")),
                format!("{label} barcodes are not emulated"),
            ),
            total,
        ),
    }
}

fn cut_off_barcode(data: &[u8], label: &str) -> Step {
    let len = (3 + MAX_BARCODE_A_BYTES).min(data.len());
    Step::Done(
        unknown(
            &data[..len],
            Some(format!("GS k {label}")),
            format!(
                "barcode data longer than {MAX_BARCODE_A_BYTES} bytes without a NUL terminator"
            ),
        ),
        len,
    )
}

fn barcode_kind(m: u8) -> (Option<Symbology>, &'static str) {
    match m {
        2 | 67 => (Some(Symbology::Ean13), "EAN13"),
        4 | 69 => (Some(Symbology::Code39), "CODE39"),
        73 => (Some(Symbology::Code128), "CODE128"),
        0 | 65 => (None, "UPC-A"),
        1 | 66 => (None, "UPC-E"),
        3 | 68 => (None, "EAN8"),
        5 | 70 => (None, "ITF"),
        6 | 71 => (None, "CODABAR"),
        72 => (None, "CODE93"),
        74..=79 => (None, "GS1"),
        _ => (None, "unknown"),
    }
}

/// `GS ( x pL pH ...`: commands with a 16-bit length. Only `GS ( k` QR functions are emulated.
fn gs_paren(data: &[u8]) -> Step {
    if data.len() < 5 {
        return Step::NeedMore;
    }
    let x = data[2];
    let len = usize::from(data[3]) + usize::from(data[4]) * 256;
    let total = 5 + len;
    if x != b'k' {
        return sized_unknown(data, 5, total, format!("GS ( {}", printable(x)));
    }
    if data.len() < total {
        return Step::NeedMore;
    }
    let raw = &data[..total];
    let body = &data[5..total];
    let (Some(&cn), Some(&f)) = (body.first(), body.get(1)) else {
        return Step::Done(
            unknown(
                raw,
                Some("GS ( k".into()),
                "GS ( k without a function".into(),
            ),
            total,
        );
    };
    if cn != 49 {
        let symbol = match cn {
            48 => "PDF417",
            50 => "MaxiCode",
            51 => "GS1 DataBar",
            52 => "Composite",
            53 => "Aztec",
            54 => "DataMatrix",
            _ => "unknown symbol",
        };
        return Step::Done(
            unknown(
                raw,
                Some(format!("GS ( k {symbol}")),
                format!("{symbol} symbols are not emulated"),
            ),
            total,
        );
    }
    let params = &body[2..];
    let node = match (f, params) {
        (65, [n1, ..]) => match n1 {
            49 => Some(Node::QrModel { model: 1 }),
            50 => Some(Node::QrModel { model: 2 }),
            51 => Some(Node::QrModel { model: 3 }),
            _ => None,
        },
        (67, [n, ..]) => Some(Node::QrModuleSize { dots: *n }),
        (69, [n, ..]) => match n {
            48 => Some(Node::QrErrorCorrection {
                level: QrErrorCorrection::L,
            }),
            49 => Some(Node::QrErrorCorrection {
                level: QrErrorCorrection::M,
            }),
            50 => Some(Node::QrErrorCorrection {
                level: QrErrorCorrection::Q,
            }),
            51 => Some(Node::QrErrorCorrection {
                level: QrErrorCorrection::H,
            }),
            _ => None,
        },
        (80, [48, data @ ..]) => Some(Node::QrStore {
            text: String::from_utf8_lossy(data).into_owned(),
            data: data.to_vec(),
        }),
        (81, [48, ..]) => Some(Node::QrPrint),
        _ => None,
    };
    match node {
        Some(node) => Step::Done(node, total),
        None => Step::Done(
            unknown(
                raw,
                Some("GS ( k QR".into()),
                format!("QR code function {} is not emulated", u16::from(f) + 100),
            ),
            total,
        ),
    }
}

/// `GS v 0 m xL xH yL yH d1..dk`: raster bit image, k = (xL + xH * 256) * (yL + yH * 256).
fn raster(data: &[u8]) -> Step {
    let Some(&kind) = data.get(2) else {
        return Step::NeedMore;
    };
    if kind != b'0' {
        return Step::Done(
            unknown(
                &data[..3],
                None,
                format!("unknown command GS v {}", printable(kind)),
            ),
            3,
        );
    }
    if data.len() < 8 {
        return Step::NeedMore;
    }
    let m = data[3];
    if !matches!(m, 0..=3 | 48..=51) {
        return invalid(&data[..8], "GS v 0".into());
    }
    let width_bytes = u16::from_le_bytes([data[4], data[5]]);
    let height = u16::from_le_bytes([data[6], data[7]]);
    let k = usize::from(width_bytes) * usize::from(height);
    if k > MAX_IMAGE_BYTES {
        return Step::Skip(
            unknown(&data[..8], Some("GS v 0".into()), too_large("GS v 0")),
            8,
            k as u64,
        );
    }
    let total = 8 + k;
    if data.len() < total {
        return Step::NeedMore;
    }
    Step::Done(
        Node::RasterImage {
            width_bytes,
            height,
            double_width: m & 1 == 1,
            double_height: m & 2 == 2,
            data: data[8..total].to_vec(),
        },
        total,
    )
}

fn fs(data: &[u8]) -> Step {
    let Some(&cmd) = data.get(1) else {
        return Step::NeedMore;
    };
    let name = format!("FS {}", printable(cmd));
    let len = match cmd {
        b'!' | b'-' | b'C' | b'W' => Some(3),
        b'&' | b'.' => Some(2),
        b'S' | b'p' => Some(4),
        _ => None,
    };
    match len {
        Some(len) => fixed_unknown(data, len, name),
        None => Step::Done(
            unknown(&data[..2], None, format!("unknown command {name}")),
            2,
        ),
    }
}

fn dle(data: &[u8]) -> Step {
    let Some(&cmd) = data.get(1) else {
        return Step::NeedMore;
    };
    match cmd {
        0x04 => {
            let Some(&n) = data.get(2) else {
                return Step::NeedMore;
            };
            let kind = match n {
                1 => StatusKind::Printer,
                2 => StatusKind::OfflineCause,
                3 => StatusKind::ErrorCause,
                4 => StatusKind::PaperSensor,
                7 | 8 => return fixed_unknown(data, 4, format!("DLE EOT {n}")),
                _ => return invalid(&data[..3], "DLE EOT".into()),
            };
            Step::Done(Node::Status { kind }, 3)
        }
        0x05 => fixed_unknown(data, 3, "DLE ENQ".into()),
        0x14 => {
            let Some(&f) = data.get(2) else {
                return Step::NeedMore;
            };
            if data.len() < 5 {
                return Step::NeedMore;
            }
            if f == 1 {
                let pin = match data[3] {
                    0 => 2,
                    1 => 5,
                    _ => return invalid(&data[..5], "DLE DC4 1".into()),
                };
                let ms = u64::from(data[4]) * 100;
                return Step::Done(
                    Node::Pulse {
                        pin,
                        on_ms: ms,
                        off_ms: ms,
                        realtime: true,
                    },
                    5,
                );
            }
            Step::Done(
                unknown(
                    &data[..5],
                    Some(format!("DLE DC4 {f}")),
                    format!("DLE DC4 {f} is not emulated"),
                ),
                5,
            )
        }
        _ => Step::Done(
            unknown(&data[..2], None, format!("unknown command DLE 0x{cmd:02x}")),
            2,
        ),
    }
}

/// Total length of `ESC x` commands that take fixed parameters but are not emulated.
fn esc_fixed_len(cmd: u8) -> Option<usize> {
    match cmd {
        b'2' | b'L' | b'S' | b'v' => Some(2),
        b' ' | b'%' | b'3' | b'=' | b'?' | b'G' | b'J' | b'R' | b'T' | b'U' | b'V' | b'e'
        | b'r' | b'u' | b'{' => Some(3),
        b'$' | b'\\' | b'c' => Some(4),
        b'W' => Some(10),
        _ => None,
    }
}

/// Total length of `GS x` commands that take fixed parameters but are not emulated.
fn gs_fixed_len(cmd: u8) -> Option<usize> {
    match cmd {
        b':' | b'c' => Some(2),
        b'/' | b'B' | b'I' | b'a' | b'b' | b'f' | b'r' => Some(3),
        b'$' | b'L' | b'P' | b'W' | b'\\' => Some(4),
        b'^' => Some(5),
        _ => None,
    }
}

/// A command with one parameter byte at index `len - 1`, mapped by `f`; `None` means the
/// parameter is invalid.
fn param(
    data: &[u8],
    len: usize,
    f: impl FnOnce(u8) -> Option<Node>,
    name: impl FnOnce() -> String,
) -> Step {
    if data.len() < len {
        return Step::NeedMore;
    }
    match f(data[len - 1]) {
        Some(node) => Step::Done(node, len),
        None => invalid(&data[..len], name()),
    }
}

fn invalid(raw: &[u8], name: String) -> Step {
    let warning = format!("invalid parameter for {name}");
    Step::Done(unknown(raw, Some(name), warning), raw.len())
}

fn fixed_unknown(data: &[u8], len: usize, name: String) -> Step {
    if data.len() < len {
        return Step::NeedMore;
    }
    let warning = format!("{name} is not emulated");
    Step::Done(unknown(&data[..len], Some(name), warning), len)
}

/// An unsupported command of `total` bytes with a `header`-byte header: kept whole if small,
/// otherwise reported by its header and skipped.
fn sized_unknown(data: &[u8], header: usize, total: usize, name: String) -> Step {
    if total - header > MAX_UNKNOWN_BYTES {
        let skip = (total - header) as u64;
        return Step::Skip(
            unknown(&data[..header], Some(name.clone()), too_large(&name)),
            header,
            skip,
        );
    }
    fixed_unknown(data, total, name)
}

/// An unsupported command whose parameters end at a NUL, at most `max` bytes after the
/// `header`.
fn terminated_unknown(data: &[u8], header: usize, max: usize, name: String) -> Step {
    let body = &data[header..];
    match body.iter().take(max).position(|&b| b == 0) {
        Some(end) => fixed_unknown(data, header + end + 1, name),
        None if body.len() >= max => {
            let len = header + max;
            let warning = format!("{name} without a NUL terminator");
            Step::Done(unknown(&data[..len], Some(name), warning), len)
        }
        None => Step::NeedMore,
    }
}

fn unknown(raw: &[u8], command: Option<String>, warning: String) -> Node {
    Node::Unknown {
        bytes: raw.to_vec(),
        command,
        warning,
    }
}

fn too_large(name: &str) -> String {
    format!("{name} data is too large to keep; it was skipped")
}

/// A command byte as written in the reference: the ASCII character, or hex.
fn printable(byte: u8) -> String {
    if byte.is_ascii_graphic() {
        char::from(byte).to_string()
    } else {
        format!("0x{byte:02x}")
    }
}

fn control_name(byte: u8) -> Option<String> {
    let name = match byte {
        0x00 => "NUL",
        0x0c => "FF",
        0x18 => "CAN",
        _ => return None,
    };
    Some(name.to_owned())
}
