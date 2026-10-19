//! The WebPRNT-compatible dialect (PRD section 9.1): `POST /StarWebPRNT/SendMessage` (or
//! `/p/<deviceId>/StarWebPRNT/SendMessage`) with a `<root>` request document.
//!
//! Implemented from the public "Star webPRNT User's Manual" (request and response elements)
//! and the public "ASB (Automatic Status Back) Function Specifications Manual" (status bytes).
//! No vendor SDK code is used.

use roxmltree::Node;

use crate::escpos::{unescape, Align, EscPos, Hri, QrLevel};
use crate::xml::{self, attr, bad, elements, flag, number, text};
use crate::{DialectError, Outcome, PrintRequest, PrinterStatus};

/// Response `code` values from the manual.
pub mod code {
    /// Success.
    pub const SUCCESS: &str = "0";
    /// The printer is offline or in an error state.
    pub const OFFLINE: &str = "1100";
    /// The printer is busy; send the data again.
    pub const BUSY: &str = "2001";
}

/// Parses a request document. Elements and attribute values the manual does not define are
/// an error.
///
/// ```
/// use hardless_http_dialects::webprnt;
/// let req = webprnt::parse(r#"<root><alignment position="center"/>
///   <text emphasis="true">Hello\x0a</text><cutpaper type="partial" feed="true"/></root>"#).unwrap();
/// assert_eq!(req.escpos, b"\x1b@\x1ba\x01\x1bE\x01Hello\n\x1dVB\x00");
/// ```
pub fn parse(document: &str) -> Result<PrintRequest, DialectError> {
    let doc = xml::parse(document)?;
    let root = doc.root_element();
    if root.tag_name().name() != "root" {
        return Err(DialectError::new(format!(
            "expected <root>, not <{}>",
            root.tag_name().name()
        )));
    }
    let mut out = EscPos::new();
    let mut warnings = Vec::new();
    let mut size = (1u8, 1u8);
    for node in elements(root) {
        element(node, &mut out, &mut size, &mut warnings)?;
    }
    Ok(PrintRequest {
        escpos: out.finish(),
        warnings,
    })
}

fn element(
    node: Node<'_, '_>,
    doc: &mut EscPos,
    size: &mut (u8, u8),
    warnings: &mut Vec<String>,
) -> Result<(), DialectError> {
    match node.tag_name().name() {
        "initialization" => {
            if flag(node, "reset")? == Some(true) {
                doc.raw(&[0x1B, 0x40]);
                *size = (1, 1);
            }
            Ok(())
        }
        "text" => text_element(node, doc, size, warnings),
        "alignment" => {
            // The manual's attribute table spells it "poistion"; accept both.
            let position = attr(node, "position")
                .or_else(|| attr(node, "poistion"))
                .unwrap_or("left");
            doc.align(match position {
                "left" => Align::Left,
                "center" => Align::Center,
                "right" => Align::Right,
                other => return Err(bad(node, "position", other)),
            });
            Ok(())
        }
        "feed" => {
            if let Some(lines) = number::<u8>(node, "line")?.filter(|&n| n > 0) {
                doc.feed_lines(lines);
            } else if let Some(dots) = number::<u32>(node, "unit")?.filter(|&n| n > 0) {
                // 24 dots is one 3 mm line.
                doc.feed_lines(u8::try_from(dots.div_ceil(24)).unwrap_or(u8::MAX));
            }
            Ok(())
        }
        "cutpaper" => {
            let partial = match attr(node, "type").unwrap_or("full") {
                "full" => false,
                "partial" => true,
                other => return Err(bad(node, "type", other)),
            };
            doc.cut(partial, flag(node, "feed")?.unwrap_or(true));
            Ok(())
        }
        "barcode" => barcode(node, doc),
        "qrcode" => {
            let model = match attr(node, "model").unwrap_or("model2") {
                "model1" => 1,
                "model2" => 2,
                other => return Err(bad(node, "model", other)),
            };
            let level = match attr(node, "level").unwrap_or("level_l") {
                "level_l" => QrLevel::L,
                "level_m" => QrLevel::M,
                "level_q" => QrLevel::Q,
                "level_h" => QrLevel::H,
                other => return Err(bad(node, "level", other)),
            };
            let cell = number::<u8>(node, "cell")?.unwrap_or(3);
            doc.qr(model, cell, level, &unescape(&text(node)));
            Ok(())
        }
        "pdf417" => {
            doc.other_symbol(48, &unescape(&text(node)));
            Ok(())
        }
        "bitimage" | "bitImage" => {
            let width = number::<u32>(node, "width")?.unwrap_or(0);
            let height = number::<u32>(node, "height")?.unwrap_or(0).min(2400);
            let encoded: String = text(node).chars().filter(|c| !c.is_whitespace()).collect();
            let data = hardless_core::base64::decode(&encoded)
                .map_err(|e| DialectError::new(format!("<bitimage> data is not base64: {e}")))?;
            if width > 0 && height > 0 {
                doc.raster(width, height, &data);
            }
            Ok(())
        }
        "peripheral" => {
            let pin5 = match number::<u8>(node, "channel")?.unwrap_or(1) {
                1 => false,
                2 => true,
                other => return Err(bad(node, "channel", &other.to_string())),
            };
            let on = number::<u32>(node, "on")?.unwrap_or(200);
            let off = number::<u32>(node, "off")?.unwrap_or(200);
            doc.pulse(pin5, on, off);
            Ok(())
        }
        "rawdata" => {
            let encoded: String = text(node).chars().filter(|c| !c.is_whitespace()).collect();
            let data = hardless_core::base64::decode(&encoded)
                .map_err(|e| DialectError::new(format!("<rawdata> is not base64: {e}")))?;
            doc.raw(&data);
            Ok(())
        }
        "logo" => {
            let number = number::<u8>(node, "number")?.unwrap_or(1);
            warnings.push(format!("logo {number}: the emulator stores no logos"));
            // GS ( L function 69 (print NV graphics); the printer reports it as unknown.
            doc.raw(&[
                0x1D, 0x28, 0x4C, 0x06, 0x00, 0x30, 0x45, 0x30, number, 0x01, 0x01,
            ]);
            Ok(())
        }
        "sound" | "sound_with_setting" => {
            warnings.push(format!(
                "<{}>: the emulated printer has no buzzer",
                node.tag_name().name()
            ));
            Ok(())
        }
        "ruledline" | "holdprint" => {
            warnings.push(format!(
                "<{}> is not reproduced by the emulator",
                node.tag_name().name()
            ));
            Ok(())
        }
        other => Err(DialectError::new(format!(
            "<{other}> is not a webPRNT request element"
        ))),
    }
}

