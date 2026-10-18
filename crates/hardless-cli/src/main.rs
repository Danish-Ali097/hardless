//! The `hardless` binary.
//!
//! Milestone M1 ships `hardless start` (FR-21, FR-30). `scan`, `tap`, `fault`, `receipts`
//! and `mcp` arrive in milestones M2 and M3.
#![forbid(unsafe_code)]

use std::net::IpAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use hardless_server::{Overrides, ServerConfig};

/// Hardless: POS hardware emulator for web and PWA development and testing.
#[derive(Debug, Parser)]
#[command(name = "hardless", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the server: dashboard, REST API and the bridge for the browser shim.
    Start(StartArgs),
}

#[derive(Debug, clap::Args)]
struct StartArgs {
    /// Config file (default: ./hardless.config.json if it exists). Env: HARDLESS_CONFIG.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Address to listen on. 0.0.0.0 makes it reachable from other machines.
    /// Env: HARDLESS_HOST. [default: 127.0.0.1]
    #[arg(long)]
    host: Option<IpAddr>,
    /// HTTP port for the dashboard, API and bridge. Env: HARDLESS_PORT. [default: 8080]
    #[arg(long)]
    port: Option<u16>,
    /// HTTPS port (served from milestone M2). Env: HARDLESS_HTTPS_PORT. [default: 8443]
    #[arg(long)]
    https_port: Option<u16>,
    /// Require this bearer token for the API and bridge. Env: HARDLESS_TOKEN.
    #[arg(long)]
    token: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Start(args) => start(args),
    }
}

fn start(args: StartArgs) -> ExitCode {
    let flags = Overrides {
        config: args.config,
        host: args.host,
        port: args.port,
        https_port: args.https_port,
        token: args.token,
    };
    let config = match ServerConfig::load(&|key| std::env::var(key).ok(), &flags) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("hardless: {e}");
            return ExitCode::from(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("hardless: cannot start the runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(hardless_server::serve(config)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hardless: {e}");
            ExitCode::FAILURE
        }
    }
}
