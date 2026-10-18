//! Character code tables (FR-09): CP437 and CP858 built in, others pluggable.
//!
//! Bytes `0x20..=0x7F` are ASCII in every table; a table maps the upper half, `0x80..=0xFF`.
//! The built-in tables match the Windows code page definitions.

use std::collections::BTreeMap;
use std::sync::Arc;

/// A character code table that `ESC t n` can select.
pub trait CodePage: Send + Sync {
    /// The name shown in the AST, e.g. `CP858`.
    fn name(&self) -> &str;
    /// The character for a byte in `0x80..=0xFF`.
    fn decode_high(&self, byte: u8) -> char;
}

/// A code page defined by a 128-entry table for `0x80..=0xFF`.
///
/// ```
/// use hardless_escpos::{CodePage, TableCodePage};
/// let page = TableCodePage::new("CP-TEST", ['?'; 128]);
/// assert_eq!(page.decode_high(0x80), '?');
/// ```
#[derive(Clone, Debug)]
pub struct TableCodePage {
    name: String,
    upper: [char; 128],
}

impl TableCodePage {
    /// A code page from its upper-half table.
    pub fn new(name: impl Into<String>, upper: [char; 128]) -> Self {
        Self {
            name: name.into(),
            upper,
        }
    }
}

impl CodePage for TableCodePage {
    fn name(&self) -> &str {
        &self.name
    }

    fn decode_high(&self, byte: u8) -> char {
        self.upper
            .get(usize::from(byte.wrapping_sub(0x80)))
            .copied()
            .unwrap_or(char::REPLACEMENT_CHARACTER)
    }
}

struct Builtin {
    name: &'static str,
    upper: &'static [u16; 128],
}

impl CodePage for Builtin {
    fn name(&self) -> &str {
        self.name
    }

    fn decode_high(&self, byte: u8) -> char {
        self.upper
            .get(usize::from(byte.wrapping_sub(0x80)))
            .and_then(|&c| char::from_u32(u32::from(c)))
            .unwrap_or(char::REPLACEMENT_CHARACTER)
    }
}

/// `ESC t` table number of PC437 (USA, standard Europe).
pub const TABLE_PC437: u8 = 0;
/// `ESC t` table number of PC858 (PC850 with the euro sign).
pub const TABLE_PC858: u8 = 19;

/// The code pages a [`Parser`](crate::Parser) can switch between, keyed by `ESC t` number.
///
/// ```
/// use hardless_escpos::{CodePages, TableCodePage};
/// let pages = CodePages::default().with(17, TableCodePage::new("CP866", ['?'; 128]));
/// assert_eq!(pages.get(17).unwrap().name(), "CP866");
/// assert_eq!(pages.get(19).unwrap().name(), "CP858");
/// ```
#[derive(Clone)]
pub struct CodePages {
    tables: BTreeMap<u8, Arc<dyn CodePage>>,
}

impl Default for CodePages {
    /// CP437 as table 0 and CP858 as table 19.
    fn default() -> Self {
        let mut tables: BTreeMap<u8, Arc<dyn CodePage>> = BTreeMap::new();
        tables.insert(
            TABLE_PC437,
            Arc::new(Builtin {
                name: "CP437",
                upper: &CP437,
            }),
        );
        tables.insert(
            TABLE_PC858,
            Arc::new(Builtin {
                name: "CP858",
                upper: &CP858,
            }),
        );
        Self { tables }
    }
}

impl CodePages {
    /// Adds or replaces the table for `ESC t n`.
    pub fn with(mut self, n: u8, page: impl CodePage + 'static) -> Self {
        self.tables.insert(n, Arc::new(page));
        self
    }

    /// The table for `ESC t n`, if known.
    pub fn get(&self, n: u8) -> Option<&dyn CodePage> {
        self.tables.get(&n).map(|p| p.as_ref())
    }

    /// Decodes one printable byte (`0x20..=0xFF`) with table `n`; unknown tables fall back to
    /// CP437.
    pub(crate) fn decode(&self, n: u8, byte: u8) -> char {
        if byte < 0x80 {
            return char::from(byte);
        }
        self.get(n)
            .or_else(|| self.get(TABLE_PC437))
            .map_or(char::REPLACEMENT_CHARACTER, |page| page.decode_high(byte))
    }
}

impl std::fmt::Debug for CodePages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(self.tables.iter().map(|(n, page)| (n, page.name())))
            .finish()
    }
}

