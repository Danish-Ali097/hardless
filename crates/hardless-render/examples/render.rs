//! Renders an ESC/POS file to an HTML document on stdout:
//! `cargo run -p hardless-render --example render -- fixtures/escpos/33-full-receipt.bin 58`
#![allow(clippy::unwrap_used, clippy::expect_used)]

use hardless_render::{render_html_document, PaperWidth, RenderOptions};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: render <file.bin> [58|80]");
    let paper = match args.next().as_deref() {
        Some("58") => PaperWidth::Mm58,
        _ => PaperWidth::Mm80,
    };
    let data = std::fs::read(path).expect("read input");
    let ast = hardless_escpos::parse(&data);
    print!(
        "{}",
        render_html_document(
            &ast,
            &RenderOptions {
                paper,
                show_unknown: true
            }
        )
    );
}
