//! The `hardless` binary.
//!
//! Milestone M0 only reports its version. Subcommands (`start`, `scan`, `tap`, `fault`,
//! `receipts`, `mcp`) arrive with FR-30 in milestone M2.
#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let arg = std::env::args().nth(1);
    match arg.as_deref() {
        None | Some("--version") | Some("-V") => {
            println!("hardless {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("hardless: unknown command `{other}` (subcommands arrive in milestone M2)");
            ExitCode::from(2)
        }
    }
}
