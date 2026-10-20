//! FR-30: the CLI's `scan`, `tap`, `fault` and `receipts` against a running server.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use std::process::{Command, Output};

use hardless_server::{spawn, RunningServer, ServerConfig};

async fn server() -> RunningServer {
    spawn(ServerConfig {
        port: 0,
        https: false,
        ..ServerConfig::default()
    })
    .await
    .unwrap()
}

/// Runs the real `hardless` binary against `server`.
async fn hardless(server: &RunningServer, args: &[&str]) -> Output {
    let url = format!("http://{}", server.addr);
    let args: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
    tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_hardless"))
            .args(&args)
            .env("HARDLESS_SERVER", url)
            .env_remove("HARDLESS_TOKEN")
            .output()
            .unwrap()
    })
    .await
    .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn fr_30_scan_reaches_the_scanner() {
    let server = server().await;
    let out = hardless(
        &server,
        &["scan", "scanner-1", "4006381333931", "--terminator", "tab"],
    )
    .await;
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "Scanned 4006381333931 on scanner-1.\n");
    let state = server
        .engine
        .summary(&"scanner-1".parse().unwrap())
        .unwrap()
        .state;
    assert_eq!(state["lastCode"], "4006381333931");
}

#[tokio::test(flavor = "multi_thread")]
async fn fr_30_fault_turns_faults_on_and_off_with_options() {
    let server = server().await;
    let printer = "printer-1".parse().unwrap();
    let out = hardless(
        &server,
        &[
            "fault",
            "printer-1",
            "disconnect-mid-write",
            "on",
            "--option",
            "afterBytes=32",
        ],
    )
    .await;
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "disconnect-mid-write is on for printer-1.\n");
    let summary = server.engine.summary(&printer).unwrap();
    assert_eq!(summary.faults[0].options["afterBytes"], 32);

    let out = hardless(
        &server,
        &["fault", "printer-1", "disconnect-mid-write", "off"],
    )
    .await;
    assert!(out.status.success());
    assert!(server.engine.summary(&printer).unwrap().faults.is_empty());

    let out = hardless(&server, &["fault", "printer-1", "jammed", "on"]).await;
    assert!(!out.status.success(), "a drawer fault on a printer");
    assert!(stderr(&out).contains("jammed"), "{}", stderr(&out));
}

#[tokio::test(flavor = "multi_thread")]
async fn fr_30_receipts_lists_text_and_saves_renders() {
    let server = server().await;
    let printer = "printer-1".parse().unwrap();
    let out = hardless(&server, &["receipts", "printer-1", "--last"]).await;
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("printer-1 has no receipts yet"),
        "{}",
        stderr(&out)
    );

    for text in ["First\n", "Total: 1,250\n"] {
        let bytes = format!("{text}\x1dV\x00");
        server
            .engine
            .drive(&printer, |r| r.host_bytes(&printer, bytes.as_bytes()))
            .unwrap();
    }
    let out = hardless(&server, &["receipts", "printer-1", "--last"]).await;
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "Total: 1,250\n");

    let out = hardless(&server, &["receipts", "printer-1"]).await;
    assert_eq!(
        stdout(&out),
        "--- receipt 2 ---\nTotal: 1,250\n\n--- receipt 1 ---\nFirst\n"
    );

    let out = hardless(
        &server,
        &["receipts", "printer-1", "--last", "--format", "json"],
    )
    .await;
    let job: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(job["id"], 2);

    let png = std::env::temp_dir().join(format!("hardless-cli-{}.png", std::process::id()));
    let out = hardless(
        &server,
        &[
            "receipts",
            "printer-1",
            "--format",
            "png",
            "--out",
            png.to_str().unwrap(),
        ],
    )
    .await;
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(std::fs::read(&png).unwrap().starts_with(b"\x89PNG"));
    std::fs::remove_file(png).ok();

    let out = hardless(&server, &["receipts", "printer-1", "--format", "png"]).await;
    assert!(stderr(&out).contains("PNG output needs --out FILE"));
}

#[tokio::test(flavor = "multi_thread")]
async fn fr_30_tap_and_clear_errors() {
    let server = server().await;
    let out = hardless(&server, &["tap", "scanner-1", "--uid", "04a2b3c4"]).await;
    assert!(out.status.success(), "{}", stderr(&out));
    let out = hardless(&server, &["scan", "nope", "1"]).await;
    assert!(!out.status.success());
    assert!(stderr(&out).contains("nope"), "{}", stderr(&out));
}

#[test]
fn fr_30_an_unreachable_server_says_how_to_fix_it() {
    let out = Command::new(env!("CARGO_BIN_EXE_hardless"))
        .args(["scan", "scanner-1", "1", "--server", "http://127.0.0.1:9"])
        .env_remove("HARDLESS_SERVER")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("Is `hardless start` running?"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn nfr_14_licenses_prints_the_third_party_notices() {
    let out = Command::new(env!("CARGO_BIN_EXE_hardless"))
        .arg("licenses")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = stdout(&out);
    for part in [
        "Part 1: Rust crates compiled into the binary",
        "Part 2: Go Mono font",
        "Bigelow & Holmes",
        "Part 3: The dashboard",
        "react 18.",
    ] {
        assert!(text.contains(part), "missing {part}");
    }
}
