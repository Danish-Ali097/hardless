//! FR-11: SVG and PNG output with the same layout as the HTML output.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use hardless_escpos::parse;
use hardless_render::{render_svg, PaperWidth, RenderOptions};

const MM58: RenderOptions = RenderOptions {
    paper: PaperWidth::Mm58,
    show_unknown: true,
};

/// The `x` of the first `<text>` run, from its `translate(x y)`.
fn first_text_x(svg: &str) -> f64 {
    let start = svg.find("translate(").unwrap() + "translate(".len();
    let rest = &svg[start..];
    rest[..rest.find(' ').unwrap()].parse().unwrap()
}

#[test]
fn fr_11_svg_is_one_unit_per_printer_dot() {
    assert_eq!(PaperWidth::Mm80.dots(), 576);
    assert_eq!(PaperWidth::Mm58.dots(), 384);
    let svg = render_svg(&parse(b"x\n"), &RenderOptions::default());
    assert!(svg.contains(r#"width="612""#), "{svg}");
    let svg = render_svg(&parse(b"x\n"), &MM58);
    assert!(svg.contains(r#"width="420""#), "{svg}");
}

#[test]
fn fr_11_svg_grows_one_line_pitch_per_printed_line() {
    let height = |data: &[u8]| -> u32 {
        let svg = render_svg(&parse(data), &RenderOptions::default());
        let start = svg.find(r#"height=""#).unwrap() + 8;
        svg[start..start + svg[start..].find('"').unwrap()]
            .parse()
            .unwrap()
    };
    assert_eq!(height(b"one\ntwo\n") - height(b"one\n"), 30);
    assert_eq!(
        height(b"\x1d!\x01tall\n") - height(b"one\n"),
        30,
        "double height doubles the line"
    );
}

#[test]
fn fr_11_svg_aligns_like_the_html_output() {
    let left = first_text_x(&render_svg(&parse(b"AB\n"), &RenderOptions::default()));
    let center = first_text_x(&render_svg(
        &parse(b"\x1ba\x01AB\n"),
        &RenderOptions::default(),
    ));
    let right = first_text_x(&render_svg(
        &parse(b"\x1ba\x02AB\n"),
        &RenderOptions::default(),
    ));
    assert_eq!(left, 18.0, "left margin");
    assert_eq!(center, 18.0 + (576.0 - 24.0) / 2.0);
    assert_eq!(right, 18.0 + 576.0 - 24.0);
}

#[test]
fn fr_11_svg_text_runs_fill_their_columns_exactly() {
    let svg = render_svg(
        &parse(b"\x1bE\x01\x1b-\x01Bold\n"),
        &RenderOptions::default(),
    );
    assert!(
        svg.contains(r#"textLength="48.00""#),
        "4 columns of 12 dots: {svg}"
    );
    assert!(svg.contains(r#"font-weight="bold""#), "{svg}");
    assert!(svg.contains(r#"height="1"/>"#), "1-dot underline: {svg}");
    let wide = render_svg(&parse(b"\x1d!\x10W\n"), &RenderOptions::default());
    assert!(wide.contains("scale(2 1)"), "{wide}");
}

#[test]
fn fr_11_svg_draws_cuts_codes_and_images() {
    let mut data = b"\x1d(k\x03\x001P0hi\x1d(k\x03\x001Q0".to_vec(); // QR "hi"
    data.extend(b"\x1dH\x02\x1dkE\x03ABC"); // CODE39 with HRI below
    data.extend(b"\x1dv0\x00\x01\x00\x01\x00\xff"); // 8x1 image
    data.extend(b"\x1dV\x01"); // partial cut
    let svg = render_svg(&parse(&data), &RenderOptions::default());
    for part in [
        r#"class="hl-qr""#,
        r#"class="hl-barcode""#,
        r#"class="hl-image""#,
        r#"stroke-dasharray="2 4""#,
        r#"text-anchor="middle">ABC</text>"#,
    ] {
        assert!(svg.contains(part), "missing {part}: {svg}");
    }
}

#[test]
fn fr_11_svg_escapes_receipt_text() {
    let svg = render_svg(&parse(b"<b>&\"\n"), &RenderOptions::default());
    assert!(svg.contains("&lt;b&gt;&amp;&quot;"), "{svg}");
    assert!(!svg.contains("<b>"), "{svg}");
}

#[test]
fn fr_12_svg_unknown_badges_can_be_turned_off() {
    let ast = parse(b"\x1b3\x18x\n");
    assert!(render_svg(&ast, &RenderOptions::default()).contains("#fde68a"));
    let off = RenderOptions {
        show_unknown: false,
        ..Default::default()
    };
    assert!(!render_svg(&ast, &off).contains("#fde68a"));
}

#[cfg(feature = "png")]
mod png {
    use super::*;
    use hardless_render::{render_png, MAX_PNG_HEIGHT};
    use resvg::tiny_skia::Pixmap;

    /// Dark pixels in a decoded PNG.
    fn ink(png: &[u8]) -> usize {
        let pixmap = Pixmap::decode_png(png).unwrap();
        pixmap
            .pixels()
            .iter()
            .filter(|p| u32::from(p.red()) + u32::from(p.green()) + u32::from(p.blue()) < 300)
            .count()
    }

    fn png(data: &[u8]) -> Vec<u8> {
        render_png(&parse(data), &RenderOptions::default()).unwrap()
    }

    #[test]
    fn fr_11_png_has_the_svg_size() {
        let png = png(b"one\ntwo\n");
        let pixmap = Pixmap::decode_png(&png).unwrap();
        assert_eq!((pixmap.width(), pixmap.height()), (612, 12 + 60 + 12));
    }

    #[test]
    fn fr_11_png_draws_text_with_the_bundled_font() {
        assert_eq!(ink(&png(b"   \n")), 0, "blank paper");
        let regular = ink(&png(b"HARDLESS\n"));
        let bold = ink(&png(b"\x1bE\x01HARDLESS\n"));
        assert!(regular > 200, "text is drawn: {regular}");
        assert!(
            bold > regular,
            "bold uses the bold face: {bold} vs {regular}"
        );
    }

    #[test]
    fn fr_11_png_covers_cp437_box_drawing() {
        // ESC t 0 (CP437), then 0xC4 0xC4 0xC4: "───".
        assert!(ink(&png(b"\x1bt\x00\xc4\xc4\xc4\n")) > 20);
    }

    #[test]
    fn fr_11_png_refuses_receipts_longer_than_the_limit() {
        let lines = MAX_PNG_HEIGHT / 30 + 1;
        let data = b"x\n".repeat(lines as usize);
        let err = render_png(&parse(&data), &RenderOptions::default()).unwrap_err();
        assert!(err.to_string().contains("PNG output stops at"), "{err}");
    }
}