fn text_element(
    node: Node<'_, '_>,
    doc: &mut EscPos,
    size: &mut (u8, u8),
    warnings: &mut Vec<String>,
) -> Result<(), DialectError> {
    if let Some(font) = attr(node, "font") {
        match font {
            "font_a" => doc.font_b(false),
            "font_b" => doc.font_b(true),
            other => return Err(bad(node, "font", other)),
        }
    }
    if let Some(on) = flag(node, "emphasis")? {
        doc.bold(on);
    }
    if let Some(on) = flag(node, "underline")? {
        doc.underline(u8::from(on));
    }
    let width = number::<u8>(node, "width")?.map_or(size.0, |w| w.clamp(1, 6));
    let height = number::<u8>(node, "height")?.map_or(size.1, |h| h.clamp(1, 6));
    if (width, height) != *size {
        doc.size(width, height);
        *size = (width, height);
    }
    for name in ["invert", "characterspace", "linespace"] {
        if attr(node, name).is_some() {
            warnings.push(format!("<text {name}>: not reproduced by the emulator"));
        }
    }
    // Control characters arrive escaped as `\xNN`; text is UTF-8 and is printed in the code
    // page that has its characters, so `codepage` and `international` need no translation.
    let content = unescape(&text(node));
    doc.text(&String::from_utf8_lossy(&content));
    Ok(())
}

fn barcode(node: Node<'_, '_>, doc: &mut EscPos) -> Result<(), DialectError> {
    // GS k function B symbol numbers.
    let m = match attr(node, "symbology").unwrap_or("UPCE") {
        "UPCA" => 65,
        "UPCE" => 66,
        "JAN13" => 67,
        "JAN8" => 68,
        "Code39" => 69,
        "ITF" => 70,
        "NW7" => 71,
        "Code93" => 72,
        "Code128" => 73,
        other => return Err(bad(node, "symbology", other)),
    };
    let module = match attr(node, "width").unwrap_or("width3") {
        "width2" => 2,
        "width3" => 3,
        "width4" => 4,
        mode => match mode
            .strip_prefix("width_mode")
            .and_then(|n| n.parse::<u8>().ok())
        {
            Some(n @ 1..=9) => 2 + (n - 1) / 2,
            _ => return Err(bad(node, "width", mode)),
        },
    };
    let height = number::<u8>(node, "height")?.unwrap_or(162);
    let hri = if flag(node, "hri")?.unwrap_or(true) {
        Hri::Below
    } else {
        Hri::None
    };
    let mut data = unescape(&text(node));
    if m == 73 && !data.starts_with(b"{") {
        // ESC/POS CODE128 data names its code set; plain text is code set B.
        data.splice(0..0, *b"{B");
    }
    doc.barcode(m, &data, hri, module, height);
    Ok(())
}

/// The Star Automatic Status Back bytes for the response `status`: two header bytes, then
/// printer status bytes as in the ASB manual (status 1: drawer switch bit 2, offline bit 3,
/// cover bit 5; status 4: paper near end bit 2, paper end bit 3).
pub fn asb(printer: &PrinterStatus) -> [u8; 11] {
    let mut bytes = [0u8; 11];
    bytes[0] = 0x23;
    bytes[1] = 0x86;
    if printer.drawer_pin3_high {
        bytes[2] |= 0x04;
    }
    if !printer.online {
        bytes[2] |= 0x08;
    }
    if printer.cover_open {
        bytes[2] |= 0x20;
    }
    if printer.paper_near_end {
        bytes[5] |= 0x04;
    }
    if printer.paper_end {
        bytes[5] |= 0x08;
    }
    bytes
}

/// The response document.
///
/// ```
/// use hardless_http_dialects::{webprnt, Outcome, PrinterStatus};
/// let ok = PrinterStatus { online: true, ..Default::default() };
/// assert_eq!(
///     webprnt::response(&Outcome::Printed, &ok),
///     r#"<?xml version="1.0" encoding="UTF-8"?><root><success>true</success><code>0</code><status>2386000000000000000000</status></root>"#
/// );
/// ```
pub fn response(outcome: &Outcome, printer: &PrinterStatus) -> String {
    let code = match outcome {
        Outcome::Printed => code::SUCCESS,
        Outcome::Timeout => code::BUSY,
        _ => code::OFFLINE,
    };
    let status: String = asb(printer).iter().map(|b| format!("{b:02X}")).collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><root><success>{}</success><code>{code}</code><status>{status}</status></root>"#,
        outcome.printed()
    )
}
