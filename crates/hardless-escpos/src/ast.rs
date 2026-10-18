//! The command AST.
//!
//! One [`Node`] per command, in the order the printer receives them. Nodes describe what was
//! sent, not the resulting layout: the renderer replays them to build the receipt.

use serde::{Deserialize, Serialize};

/// Text justification (`ESC a n`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum Align {
    /// Left (the default).
    #[default]
    Left,
    /// Centered.
    Center,
    /// Right.
    Right,
}

/// Character font (`ESC M n`, `ESC ! n`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum Font {
    /// Font A, 12x24 dots (the default).
    #[default]
    A,
    /// Font B, 9x17 dots.
    B,
    /// Font C, where the printer has one.
    C,
}

/// Where barcode text (HRI, human readable interpretation) is printed (`GS H n`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum HriPosition {
    /// Not printed (the default).
    #[default]
    None,
    /// Above the barcode.
    Above,
    /// Below the barcode.
    Below,
    /// Above and below.
    Both,
}

/// A barcode symbology this parser renders (`GS k`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "kebab-case")]
pub enum Symbology {
    /// CODE128. The data starts with a code set selector: `{A`, `{B` or `{C`.
    Code128,
    /// EAN-13: 12 digits (the printer adds the check digit) or 13.
    Ean13,
    /// CODE39.
    Code39,
}

/// QR code error correction level (`GS ( k`, function 169).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum QrErrorCorrection {
    /// About 7% recovery (the default).
    #[default]
    L,
    /// About 15%.
    M,
    /// About 25%.
    Q,
    /// About 30%.
    H,
}

/// Which status a `DLE EOT n` request asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "kebab-case")]
pub enum StatusKind {
    /// `n = 1`: printer status (drawer sensor, online/offline).
    Printer,
    /// `n = 2`: offline cause (cover open, paper end).
    OfflineCause,
    /// `n = 3`: error cause.
    ErrorCause,
    /// `n = 4`: roll paper sensor (near end, end).
    PaperSensor,
}

