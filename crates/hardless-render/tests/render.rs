//! FR-10, FR-12, FR-13: HTML and text rendering.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use hardless_escpos::parse;
use hardless_render::{
    render_html, render_html_document, to_text, to_text_with, PaperWidth, RenderOptions,
};

const MM58: RenderOptions = RenderOptions {
    paper: PaperWidth::Mm58,
    show_unknown: true,
};

fn lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

#[test]
fn fr_10_paper_widths_have_the_printer_column_counts() {
    assert_eq!(PaperWidth::Mm58.columns(), (32, 42));
    assert_eq!(PaperWidth::Mm80.columns(), (48, 64));
    let html = render_html(&parse(b"x\n"), &MM58);
    assert!(
        html.contains("hl-paper-58") && html.contains("--hl-cols:32"),
        "{html}"
    );
}

#[test]
fn fr_10_long_lines_wrap_at_the_column_count() {
    let text = to_text_with(&parse(&[b'x'; 40]), &MM58);
    assert_eq!(lines(&text), ["x".repeat(32), "x".repeat(8)]);
    // Font B fits more characters per line.
    let font_b = to_text_with(&parse(&[&b"\x1bM\x01"[..], &[b'x'; 40]].concat()), &MM58);
    assert_eq!(lines(&font_b), ["x".repeat(40)]);
}

#[test]
fn fr_10_double_width_characters_take_two_columns() {
    let mut data = b"\x1d!\x10".to_vec();
    data.extend([b'W'; 20]);
    let text = to_text_with(&parse(&data), &MM58);
    assert_eq!(lines(&text), ["W".repeat(16), "W".repeat(4)]);
}

#[test]
fn fr_10_alignment_bold_underline_and_size_in_html() {
    let html = render_html(
        &parse(b"\x1ba\x01\x1bE\x01\x1b-\x02\x1d!\x11Big\n"),
        &RenderOptions::default(),
    );
    assert!(html.contains("hl-align-center"), "{html}");
    assert!(html.contains(r#"class="hl-b hl-u2 hl-s""#), "{html}");
    assert!(html.contains("--w:2;--h:2;--n:3"), "{html}");
}

#[test]
fn fr_10_alignment_applies_from_the_start_of_a_line() {
    // ESC a mid-line affects the next line, like on a printer.
    let text = to_text(&parse(b"left\x1ba\x02\nright\n"));
    assert_eq!(lines(&text)[0], "left");
    assert_eq!(lines(&text)[1], format!("{}right", " ".repeat(43)));
}

#[test]
fn fr_10_init_clears_buffered_text_and_formatting() {
    let text = to_text(&parse(b"\x1ba\x01lost\x1b@kept\n"));
    assert_eq!(text, "kept\n");
}

#[test]
fn fr_12_cuts_are_drawn_full_and_partial() {
    let html = render_html(
        &parse(b"a\n\x1dV\x00b\n\x1dV\x01"),
        &RenderOptions::default(),
    );
    assert_eq!(html.matches("hl-cut").count(), 3, "{html}");
    assert!(html.contains("hl-cut hl-cut-partial"));
}

#[test]
fn fr_12_qr_codes_are_drawn_as_svg() {
    let data = b"\x1d(k\x03\x001C\x06\x1d(k\x08\x001P0hello\x1d(k\x03\x001Q0";
    let html = render_html(&parse(data), &RenderOptions::default());
    assert!(
        html.contains(r#"<svg style="width:"#) && html.contains("hl-qr"),
        "{html}"
    );
    assert_eq!(to_text(&parse(data)), "[QR: hello]\n");
}

#[test]
fn fr_12_barcodes_are_drawn_with_hri_text() {
    let data = b"\x1dH\x02\x1dkI\x09{BABC-123";
    let html = render_html(&parse(data), &RenderOptions::default());
    assert!(html.contains("hl-barcode"), "{html}");
    assert!(
        html.contains(r#"<div class="hl-hri">ABC-123</div>"#),
        "{html}"
    );
    assert_eq!(to_text(&parse(data)), "[BARCODE CODE128: ABC-123]\n");

    for data in [&b"\x1dk\x02400638133393\x00"[..], b"\x1dk\x04HARDLESS\x00"] {
        let html = render_html(&parse(data), &RenderOptions::default());
        assert!(
            html.contains("hl-barcode") && !html.contains("hl-error"),
            "{html}"
        );
    }
}

#[test]
fn fr_12_invalid_barcode_data_shows_an_error_instead_of_failing() {
    let html = render_html(&parse(b"\x1dkC\x03abc"), &RenderOptions::default());
    assert!(html.contains("hl-error"), "{html}");
}

#[test]
fn fr_12_raster_images_are_drawn() {
    let data = b"\x1dv0\x00\x01\x00\x02\x00\xff\x81";
    let html = render_html(&parse(data), &RenderOptions::default());
    assert!(
        html.contains("hl-image") && html.contains("M0 0h8v1h-8z"),
        "{html}"
    );
    assert_eq!(to_text(&parse(data)), "[IMAGE 8x2]\n");
}

#[test]
fn fr_12_unknown_commands_show_a_badge_that_can_be_turned_off() {
    let ast = parse(b"a\x1b3\x18b\n");
    let with = render_html(&ast, &RenderOptions::default());
    assert!(
        with.contains(r#"class="hl-unknown""#) && with.contains("ESC 3"),
        "{with}"
    );
    let without = render_html(
        &ast,
        &RenderOptions {
            show_unknown: false,
            ..Default::default()
        },
    );
    assert!(!without.contains("hl-unknown"), "{without}");
    assert_eq!(to_text(&ast), "ab\n");
}

#[test]
fn fr_13_to_text_extracts_the_receipt_for_assertions() {
    let ast =
        parse(b"\x1ba\x01HARDLESS\n\x1ba\x00Total:   1,250\n\x1bd\x01\x1bp\x00\x19\xfa\x1dV\x00");
    assert_eq!(
        to_text(&ast),
        format!("{}HARDLESS\nTotal:   1,250\n\n", " ".repeat(20))
    );
}

#[test]
fn receipt_text_is_escaped_in_html() {
    let html = render_html(
        &parse(b"<script>alert('x')</script> & \"q\"\n\x1dkE\x03<&>"),
        &RenderOptions::default(),
    );
    assert!(!html.contains("<script>"), "{html}");
    assert!(html.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt; &amp; &quot;q&quot;"));
}

#[test]
fn full_document_includes_the_stylesheet() {
    let doc = render_html_document(&parse(b"x\n"), &RenderOptions::default());
    assert!(doc.starts_with("<!doctype html>") && doc.contains(".hl-receipt{"));
}
