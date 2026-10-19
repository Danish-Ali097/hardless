#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod codes;
mod html;
mod layout;
#[cfg(feature = "png")]
mod png;
mod svg;
mod text;

use serde::{Deserialize, Serialize};

pub use html::{render_html, render_html_document, STYLESHEET};
#[cfg(feature = "png")]
pub use png::{render_png, PngError, MAX_PNG_HEIGHT};
pub use svg::render_svg;
pub use text::{to_text, to_text_with};

/// Licence of the bundled Go Mono font (BSD-3-Clause), which the PNG output embeds. Ship it
/// with any binary built with the `png` feature.
pub const FONT_LICENSE: &str = include_str!("../fonts/LICENSE");

/// Paper roll width.
///
/// ```
/// use hardless_render::PaperWidth;
/// assert_eq!(PaperWidth::Mm80.columns(), (48, 64));
/// assert_eq!(serde_json::to_string(&PaperWidth::Mm58).unwrap(), "58");
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub enum PaperWidth {
    /// 58 mm: 384 dots, 32 columns in Font A, 42 in Font B.
    Mm58,
    /// 80 mm: 576 dots, 48 columns in Font A, 64 in Font B (the default).
    #[default]
    Mm80,
}

impl PaperWidth {
    /// Columns per line in Font A and Font B.
    pub fn columns(self) -> (u32, u32) {
        match self {
            PaperWidth::Mm58 => (32, 42),
            PaperWidth::Mm80 => (48, 64),
        }
    }

    /// The width in millimetres.
    pub fn millimetres(self) -> u16 {
        match self {
            PaperWidth::Mm58 => 58,
            PaperWidth::Mm80 => 80,
        }
    }

    /// The printable width in dots: 576 for 80 mm, 384 for 58 mm (12 dots per Font A column).
    pub fn dots(self) -> u32 {
        self.columns().0 * layout::DOTS_PER_COLUMN
    }
}

impl TryFrom<u16> for PaperWidth {
    type Error = String;

    fn try_from(mm: u16) -> Result<Self, String> {
        match mm {
            58 => Ok(PaperWidth::Mm58),
            80 => Ok(PaperWidth::Mm80),
            other => Err(format!("paper width must be 58 or 80, not {other}")),
        }
    }
}

impl From<PaperWidth> for u16 {
    fn from(paper: PaperWidth) -> u16 {
        paper.millimetres()
    }
}

/// Rendering options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderOptions {
    /// Paper width (default 80 mm).
    pub paper: PaperWidth,
    /// Show unknown commands as small badges in HTML (default on; FR-12).
    pub show_unknown: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            paper: PaperWidth::Mm80,
            show_unknown: true,
        }
    }
}
