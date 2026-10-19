//! Builds ESC/POS bytes. Both dialects are translated to ESC/POS so the printer device parses
//! them into the same command AST as a Web Serial print (FR-31).
//!
//! Commands follow the public ESC/POS command reference.

use std::collections::HashMap;
use std::sync::OnceLock;

use hardless_escpos::{CodePages, TABLE_PC437, TABLE_PC858};

/// Horizontal alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Align {
    Left,
    Center,
    Right,
}

/// HRI text position for barcodes (`GS H`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hri {
    None,
    Above,
    Below,
    Both,
}

/// QR error correction level (`GS ( k` function 169).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QrLevel {
    L,
    M,
    Q,
    H,
}

/// Unicode to byte for the code pages the printer knows, built from its decode tables.
struct Encoder {
    pc437: HashMap<char, u8>,
    pc858: HashMap<char, u8>,
}

fn encoder() -> &'static Encoder {
    static ENCODER: OnceLock<Encoder> = OnceLock::new();
    ENCODER.get_or_init(|| {
        let pages = CodePages::default();
        let table = |n: u8| -> HashMap<char, u8> {
            let mut map = HashMap::new();
            if let Some(page) = pages.get(n) {
                for byte in 0x80..=0xFF_u8 {
                    map.entry(page.decode_high(byte)).or_insert(byte);
                }
            }
            map
        };
        Encoder {
            pc437: table(TABLE_PC437),
            pc858: table(TABLE_PC858),
        }
    })
}

/// An ESC/POS document being built.
#[derive(Debug)]
pub(crate) struct EscPos {
    out: Vec<u8>,
    /// The `ESC t` table in effect.
    table: u8,
}

impl EscPos {
    /// A document that starts with `ESC @`, so every request starts from the power-on state.
    pub fn new() -> Self {
        Self {
            out: vec![0x1B, 0x40],
            table: TABLE_PC437,
        }
    }

    pub fn finish(self) -> Vec<u8> {
        self.out
    }

    pub fn raw(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
    }

    /// Text, encoded in CP437, switching to CP858 for characters only it has (such as `€`).
    /// Characters neither table has print as `?`. `\n` is a line feed.
    pub fn text(&mut self, text: &str) {
        let enc = encoder();
        for c in text.chars() {
            match c {
                '\n' => self.out.push(0x0A),
                '\r' => {}
                '\t' => self.out.push(0x09),
                c if (' '..='~').contains(&c) => self.out.push(c as u8),
                c if c.is_control() => {}
                c => {
                    let current = if self.table == TABLE_PC858 {
                        &enc.pc858
                    } else {
                        &enc.pc437
                    };
                    if let Some(&b) = current.get(&c) {
                        self.out.push(b);
                    } else if let Some(&b) = enc.pc858.get(&c) {
                        self.select_table(TABLE_PC858);
                        self.out.push(b);
                    } else if let Some(&b) = enc.pc437.get(&c) {
                        self.select_table(TABLE_PC437);
                        self.out.push(b);
                    } else {
                        self.out.push(b'?');
                    }
                }
            }
        }
    }

    fn select_table(&mut self, n: u8) {
        self.out.extend_from_slice(&[0x1B, 0x74, n]);
        self.table = n;
    }

    pub fn line_feed(&mut self) {
        self.out.push(0x0A);
    }

    /// `ESC a n`.
    pub fn align(&mut self, align: Align) {
        let n = match align {
            Align::Left => 0,
            Align::Center => 1,
            Align::Right => 2,
        };
        self.out.extend_from_slice(&[0x1B, 0x61, n]);
    }

    /// `ESC E n`.
    pub fn bold(&mut self, on: bool) {
        self.out.extend_from_slice(&[0x1B, 0x45, u8::from(on)]);
    }

    /// `ESC - n`: 0 off, 1 or 2 dots.
    pub fn underline(&mut self, dots: u8) {
        self.out.extend_from_slice(&[0x1B, 0x2D, dots.min(2)]);
    }

    /// `GS ! n`: width and height multipliers, 1 to 8.
    pub fn size(&mut self, width: u8, height: u8) {
        let w = width.clamp(1, 8) - 1;
        let h = height.clamp(1, 8) - 1;
        self.out.extend_from_slice(&[0x1D, 0x21, (w << 4) | h]);
    }

    /// `ESC M n`: 0 Font A, 1 Font B.
    pub fn font_b(&mut self, on: bool) {
        self.out.extend_from_slice(&[0x1B, 0x4D, u8::from(on)]);
    }

    /// `ESC d n`.
    pub fn feed_lines(&mut self, lines: u8) {
        self.out.extend_from_slice(&[0x1B, 0x64, lines]);
    }

    /// `GS V`: `GS V 1` (partial, no feed), or `GS V 65/66 0` (feed, then full/partial cut).
    pub fn cut(&mut self, partial: bool, feed: bool) {
        match (feed, partial) {
            (false, false) => self.out.extend_from_slice(&[0x1D, 0x56, 0x00]),
            (false, true) => self.out.extend_from_slice(&[0x1D, 0x56, 0x01]),
            (true, false) => self.out.extend_from_slice(&[0x1D, 0x56, 0x41, 0x00]),
            (true, true) => self.out.extend_from_slice(&[0x1D, 0x56, 0x42, 0x00]),
        }
    }

