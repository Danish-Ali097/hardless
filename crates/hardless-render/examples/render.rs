//! Renders an ESC/POS file to HTML, SVG or PNG on stdout:
//! `cargo run -p hardless-render --example render -- fixtures/escpos/33-full-receipt.bin 58`
//! `cargo run -p hardless-render --features png --example render -- receipt.bin 80 png > r.png`
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write as _;

use hardless_render::{render_html_document, render_svg, PaperWidth, RenderOptions};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("usage: render <file.bin> [58|80] [html|svg|png]");
    let paper = match args.next().as_deref() {
        Some("58") => PaperWidth::Mm58,
        _ => PaperWidth::Mm80,
    };
    let format = args.next().unwrap_or_else(|| "html".into());
    let data = std::fs::read(path).expect("read input");
    let ast = hardless_escpos::parse(&data);
    let options = RenderOptions {
        paper,
        show_unknown: true,
    };
    let out: Vec<u8> = match format.as_str() {
        "svg" => render_svg(&ast, &options).into_bytes(),
        #[cfg(feature = "png")]
        "png" => hardless_render::render_png(&ast, &options).expect("render PNG"),
        #[cfg(not(feature = "png"))]
        "png" => panic!("PNG output needs `--features png`"),
        _ => render_html_document(&ast, &options).into_bytes(),
    };
    std::io::stdout().write_all(&out).expect("write output");
}
