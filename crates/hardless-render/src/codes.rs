//! QR codes, barcodes and raster images as inline SVG (FR-12).
//!
//! Sizes are in printer dots; the HTML scales them so 12 dots equal one Font A column.

use barcoders::sym::{code128::Code128, code39::Code39, ean13::EAN13};
use hardless_escpos::{QrErrorCorrection, Symbology};
use qrcode::{Color, EcLevel, QrCode};

/// An SVG drawing and its size in dots.
pub(crate) struct Svg {
    pub width: u32,
    pub height: u32,
    pub body: String,
}

/// Quiet zone around a QR code, in modules (the printer adds it too).
const QR_QUIET_MODULES: u32 = 2;

/// Draws a QR code, or explains why it cannot be drawn.
pub(crate) fn qr(data: &[u8], module_dots: u8, level: QrErrorCorrection) -> Result<Svg, String> {
    let level = match level {
        QrErrorCorrection::L => EcLevel::L,
        QrErrorCorrection::M => EcLevel::M,
        QrErrorCorrection::Q => EcLevel::Q,
        QrErrorCorrection::H => EcLevel::H,
    };
    let code = QrCode::with_error_correction_level(data, level)
        .map_err(|e| format!("cannot encode QR code: {e}"))?;
    let n = u32::try_from(code.width()).map_err(|_| "QR code too large".to_owned())?;
    let colors = code.to_colors();
    let mut path = String::new();
    for (i, color) in colors.iter().enumerate() {
        if *color == Color::Dark {
            let i = u32::try_from(i).unwrap_or(0);
            let (x, y) = (i % n + QR_QUIET_MODULES, i / n + QR_QUIET_MODULES);
            path.push_str(&format!("M{x} {y}h1v1h-1z"));
        }
    }
    let modules = n + 2 * QR_QUIET_MODULES;
    let size = modules * u32::from(module_dots.max(1));
    Ok(Svg {
        width: size,
        height: size,
        body: format!(
            r#"<svg class="hl-qr" viewBox="0 0 {modules} {modules}" shape-rendering="crispEdges" role="img" aria-label="QR code"><path d="{path}"/></svg>"#
        ),
    })
}

/// The human-readable text of a barcode (what the HRI line shows).
pub(crate) fn barcode_text(symbology: Symbology, data: &str) -> String {
    match symbology {
        Symbology::Code128 => code128_input(data)
            .map(|s| {
                s.chars()
                    .filter(|c| !matches!(c, 'À' | 'Ɓ' | 'Ć'))
                    .collect()
            })
            .unwrap_or_else(|_| data.to_owned()),
        Symbology::Ean13 | Symbology::Code39 => data.to_owned(),
    }
}

/// Converts ESC/POS CODE128 data (`{A`/`{B`/`{C` code set selectors, set C as byte values
/// 0..=99, `{{` for a literal brace) into the form `barcoders` expects.
fn code128_input(data: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut set_c = false;
    let mut chars = data.chars();
    while let Some(c) = chars.next() {
        if c == '{' {
            match chars.next() {
                Some('A') => {
                    out.push('À');
                    set_c = false;
                }
                Some('B') => {
                    out.push('Ɓ');
                    set_c = false;
                }
                Some('C') => {
                    out.push('Ć');
                    set_c = true;
                }
                Some('{') => out.push('{'),
                Some(other) => return Err(format!("CODE128 function {{{other} is not supported")),
                None => return Err("CODE128 data ends with {".into()),
            }
        } else if set_c {
            let value = u32::from(c);
            if value > 99 {
                return Err(format!("CODE128 set C value {value} is above 99"));
            }
            out.push_str(&format!("{value:02}"));
        } else {
            out.push(c);
        }
    }
    if !out.starts_with(['À', 'Ɓ', 'Ć']) {
        return Err("CODE128 data must start with {A, {B or {C".into());
    }
    Ok(out)
}

/// Draws a barcode, or explains why it cannot be drawn.
pub(crate) fn barcode(
    symbology: Symbology,
    data: &str,
    module_dots: u8,
    height_dots: u8,
) -> Result<Svg, String> {
    let bars = match symbology {
        Symbology::Code128 => Code128::new(code128_input(data)?)
            .map(|c| c.encode())
            .map_err(|e| e.to_string())?,
        Symbology::Ean13 => EAN13::new(data)
            .map(|c| c.encode())
            .map_err(|e| e.to_string())?,
        Symbology::Code39 => Code39::new(data)
            .map(|c| c.encode())
            .map_err(|e| e.to_string())?,
    };
    let modules = u32::try_from(bars.len()).map_err(|_| "barcode too long".to_owned())?;
    let mut path = String::new();
    let mut x = 0u32;
    while (x as usize) < bars.len() {
        if bars[x as usize] == 1 {
            let start = x;
            while (x as usize) < bars.len() && bars[x as usize] == 1 {
                x += 1;
            }
            path.push_str(&format!("M{start} 0h{}v1h-{}z", x - start, x - start));
        } else {
            x += 1;
        }
    }
    Ok(Svg {
        width: modules * u32::from(module_dots.max(1)),
        height: u32::from(height_dots.max(1)),
        body: format!(
            r#"<svg class="hl-barcode" viewBox="0 0 {modules} 1" preserveAspectRatio="none" shape-rendering="crispEdges" role="img" aria-label="barcode"><path d="{path}"/></svg>"#
        ),
    })
}

/// Draws a 1-bit raster image (MSB first, `width_bytes` bytes per row).
pub(crate) fn image(
    width_bytes: u16,
    height: u16,
    double_width: bool,
    double_height: bool,
    data: &[u8],
) -> Svg {
    let w = u32::from(width_bytes) * 8;
    let h = u32::from(height);
    let mut path = String::new();
    for y in 0..h {
        let mut x = 0u32;
        while x < w {
            if pixel(data, width_bytes, x, y) {
                let start = x;
                while x < w && pixel(data, width_bytes, x, y) {
                    x += 1;
                }
                path.push_str(&format!("M{start} {y}h{}v1h-{}z", x - start, x - start));
            } else {
                x += 1;
            }
        }
    }
    Svg {
        width: w * if double_width { 2 } else { 1 },
        height: h * if double_height { 2 } else { 1 },
        body: format!(
            r#"<svg class="hl-image" viewBox="0 0 {w} {h}" preserveAspectRatio="none" shape-rendering="crispEdges" role="img" aria-label="image"><path d="{path}"/></svg>"#
        ),
    }
}

fn pixel(data: &[u8], width_bytes: u16, x: u32, y: u32) -> bool {
    let index = y as usize * usize::from(width_bytes) + (x / 8) as usize;
    data.get(index)
        .is_some_and(|byte| byte & (0x80 >> (x % 8)) != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code128_sets_and_escapes() {
        assert_eq!(code128_input("{BABC").unwrap(), "ƁABC");
        assert_eq!(code128_input("{C\u{0c}\u{22}").unwrap(), "Ć1234");
        assert_eq!(code128_input("{B{{x").unwrap(), "Ɓ{x");
        assert!(code128_input("ABC").is_err());
        assert!(code128_input("{C\u{7f}").is_err());
    }

    #[test]
    fn image_runs() {
        let svg = image(1, 1, false, false, &[0b1100_0011]);
        assert!(svg.body.contains("M0 0h2v1h-2z"));
        assert!(svg.body.contains("M6 0h2v1h-2z"));
        assert_eq!((svg.width, svg.height), (8, 1));
    }
}