/// CP437 bytes 0x80..=0xFF (Windows code page 437).
static CP437: [u16; 128] = [
    0x00C7, 0x00FC, 0x00E9, 0x00E2, 0x00E4, 0x00E0, 0x00E5, 0x00E7, 0x00EA, 0x00EB, 0x00E8, 0x00EF,
    0x00EE, 0x00EC, 0x00C4, 0x00C5, 0x00C9, 0x00E6, 0x00C6, 0x00F4, 0x00F6, 0x00F2, 0x00FB, 0x00F9,
    0x00FF, 0x00D6, 0x00DC, 0x00A2, 0x00A3, 0x00A5, 0x20A7, 0x0192, 0x00E1, 0x00ED, 0x00F3, 0x00FA,
    0x00F1, 0x00D1, 0x00AA, 0x00BA, 0x00BF, 0x2310, 0x00AC, 0x00BD, 0x00BC, 0x00A1, 0x00AB, 0x00BB,
    0x2591, 0x2592, 0x2593, 0x2502, 0x2524, 0x2561, 0x2562, 0x2556, 0x2555, 0x2563, 0x2551, 0x2557,
    0x255D, 0x255C, 0x255B, 0x2510, 0x2514, 0x2534, 0x252C, 0x251C, 0x2500, 0x253C, 0x255E, 0x255F,
    0x255A, 0x2554, 0x2569, 0x2566, 0x2560, 0x2550, 0x256C, 0x2567, 0x2568, 0x2564, 0x2565, 0x2559,
    0x2558, 0x2552, 0x2553, 0x256B, 0x256A, 0x2518, 0x250C, 0x2588, 0x2584, 0x258C, 0x2590, 0x2580,
    0x03B1, 0x00DF, 0x0393, 0x03C0, 0x03A3, 0x03C3, 0x00B5, 0x03C4, 0x03A6, 0x0398, 0x03A9, 0x03B4,
    0x221E, 0x03C6, 0x03B5, 0x2229, 0x2261, 0x00B1, 0x2265, 0x2264, 0x2320, 0x2321, 0x00F7, 0x2248,
    0x00B0, 0x2219, 0x00B7, 0x221A, 0x207F, 0x00B2, 0x25A0, 0x00A0,
];

/// CP858 bytes 0x80..=0xFF (Windows code page 858: CP850 with the euro sign at 0xD5).
static CP858: [u16; 128] = [
    0x00C7, 0x00FC, 0x00E9, 0x00E2, 0x00E4, 0x00E0, 0x00E5, 0x00E7, 0x00EA, 0x00EB, 0x00E8, 0x00EF,
    0x00EE, 0x00EC, 0x00C4, 0x00C5, 0x00C9, 0x00E6, 0x00C6, 0x00F4, 0x00F6, 0x00F2, 0x00FB, 0x00F9,
    0x00FF, 0x00D6, 0x00DC, 0x00F8, 0x00A3, 0x00D8, 0x00D7, 0x0192, 0x00E1, 0x00ED, 0x00F3, 0x00FA,
    0x00F1, 0x00D1, 0x00AA, 0x00BA, 0x00BF, 0x00AE, 0x00AC, 0x00BD, 0x00BC, 0x00A1, 0x00AB, 0x00BB,
    0x2591, 0x2592, 0x2593, 0x2502, 0x2524, 0x00C1, 0x00C2, 0x00C0, 0x00A9, 0x2563, 0x2551, 0x2557,
    0x255D, 0x00A2, 0x00A5, 0x2510, 0x2514, 0x2534, 0x252C, 0x251C, 0x2500, 0x253C, 0x00E3, 0x00C3,
    0x255A, 0x2554, 0x2569, 0x2566, 0x2560, 0x2550, 0x256C, 0x00A4, 0x00F0, 0x00D0, 0x00CA, 0x00CB,
    0x00C8, 0x20AC, 0x00CD, 0x00CE, 0x00CF, 0x2518, 0x250C, 0x2588, 0x2584, 0x00A6, 0x00CC, 0x2580,
    0x00D3, 0x00DF, 0x00D4, 0x00D2, 0x00F5, 0x00D5, 0x00B5, 0x00FE, 0x00DE, 0x00DA, 0x00DB, 0x00D9,
    0x00FD, 0x00DD, 0x00AF, 0x00B4, 0x00AD, 0x00B1, 0x2017, 0x00BE, 0x00B6, 0x00A7, 0x00F7, 0x00B8,
    0x00B0, 0x00A8, 0x00B7, 0x00B9, 0x00B3, 0x00B2, 0x25A0, 0x00A0,
];
