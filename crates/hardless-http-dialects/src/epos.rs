//! The ePOS-Print-compatible dialect (PRD section 9.1): `POST
//! /cgi-bin/epos/service.cgi?devid=<id>&timeout=<ms>` with a SOAP envelope holding an
//! `epos-print` document.
//!
//! Implemented from the public "ePOS-Print XML User's Manual" (element and attribute names,
//! response codes and status bits). No vendor SDK code is used.

use roxmltree::Node;

use crate::escpos::{unescape, Align, EscPos, Hri, QrLevel};
use crate::xml::{self, attr, bad, child, elements, flag, number, text};
use crate::{DialectError, Outcome, PrintRequest, PrinterStatus};

/// The `epos-print` namespace.
pub const NAMESPACE: &str = "http://www.epson-pos.com/schemas/2011/03/epos-print";
/// The SOAP 1.1 envelope namespace.
pub const SOAP_NAMESPACE: &str = "http://schemas.xmlsoap.org/soap/envelope/";

/// `status` bits of the response, from the manual's status table.
pub mod status {
    /// No response from the printer.
    pub const NO_RESPONSE: u32 = 0x0000_0001;
    /// Printing completed.
    pub const PRINT_SUCCESS: u32 = 0x0000_0002;
    /// The drawer-kick connector's pin 3 reads high.
    pub const DRAWER_KICK: u32 = 0x0000_0004;
    /// The printer is offline.
    pub const OFFLINE: u32 = 0x0000_0008;
    /// The cover is open.
    pub const COVER_OPEN: u32 = 0x0000_0020;
    /// The roll is nearly empty.
    pub const RECEIPT_NEAR_END: u32 = 0x0002_0000;
    /// The roll is empty.
    pub const RECEIPT_END: u32 = 0x0008_0000;
}

/// A parsed request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EposRequest {
    /// The document as ESC/POS.
    pub print: PrintRequest,
    /// `devid` from the SOAP header, if given there instead of the URL.
    pub devid: Option<String>,
    /// `timeout` in milliseconds from the SOAP header.
    pub timeout_ms: Option<u64>,
    /// `printjobid` from the SOAP header, echoed in the response.
    pub print_job_id: Option<String>,
}

/// Parses a request: a SOAP envelope with an `epos-print` body, or a bare `epos-print`
/// document. Elements and attribute values the manual does not define are an error, which
/// the endpoint answers as `SchemaError`.
///
/// ```
/// use hardless_http_dialects::epos;
/// let req = epos::parse(r#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body>
///   <epos-print xmlns="http://www.epson-pos.com/schemas/2011/03/epos-print">
///     <text align="center">Hello&#10;</text><cut type="feed"/>
///   </epos-print></s:Body></s:Envelope>"#).unwrap();
/// assert_eq!(req.print.escpos, b"\x1b@\x1ba\x01Hello\n\x1dVB\x00");
/// ```
pub fn parse(document: &str) -> Result<EposRequest, DialectError> {
    let doc = xml::parse(document)?;
    let root = doc.root_element();
    let mut request = EposRequest::default();
    let body = match root.tag_name().name() {
        "epos-print" => root,
        "Envelope" => {
            if let Some(parameter) = child(root, "Header").and_then(|h| child(h, "parameter")) {
                request.devid = child(parameter, "devid").map(|n| text(n).trim().to_owned());
                request.timeout_ms = child(parameter, "timeout")
                    .map(|n| text(n).trim().parse())
                    .transpose()
                    .map_err(|_| DialectError::new("<timeout> must be a number"))?;
                request.print_job_id =
                    child(parameter, "printjobid").map(|n| text(n).trim().to_owned());
            }
            child(root, "Body")
                .and_then(|b| child(b, "epos-print"))
                .ok_or_else(|| DialectError::new("the SOAP body has no <epos-print> element"))?
        }
        other => {
            return Err(DialectError::new(format!(
                "expected a SOAP envelope or <epos-print>, not <{other}>"
            )))
        }
    };
    request.print = translate(body)?;
    Ok(request)
}

/// Text settings that carry over between `text` elements in a document.
#[derive(Clone, Copy)]
struct TextState {
    width: u8,
    height: u8,
}

fn translate(epos_print: Node<'_, '_>) -> Result<PrintRequest, DialectError> {
    let mut doc = EscPos::new();
    let mut warnings = Vec::new();
    let mut state = TextState {
        width: 1,
        height: 1,
    };
    for node in elements(epos_print) {
        element(node, &mut doc, &mut state, &mut warnings)?;
    }
    Ok(PrintRequest {
        escpos: doc.finish(),
        warnings,
    })
}

