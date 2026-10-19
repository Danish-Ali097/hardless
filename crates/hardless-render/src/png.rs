//! PNG output (FR-11): the SVG output rasterised with `resvg`, using only the bundled Go Mono
//! font, so a receipt renders the same on every OS and in a font-less container.

use std::fmt;
use std::sync::{Arc, OnceLock};

use hardless_escpos::Node;
use resvg::{tiny_skia, usvg};

use crate::{render_svg, RenderOptions};

/// The tallest receipt rendered to PNG, in dots (about 2.5 m of paper at 203 dpi). Longer
/// receipts would need hundreds of megabytes of pixels.
pub const MAX_PNG_HEIGHT: u32 = 20_000;

/// Why a receipt could not be rendered to PNG.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PngError(String);

impl fmt::Display for PngError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PngError {}

/// The bundled fonts, loaded once.
fn fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_font_data(include_bytes!("../fonts/Go-Mono.ttf").to_vec());
            db.load_font_data(include_bytes!("../fonts/Go-Mono-Bold.ttf").to_vec());
            db.set_monospace_family("Go Mono");
            Arc::new(db)
        })
        .clone()
}

/// Renders an AST to a PNG image, one pixel per printer dot, with the layout of
/// [`render_svg`](crate::render_svg). Needs the `png` feature.
///
/// ```
/// use hardless_escpos::parse;
/// use hardless_render::{render_png, RenderOptions};
/// let png = render_png(&parse(b"Total: 1,250\n"), &RenderOptions::default()).unwrap();
/// assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
/// ```
///
/// # Errors
///
/// Fails if the receipt is taller than [`MAX_PNG_HEIGHT`] dots.
pub fn render_png(nodes: &[Node], options: &RenderOptions) -> Result<Vec<u8>, PngError> {
    let svg = render_svg(nodes, options);
    let opt = usvg::Options {
        font_family: "Go Mono".into(),
        fontdb: fonts(),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_str(&svg, &opt).map_err(|e| PngError(e.to_string()))?;
    let size = tree.size().to_int_size();
    if size.height() > MAX_PNG_HEIGHT {
        return Err(PngError(format!(
            "the receipt is {} dots long; PNG output stops at {MAX_PNG_HEIGHT}",
            size.height()
        )));
    }
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| PngError("the receipt has no size".into()))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|e| PngError(e.to_string()))
}