/// One ESC/POS command.
///
/// On the wire (JSON), nodes are tagged by `type` in kebab-case with camelCase fields; byte
/// payloads are lowercase hex.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Node {
    /// `ESC @`: reset formatting and code page.
    Init,
    /// Printable text, decoded with the code page active when it was received.
    Text {
        /// The decoded text.
        text: String,
    },
    /// `LF`: print the line and feed one line.
    LineFeed,
    /// `CR`: carriage return (no line feed).
    CarriageReturn,
    /// `HT`: move to the next tab stop (every 8 columns).
    Tab,
    /// `ESC d n`: print and feed `lines` lines.
    Feed {
        /// Lines to feed.
        lines: u8,
    },
    /// `ESC a n`: justification.
    Align {
        /// The new justification.
        align: Align,
    },
    /// `ESC E n`: emphasized (bold) on or off.
    Bold {
        /// Whether bold is on.
        on: bool,
    },
    /// `ESC - n`: underline thickness; 0 is off.
    Underline {
        /// 0, 1 or 2 dots.
        dots: u8,
    },
    /// `GS ! n`: character size multipliers, 1 to 8.
    Size {
        /// Width multiplier.
        width: u8,
        /// Height multiplier.
        height: u8,
    },
    /// `ESC ! n`: print mode. Sets font, bold, size and underline in one command.
    #[serde(rename_all = "camelCase")]
    PrintMode {
        /// Font.
        font: Font,
        /// Emphasized.
        bold: bool,
        /// Double height.
        double_height: bool,
        /// Double width.
        double_width: bool,
        /// Underline (1 dot).
        underline: bool,
    },
    /// `ESC M n`: character font.
    Font {
        /// The font.
        font: Font,
    },
    /// `ESC t n`: character code table.
    CodePage {
        /// The table number `n`.
        table: u8,
        /// The code page's name if it is built in or registered, e.g. `CP858`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// `GS V`, `ESC i`, `ESC m`: cut the paper.
    Cut {
        /// A partial cut (one point left uncut) rather than a full cut.
        partial: bool,
        /// Lines (`GS V m n`: n x motion units) fed before cutting, if given.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        feed: Option<u8>,
    },
    /// `ESC p m t1 t2` or `DLE DC4 1 m t`: drawer kick pulse.
    #[serde(rename_all = "camelCase")]
    Pulse {
        /// Connector pin, 2 or 5.
        pin: u8,
        /// On time in milliseconds.
        on_ms: u64,
        /// Off time in milliseconds.
        off_ms: u64,
    },
    /// `GS ( k` function 165: QR code model (1, 2 or 3 for Micro QR).
    QrModel {
        /// The model number.
        model: u8,
    },
    /// `GS ( k` function 167: QR module size in dots (1 to 16).
    QrModuleSize {
        /// Dots per module.
        dots: u8,
    },
    /// `GS ( k` function 169: QR error correction level.
    QrErrorCorrection {
        /// The level.
        level: QrErrorCorrection,
    },
    /// `GS ( k` function 180: store QR data.
    QrStore {
        /// The data, as text (lossy UTF-8).
        text: String,
        /// The data bytes, as hex.
        #[serde(rename = "dataHex", with = "crate::hex::serde_vec")]
        #[cfg_attr(feature = "ts", ts(rename = "dataHex", type = "string"))]
        data: Vec<u8>,
    },
    /// `GS ( k` function 181: print the stored QR code.
    QrPrint,
    /// `GS h n`: barcode height in dots.
    BarcodeHeight {
        /// Height in dots (1 to 255).
        dots: u8,
    },
    /// `GS w n`: barcode module width in dots (2 to 6).
    BarcodeWidth {
        /// Module width in dots.
        dots: u8,
    },
    /// `GS H n`: where the barcode text is printed.
    HriPosition {
        /// The position.
        position: HriPosition,
    },
    /// `GS k`: print a barcode.
    Barcode {
        /// The symbology.
        symbology: Symbology,
        /// The barcode data as sent (ASCII).
        data: String,
    },
    /// `GS v 0 m xL xH yL yH d...`: raster bit image, 1 bit per dot, MSB first.
    #[serde(rename_all = "camelCase")]
    RasterImage {
        /// Width in bytes (8 dots per byte).
        width_bytes: u16,
        /// Height in dots.
        height: u16,
        /// Scale: double width.
        double_width: bool,
        /// Scale: double height.
        double_height: bool,
        /// The image rows, as hex.
        #[serde(rename = "dataHex", with = "crate::hex::serde_vec")]
        #[cfg_attr(feature = "ts", ts(rename = "dataHex", type = "string"))]
        data: Vec<u8>,
    },
    /// `DLE EOT n`: real-time status request. The printer answers with one status byte.
    Status {
        /// Which status was requested.
        kind: StatusKind,
    },
    /// A command this parser does not emulate, or bytes it cannot parse (FR-08).
    Unknown {
        /// The raw bytes of the command, as hex.
        #[serde(rename = "bytesHex", with = "crate::hex::serde_vec")]
        #[cfg_attr(feature = "ts", ts(rename = "bytesHex", type = "string"))]
        bytes: Vec<u8>,
        /// The command's name if it is a known command, e.g. `ESC 3` or `GS ( k PDF417`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        command: Option<String>,
        /// Why it is unknown.
        warning: String,
    },
}

impl Node {
    /// Whether this node puts something on paper (or moves it): text, feeds, cuts, codes and
    /// images. Status requests, settings and drawer pulses do not.
    ///
    /// ```
    /// use hardless_escpos::Node;
    /// assert!(Node::LineFeed.is_printing());
    /// assert!(!Node::Init.is_printing());
    /// ```
    pub fn is_printing(&self) -> bool {
        matches!(
            self,
            Node::Text { .. }
                | Node::LineFeed
                | Node::Feed { .. }
                | Node::Cut { .. }
                | Node::QrPrint
                | Node::Barcode { .. }
                | Node::RasterImage { .. }
        )
    }
}