fn element(
    node: Node<'_, '_>,
    doc: &mut EscPos,
    state: &mut TextState,
    warnings: &mut Vec<String>,
) -> Result<(), DialectError> {
    match node.tag_name().name() {
        "text" => text_element(node, doc, state, warnings),
        "feed" => {
            if let Some(lines) = number::<u8>(node, "line")? {
                doc.feed_lines(lines);
            } else if let Some(dots) = number::<u32>(node, "unit")? {
                // ESC/POS line feeds are 30 dots at the default line spacing.
                doc.feed_lines(u8::try_from(dots.div_ceil(30)).unwrap_or(u8::MAX));
            } else if attr(node, "pos").is_none() {
                doc.line_feed();
            }
            Ok(())
        }
        "image" => image(node, doc),
        "logo" => {
            let k1 = number::<u8>(node, "key1")?.unwrap_or(32);
            let k2 = number::<u8>(node, "key2")?.unwrap_or(32);
            warnings.push(format!("NV logo {k1}/{k2}: the emulator stores no logos"));
            // GS ( L function 69 (print NV graphics); the printer reports it as unknown.
            doc.raw(&[0x1D, 0x28, 0x4C, 0x06, 0x00, 0x30, 0x45, k1, k2, 0x01, 0x01]);
            Ok(())
        }
        "barcode" => barcode(node, doc, warnings),
        "symbol" => symbol(node, doc),
        "cut" => {
            match attr(node, "type").unwrap_or("feed") {
                "no_feed" => doc.cut(true, false),
                "feed" | "reserve" => doc.cut(true, true),
                other => return Err(bad(node, "type", other)),
            }
            Ok(())
        }
        "pulse" => {
            let pin5 = match attr(node, "drawer").unwrap_or("drawer_1") {
                "drawer_1" => false,
                "drawer_2" => true,
                other => return Err(bad(node, "drawer", other)),
            };
            let ms = match attr(node, "time").unwrap_or("pulse_100") {
                "pulse_100" => 100,
                "pulse_200" => 200,
                "pulse_300" => 300,
                "pulse_400" => 400,
                "pulse_500" => 500,
                other => return Err(bad(node, "time", other)),
            };
            doc.pulse(pin5, ms, ms);
            Ok(())
        }
        "command" => {
            let hex: String = text(node).chars().filter(|c| !c.is_whitespace()).collect();
            let bytes = decode_hex(&hex)
                .ok_or_else(|| DialectError::new("<command> must be hexadecimal bytes"))?;
            doc.raw(&bytes);
            Ok(())
        }
        "sound" => {
            warnings.push("<sound>: the emulated printer has no buzzer".into());
            Ok(())
        }
        "page" => {
            warnings.push("<page>: page mode is printed in standard mode".into());
            for inner in elements(node) {
                match inner.tag_name().name() {
                    "area" | "direction" | "position" | "line" | "rectangle" => {}
                    _ => element(inner, doc, state, warnings)?,
                }
            }
            Ok(())
        }
        "layout" | "hline" | "vline" | "recovery" | "reset" => {
            warnings.push(format!(
                "<{}> is not reproduced by the emulator",
                node.tag_name().name()
            ));
            Ok(())
        }
        other => Err(DialectError::new(format!(
            "<{other}> is not an epos-print element"
        ))),
    }
}

fn text_element(
    node: Node<'_, '_>,
    doc: &mut EscPos,
    state: &mut TextState,
    warnings: &mut Vec<String>,
) -> Result<(), DialectError> {
    if let Some(align) = attr(node, "align") {
        doc.align(align_value(node, align)?);
    }
    if let Some(font) = attr(node, "font") {
        match font {
            "font_a" => doc.font_b(false),
            "font_b" | "font_c" | "font_d" | "font_e" => doc.font_b(true),
            other => return Err(bad(node, "font", other)),
        }
    }
    if let Some(em) = flag(node, "em")? {
        doc.bold(em);
    }
    if let Some(ul) = flag(node, "ul")? {
        doc.underline(u8::from(ul));
    }
    let mut size = *state;
    if let Some(dw) = flag(node, "dw")? {
        size.width = if dw { 2 } else { 1 };
    }
    if let Some(dh) = flag(node, "dh")? {
        size.height = if dh { 2 } else { 1 };
    }
    if let Some(w) = number::<u8>(node, "width")? {
        size.width = w.clamp(1, 8);
    }
    if let Some(h) = number::<u8>(node, "height")? {
        size.height = h.clamp(1, 8);
    }
    if (size.width, size.height) != (state.width, state.height) {
        doc.size(size.width, size.height);
        *state = size;
    }
    for name in ["reverse", "rotate", "x", "linespc"] {
        if attr(node, name).is_some() {
            warnings.push(format!("<text {name}>: not reproduced by the emulator"));
        }
    }
    if let Some(lang) = attr(node, "lang") {
        if !matches!(lang, "en" | "de" | "fr" | "it" | "es" | "multi") {
            warnings.push(format!(
                "<text lang=\"{lang}\">: only Latin code pages are emulated"
            ));
        }
    }
    doc.text(&text(node));
    Ok(())
}