    /// `ESC p m t1 t2`: a drawer kick on connector pin 2 (`m = 0`) or 5 (`m = 1`); times in
    /// milliseconds, sent in 2 ms units.
    pub fn pulse(&mut self, pin5: bool, on_ms: u32, off_ms: u32) {
        let unit = |ms: u32| u8::try_from(ms.div_ceil(2)).unwrap_or(u8::MAX).max(1);
        self.out
            .extend_from_slice(&[0x1B, 0x70, u8::from(pin5), unit(on_ms), unit(off_ms)]);
    }

    /// `GS H`, `GS w`, `GS h`, then `GS k m n data` (function B).
    pub fn barcode(&mut self, m: u8, data: &[u8], hri: Hri, module: u8, height: u8) {
        let hri = match hri {
            Hri::None => 0,
            Hri::Above => 1,
            Hri::Below => 2,
            Hri::Both => 3,
        };
        self.out.extend_from_slice(&[0x1D, 0x48, hri]);
        self.out
            .extend_from_slice(&[0x1D, 0x77, module.clamp(1, 6)]);
        self.out.extend_from_slice(&[0x1D, 0x68, height.max(1)]);
        let data = &data[..data.len().min(255)];
        self.out
            .extend_from_slice(&[0x1D, 0x6B, m, data.len() as u8]);
        self.out.extend_from_slice(data);
    }

    /// `GS ( k` for a symbol: `cn` 49 is QR Code; other symbol types go out with their own
    /// `cn` and print as unknown commands, as on a printer without them.
    fn gs_paren_k(&mut self, cn: u8, function: u8, params: &[u8]) {
        let len = params.len() + 2;
        let (pl, ph) = ((len & 0xFF) as u8, ((len >> 8) & 0xFF) as u8);
        self.out
            .extend_from_slice(&[0x1D, 0x28, 0x6B, pl, ph, cn, function]);
        self.out.extend_from_slice(params);
    }

    /// QR Code: model (1 or 2), module size in dots, error correction, data, print.
    pub fn qr(&mut self, model: u8, module: u8, level: QrLevel, data: &[u8]) {
        self.gs_paren_k(49, 65, &[if model == 1 { 49 } else { 50 }, 0]);
        self.gs_paren_k(49, 67, &[module.clamp(1, 16)]);
        let level = match level {
            QrLevel::L => 48,
            QrLevel::M => 49,
            QrLevel::Q => 50,
            QrLevel::H => 51,
        };
        self.gs_paren_k(49, 69, &[level]);
        let data = &data[..data.len().min(7089)];
        let mut store = vec![48];
        store.extend_from_slice(data);
        self.gs_paren_k(49, 80, &store);
        self.gs_paren_k(49, 81, &[48]);
    }

    /// A 2D symbol the printer does not draw (PDF417, DataMatrix, ...): stored and printed
    /// with its own `GS ( k` code, which the printer reports as unknown.
    pub fn other_symbol(&mut self, cn: u8, data: &[u8]) {
        let data = &data[..data.len().min(4096)];
        let mut store = vec![48];
        store.extend_from_slice(data);
        self.gs_paren_k(cn, 80, &store);
        self.gs_paren_k(cn, 81, &[48]);
    }

    /// `GS v 0`: a 1-bit raster image, `width` dots wide, rows MSB first. Missing data is
    /// padded with white; extra data is dropped.
    pub fn raster(&mut self, width: u32, height: u32, data: &[u8]) {
        let width_bytes = width.div_ceil(8).min(0xFFFF);
        let height = height.min(0xFFFF);
        let len = (width_bytes * height) as usize;
        self.out.extend_from_slice(&[
            0x1D,
            0x76,
            0x30,
            0x00,
            (width_bytes & 0xFF) as u8,
            (width_bytes >> 8) as u8,
            (height & 0xFF) as u8,
            (height >> 8) as u8,
        ]);
        let take = data.len().min(len);
        self.out.extend_from_slice(&data[..take]);
        self.out.resize(self.out.len() + (len - take), 0);
    }
}

/// Decodes `\xNN` (a byte) and `\\` (a backslash) escapes, as used in barcode and symbol data.
/// Other text passes through as UTF-8.
pub(crate) fn unescape(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            if bytes.get(i + 1) == Some(&b'\\') {
                out.push(b'\\');
                i += 2;
                continue;
            }
            if bytes.get(i + 1) == Some(&b'x') {
                let hex = bytes
                    .get(i + 2..i + 4)
                    .and_then(|h| std::str::from_utf8(h).ok());
                if let Some(value) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    out.push(value);
                    i += 4;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_switches_code_page_only_when_needed() {
        let mut doc = EscPos::new();
        doc.text("é€\n");
        // é is in CP437 (0x82); € needs CP858 (0xD5).
        assert_eq!(doc.finish(), b"\x1b@\x82\x1bt\x13\xd5\n");
    }

    #[test]
    fn unknown_characters_print_as_question_marks() {
        let mut doc = EscPos::new();
        doc.text("日");
        assert_eq!(doc.finish(), b"\x1b@?");
    }

    #[test]
    fn escapes() {
        assert_eq!(unescape(r"A\x0aB\\C\xZZ"), b"A\nB\\C\\xZZ");
    }

    #[test]
    fn raster_pads_and_truncates() {
        let mut doc = EscPos::new();
        doc.raster(9, 2, &[0xFF]);
        assert_eq!(
            doc.finish(),
            [0x1B, 0x40, 0x1D, 0x76, 0x30, 0, 2, 0, 2, 0, 0xFF, 0, 0, 0]
        );
    }
}
