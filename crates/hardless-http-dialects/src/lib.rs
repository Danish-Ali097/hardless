#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// This crate's README, which holds the request and response reference for both dialects.
/// The MCP server's `hardless://docs/http-dialects` resource and the docs site read it, so
/// the reference has one source.
pub const REFERENCE: &str = include_str!("../README.md");

pub mod epos;
mod escpos;
pub mod webprnt;
mod xml;

use std::fmt;

/// Largest request document accepted, in bytes. Real receipts with images stay far below it.
pub const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;

/// A print request translated to ESC/POS (FR-31). Feeding [`PrintRequest::escpos`] to an
/// ESC/POS printer gives the same command AST as printing over Web Serial.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrintRequest {
    /// The equivalent ESC/POS bytes. They start with `ESC @`: each request starts from the
    /// printer's power-on settings.
    pub escpos: Vec<u8>,
    /// Parts of the request the emulator does not reproduce (a buzzer, page mode, ...), for
    /// the event log.
    pub warnings: Vec<String>,
}

/// What a request document could not be understood for. Both dialects answer it as a
/// schema error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialectError(String);

impl DialectError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for DialectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DialectError {}

/// The printer's condition after a print, as both dialects report it.
///
/// Read it back from the printer's own `DLE EOT` replies with
/// [`PrinterStatus::from_dle_eot`], so faults and the drawer setting count exactly as they do
/// for Web Serial.
///
/// ```
/// use hardless_http_dialects::PrinterStatus;
/// // DLE EOT 1, 2 and 4 replies of a printer that is out of paper.
/// let status = PrinterStatus::from_dle_eot(0x1A, 0x32, 0x7E);
/// assert!(status.paper_end && status.paper_near_end && !status.online);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrinterStatus {
    /// The printer is online (not stopped by paper end or an open cover).
    pub online: bool,
    /// The cover is open.
    pub cover_open: bool,
    /// The roll is empty.
    pub paper_end: bool,
    /// The roll is nearly empty.
    pub paper_near_end: bool,
    /// The drawer-kick connector's pin 3 (the drawer sensor) reads high.
    pub drawer_pin3_high: bool,
}

impl PrinterStatus {
    /// From the replies to `DLE EOT 1` (printer), `DLE EOT 2` (offline cause) and `DLE EOT 4`
    /// (paper sensor), as defined in the ESC/POS command reference.
    pub fn from_dle_eot(printer: u8, offline_cause: u8, paper: u8) -> Self {
        Self {
            online: printer & 0x08 == 0,
            drawer_pin3_high: printer & 0x04 != 0,
            cover_open: offline_cause & 0x04 != 0,
            paper_end: paper & 0x60 != 0,
            paper_near_end: paper & 0x0C != 0,
        }
    }
}

/// How a print request turned out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Printed.
    Printed,
    /// Not printed: the roll is empty.
    PaperEnd,
    /// Not printed: the cover is open.
    CoverOpen,
    /// Not printed in time: the request's timeout passed.
    Timeout,
    /// The request document could not be understood.
    BadDocument(String),
    /// No printer with the requested device ID.
    DeviceNotFound,
}

impl Outcome {
    /// The outcome of a print from the printer's status afterwards: paper end and an open
    /// cover stop a printer, so the print did not happen.
    pub fn after_print(status: &PrinterStatus) -> Self {
        if status.paper_end {
            Outcome::PaperEnd
        } else if status.cover_open {
            Outcome::CoverOpen
        } else {
            Outcome::Printed
        }
    }

    /// Whether the document printed.
    pub fn printed(&self) -> bool {
        matches!(self, Outcome::Printed)
    }
}
