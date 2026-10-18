//! Golden fixtures: HTML and text snapshots for every `fixtures/escpos/*.bin`
//! (FR-10, FR-12, FR-13).
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use std::fs;
use std::path::{Path, PathBuf};

use hardless_escpos::parse;
use hardless_render::{render_html, to_text, RenderOptions};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/escpos")
}

#[test]
fn fr_12_golden_fixtures_render_to_their_snapshots() {
    let mut files: Vec<PathBuf> = fs::read_dir(fixtures_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "bin"))
        .collect();
    files.sort();
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(fixtures_dir().join("snapshots"));
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        for path in files {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let ast = parse(&fs::read(&path).unwrap());
            let html = render_html(&ast, &RenderOptions::default()).replace("><", ">\n<");
            insta::assert_snapshot!(format!("{name}.html"), html);
            insta::assert_snapshot!(format!("{name}.text"), to_text(&ast));
        }
    });
}
