//! Embeds the built dashboard (`packages/dashboard/dist`) into the server binary. If it has
//! not been built, a placeholder page explains how to build it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let dist = manifest.join("../../packages/dashboard/dist");
    println!("cargo:rerun-if-changed={}", dist.display());

    let mut files = Vec::new();
    collect(&dist, &dist, &mut files);
    files.sort();

    let mut out = String::from(
        "/// Embedded dashboard files: (path, content type, bytes).\npub static ASSETS: &[(&str, &str, &[u8])] = &[\n",
    );
    if files.iter().any(|(name, _)| name == "index.html") {
        for (name, path) in &files {
            println!("cargo:rerun-if-changed={}", path.display());
            let _ = writeln!(
                out,
                "    ({name:?}, {:?}, include_bytes!({:?})),",
                mime(name),
                path.display().to_string()
            );
        }
    } else {
        let _ = writeln!(
            out,
            "    (\"index.html\", \"text/html; charset=utf-8\", {:?}.as_bytes()),",
            PLACEHOLDER
        );
    }
    out.push_str("];\n");
    let target = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default()).join("assets.rs");
    if let Err(e) = std::fs::write(&target, out) {
        panic!("cannot write {}: {e}", target.display());
    }
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, files);
        } else if let Ok(relative) = path.strip_prefix(root) {
            let name = relative.to_string_lossy().replace('\\', "/");
            let absolute = path.canonicalize().unwrap_or(path);
            files.push((name, absolute));
        }
    }
}

fn mime(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "json" | "map" => "application/json",
        "txt" => "text/plain; charset=utf-8",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "webmanifest" => "application/manifest+json",
        _ => "application/octet-stream",
    }
}

const PLACEHOLDER: &str = r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Hardless</title></head>
<body style="font:16px system-ui,sans-serif;max-width:40rem;margin:4rem auto;padding:0 1rem">
<h1>Hardless is running</h1>
<p>The dashboard was not built into this binary. Build it, then rebuild the server:</p>
<pre>pnpm --filter @hardless/dashboard build
cargo build -p hardless-cli</pre>
<p>The REST API is at <a href="/api/v1/devices">/api/v1/devices</a> and the shim bridge at <code>/bridge</code>.</p>
</body></html>"#;