fn align_value(node: Node<'_, '_>, value: &str) -> Result<Align, DialectError> {
    match value {
        "left" => Ok(Align::Left),
        "center" => Ok(Align::Center),
        "right" => Ok(Align::Right),
        other => Err(bad(node, "align", other)),
    }
}

fn image(node: Node<'_, '_>, doc: &mut EscPos) -> Result<(), DialectError> {
    let width =
        number::<u32>(node, "width")?.ok_or_else(|| DialectError::new("<image> needs a width"))?;
    let height = number::<u32>(node, "height")?
        .ok_or_else(|| DialectError::new("<image> needs a height"))?;
    if let Some(align) = attr(node, "align") {
        doc.align(align_value(node, align)?);
    }
    let encoded: String = text(node).chars().filter(|c| !c.is_whitespace()).collect();
    let data = hardless_core_base64(&encoded)?;
    let data = match attr(node, "mode").unwrap_or("mono") {
        "mono" => data,
        // 16-level grayscale: 4 bits a pixel. The emulated printer prints one bit, dark from
        // level 8.
        "gray16" => gray16_to_mono(&data, width, height),
        other => return Err(bad(node, "mode", other)),
    };
    doc.raster(width, height, &data);
    Ok(())
}

fn gray16_to_mono(data: &[u8], width: u32, height: u32) -> Vec<u8> {
    let row_bytes = width.div_ceil(8) as usize;
    let mut out = vec![0u8; row_bytes * height as usize];
    let src_row = width.div_ceil(2) as usize;
    for y in 0..height as usize {
        for x in 0..width as usize {
            let Some(&byte) = data.get(y * src_row + x / 2) else {
                continue;
            };
            let level = if x % 2 == 0 { byte >> 4 } else { byte & 0x0F };
            if level >= 8 {
                out[y * row_bytes + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    out
}

fn barcode(
    node: Node<'_, '_>,
    doc: &mut EscPos,
    warnings: &mut Vec<String>,
) -> Result<(), DialectError> {
    let kind = attr(node, "type").ok_or_else(|| DialectError::new("<barcode> needs a type"))?;
    // GS k function B symbol numbers.
    let m = match kind {
        "upc_a" => 65,
        "upc_e" => 66,
        "ean13" | "jan13" => 67,
        "ean8" | "jan8" => 68,
        "code39" => 69,
        "itf" => 70,
        "codabar" => 71,
        "code93" => 72,
        "code128" | "code128_auto" => 73,
        "gs1_128" => 74,
        "gs1_databar_omnidirectional" => 75,
        "gs1_databar_truncated" => 76,
        "gs1_databar_limited" => 77,
        "gs1_databar_expanded" => 78,
        other => return Err(bad(node, "type", other)),
    };
    let hri = match attr(node, "hri").unwrap_or("none") {
        "none" => Hri::None,
        "above" => Hri::Above,
        "below" => Hri::Below,
        "both" => Hri::Both,
        other => return Err(bad(node, "hri", other)),
    };
    if let Some(align) = attr(node, "align") {
        doc.align(align_value(node, align)?);
    }
    if attr(node, "rotate").is_some() {
        warnings.push("<barcode rotate>: not reproduced by the emulator".into());
    }
    let module = number::<u8>(node, "width")?.unwrap_or(3);
    let height = number::<u8>(node, "height")?.unwrap_or(162);
    let mut data = unescape(&text(node));
    if m == 73 && !data.starts_with(b"{") {
        // ESC/POS CODE128 data names its code set; plain text is code set B.
        data.splice(0..0, *b"{B");
    }
    doc.barcode(m, &data, hri, module, height);
    Ok(())
}

fn symbol(node: Node<'_, '_>, doc: &mut EscPos) -> Result<(), DialectError> {
    let kind = attr(node, "type").ok_or_else(|| DialectError::new("<symbol> needs a type"))?;
    let data = unescape(&text(node));
    let module = number::<u8>(node, "width")?.unwrap_or(3);
    let qr = |model: u8, doc: &mut EscPos| -> Result<(), DialectError> {
        let level = match attr(node, "level").unwrap_or("level_m") {
            "level_l" => QrLevel::L,
            "level_m" | "default" => QrLevel::M,
            "level_q" => QrLevel::Q,
            "level_h" => QrLevel::H,
            other => return Err(bad(node, "level", other)),
        };
        doc.qr(model, module, level, &data);
        Ok(())
    };
    match kind {
        "qrcode_model_1" => qr(1, doc),
        "qrcode_model_2" => qr(2, doc),
        // Symbols the emulated printer does not draw; `GS ( k` with their own code.
        k if k.starts_with("pdf417") => {
            doc.other_symbol(48, &data);
            Ok(())
        }
        k if k.starts_with("maxicode") => {
            doc.other_symbol(50, &data);
            Ok(())
        }
        k if k.starts_with("gs1_databar") => {
            doc.other_symbol(51, &data);
            Ok(())
        }
        k if k.starts_with("azteccode") => {
            doc.other_symbol(53, &data);
            Ok(())
        }
        k if k.starts_with("datamatrix") || k == "qrcode_micro" => {
            doc.other_symbol(54, &data);
            Ok(())
        }
        other => Err(bad(node, "type", other)),
    }
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect()
}

fn hardless_core_base64(text: &str) -> Result<Vec<u8>, DialectError> {
    hardless_core::base64::decode(text)
        .map_err(|e| DialectError::new(format!("<image> data is not base64: {e}")))
}

/// The `status` bitmask for a print's outcome and the printer's state afterwards.
///
/// ```
/// use hardless_http_dialects::{epos, Outcome, PrinterStatus};
/// let ok = PrinterStatus { online: true, ..Default::default() };
/// assert_eq!(epos::status_bits(&Outcome::Printed, &ok), epos::status::PRINT_SUCCESS);
/// ```
pub fn status_bits(outcome: &Outcome, printer: &PrinterStatus) -> u32 {
    if matches!(outcome, Outcome::BadDocument(_) | Outcome::DeviceNotFound) {
        return 0;
    }
    let mut bits = 0;
    if outcome.printed() {
        bits |= status::PRINT_SUCCESS;
    }
    if printer.drawer_pin3_high {
        bits |= status::DRAWER_KICK;
    }
    if !printer.online {
        bits |= status::OFFLINE;
    }
    if printer.cover_open {
        bits |= status::COVER_OPEN;
    }
    if printer.paper_near_end {
        bits |= status::RECEIPT_NEAR_END;
    }
    if printer.paper_end {
        bits |= status::RECEIPT_END;
    }
    bits
}

/// The response `code` for an outcome: empty on success, else the manual's error code.
pub fn error_code(outcome: &Outcome) -> &'static str {
    match outcome {
        Outcome::Printed => "",
        Outcome::PaperEnd => "EPTR_REC_EMPTY",
        Outcome::CoverOpen => "EPTR_COVER_OPEN",
        Outcome::Timeout => "EX_TIMEOUT",
        Outcome::BadDocument(_) => "SchemaError",
        Outcome::DeviceNotFound => "DeviceNotFound",
    }
}

/// The SOAP response document.
///
/// ```
/// use hardless_http_dialects::{epos, Outcome, PrinterStatus};
/// let paper_out = PrinterStatus { paper_end: true, paper_near_end: true, ..Default::default() };
/// let xml = epos::response(&Outcome::PaperEnd, &paper_out, None);
/// assert!(xml.contains(r#"success="false" code="EPTR_REC_EMPTY" status="655368""#));
/// ```
pub fn response(outcome: &Outcome, printer: &PrinterStatus, print_job_id: Option<&str>) -> String {
    let header = print_job_id.map_or(String::new(), |id| {
        format!(
            r#"<s:Header><parameter xmlns="{NAMESPACE}"><printjobid>{}</printjobid></parameter></s:Header>"#,
            xml::escape(id)
        )
    });
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?><s:Envelope xmlns:s="{SOAP_NAMESPACE}">{header}<s:Body><response success="{}" code="{}" status="{}" battery="0" xmlns="{NAMESPACE}"/></s:Body></s:Envelope>"#,
        outcome.printed(),
        error_code(outcome),
        status_bits(outcome, printer)
    )
}
