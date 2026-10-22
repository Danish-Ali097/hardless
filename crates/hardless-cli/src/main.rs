//! The `hardless` binary (FR-30).
//!
//! `hardless start` runs the server. `scan`, `tap`, `fault` and `receipts` control a running
//! server through its REST API. `mcp` serves the MCP server for coding agents over stdio
//! (FR-43). `licenses` prints the third-party notices.
#![forbid(unsafe_code)]

mod client;
mod mcp;

use std::net::IpAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use hardless_server::{Overrides, ServerConfig};
use hyper::Method;
use serde_json::{json, Map, Value};

use client::{Client, ClientError};

/// Hardless: POS hardware emulator for web and PWA development and testing.
#[derive(Debug, Parser)]
#[command(name = "hardless", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the server: dashboard, REST API, the bridge for the browser shim and the HTTP
    /// printers, on HTTP and HTTPS.
    Start(StartArgs),
    /// Scan a barcode on a keyboard-wedge scanner: pages with the shim type it.
    Scan(ScanArgs),
    /// Tap an NFC card on a reader.
    Tap(TapArgs),
    /// Turn a fault on or off, e.g. `hardless fault printer-1 paper-out on`.
    Fault(FaultArgs),
    /// Show a printer's receipts, or save one as HTML, SVG or PNG.
    Receipts(ReceiptsArgs),
    /// Serve the MCP server for AI coding agents over stdin/stdout, against a running server
    /// (or, with --embedded, one started here).
    Mcp(McpArgs),
    /// Print the licences of the third-party software in this binary.
    Licenses,
}

#[derive(Debug, clap::Args)]
struct McpArgs {
    /// The Hardless server's address.
    #[arg(long, env = "HARDLESS_SERVER", default_value = "http://127.0.0.1:8080")]
    url: String,
    /// The server's bearer token, if it has one.
    #[arg(long, env = "HARDLESS_TOKEN", hide_env_values = true)]
    token: Option<String>,
    /// Offer only the tools that read (no device changes, scans, taps or faults).
    #[arg(long)]
    read_only: bool,
    /// Start a Hardless server in this process when none answers at --url.
    #[arg(long)]
    embedded: bool,
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
    /// HTTPS port: the same dashboard, API, bridge and HTTP printers over TLS, with a local
    /// CA to trust once per device. Env: HARDLESS_HTTPS_PORT (HARDLESS_HTTPS=0 turns it off).
    /// [default: 8443]
    #[arg(long)]
    https_port: Option<u16>,
    /// Require this bearer token for the API and bridge. Env: HARDLESS_TOKEN.
    #[arg(long)]
    token: Option<String>,
}

/// Which server a command talks to.
#[derive(Debug, clap::Args)]
struct Remote {
    /// The running server's address.
    #[arg(long, env = "HARDLESS_SERVER", default_value = "http://127.0.0.1:8080")]
    server: String,
    /// The server's bearer token, if it has one.
    #[arg(long, env = "HARDLESS_TOKEN", hide_env_values = true)]
    token: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TerminatorArg {
    Enter,
    Tab,
    None,
}

#[derive(Debug, clap::Args)]
struct ScanArgs {
    /// The scanner, e.g. scanner-1.
    device: String,
    /// The barcode.
    code: String,
    /// The key after the code (default: the scanner's setting).
    #[arg(long, value_enum)]
    terminator: Option<TerminatorArg>,
    /// Milliseconds between key presses (default: the scanner's setting).
    #[arg(long)]
    delay_ms: Option<u64>,
    #[command(flatten)]
    remote: Remote,
}

#[derive(Debug, clap::Args)]
struct TapArgs {
    /// The NFC reader.
    device: String,
    /// The card's UID in hex, e.g. 04a2b3c4d5e6f7.
    #[arg(long)]
    uid: String,
    /// NDEF records as a JSON array.
    #[arg(long)]
    records: Option<String>,
    #[command(flatten)]
    remote: Remote,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OnOff {
    On,
    Off,
}

#[derive(Debug, clap::Args)]
struct FaultArgs {
    /// The device.
    device: String,
    /// The fault, e.g. paper-out (listed in the dashboard's fault panel and at <https://hardless-docs.netlify.app/reference/faults>).
    fault: String,
    /// Turn it on or off.
    state: OnOff,
    /// A fault option, e.g. --option afterBytes=32. Repeatable.
    #[arg(long = "option", value_name = "KEY=VALUE")]
    options: Vec<String>,
    #[command(flatten)]
    remote: Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Text,
    Json,
    Html,
    Svg,
    Png,
}

#[derive(Debug, clap::Args)]
struct ReceiptsArgs {
    /// The printer.
    device: String,
    /// Only the latest receipt.
    #[arg(long)]
    last: bool,
    /// How many receipts to list, newest first.
    #[arg(long, default_value_t = 10)]
    limit: usize,
    /// Output: text and json list receipts; html, svg and png render the latest one.
    #[arg(long, value_enum, default_value = "text")]
    format: Format,
    /// Paper width for html, svg and png: 58 or 80 (default: the printer's).
    #[arg(long)]
    width: Option<u16>,
    /// Write to this file instead of standard output (required for png).
    #[arg(long)]
    out: Option<PathBuf>,
    #[command(flatten)]
    remote: Remote,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Start(args) => start(args),
        Command::Licenses => licenses(),
        Command::Mcp(args) => mcp::run(&args.url, args.token, args.read_only, args.embedded),
        Command::Scan(args) => {
            let args = &args;
            remote(&args.remote, |c| async move { scan(&c, args).await })
        }
        Command::Tap(args) => {
            let args = &args;
            remote(&args.remote, |c| async move { tap(&c, args).await })
        }
        Command::Fault(args) => {
            let args = &args;
            remote(&args.remote, |c| async move { fault(&c, args).await })
        }
        Command::Receipts(args) => {
            let args = &args;
            remote(&args.remote, |c| async move { receipts(&c, args).await })
        }
    }
}

/// Runs a command against a running server, printing its error and exiting 1 on failure.
fn remote<F, Fut>(remote: &Remote, command: F) -> ExitCode
where
    F: FnOnce(Client) -> Fut,
    Fut: std::future::Future<Output = Result<(), ClientError>>,
{
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("hardless: cannot start the runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = runtime.block_on(async {
        let client = Client::new(&remote.server, remote.token.clone())?;
        command(client).await
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hardless: {e}");
            ExitCode::FAILURE
        }
    }
}

fn device_path(device: &str) -> String {
    format!("/devices/{}", percent_encode(device))
}

/// Device IDs are letters, digits, `-`, `_` and `.`; anything else is encoded so a typo
/// cannot change the path.
fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

async fn scan(client: &Client, args: &ScanArgs) -> Result<(), ClientError> {
    let mut body = json!({ "code": args.code });
    if let Some(terminator) = args.terminator {
        body["terminator"] = json!(match terminator {
            TerminatorArg::Enter => "enter",
            TerminatorArg::Tab => "tab",
            TerminatorArg::None => "none",
        });
    }
    if let Some(ms) = args.delay_ms {
        body["delayMs"] = json!(ms);
    }
    let path = format!("{}/scan", device_path(&args.device));
    client.json(Method::POST, &path, Some(&body)).await?;
    println!("Scanned {} on {}.", args.code, args.device);
    Ok(())
}

async fn tap(client: &Client, args: &TapArgs) -> Result<(), ClientError> {
    let records: Value = match &args.records {
        Some(json) => serde_json::from_str(json)
            .map_err(|e| ClientError::Other(format!("--records is not JSON: {e}")))?,
        None => json!([]),
    };
    let body = json!({ "uid": args.uid, "records": records });
    let path = format!("{}/tap", device_path(&args.device));
    client.json(Method::POST, &path, Some(&body)).await?;
    println!("Tapped card {} on {}.", args.uid, args.device);
    Ok(())
}

/// `KEY=VALUE` options; values that read as JSON (numbers, booleans) are sent as such.
fn fault_options(options: &[String]) -> Result<Map<String, Value>, ClientError> {
    let mut map = Map::new();
    for option in options {
        let (key, value) = option
            .split_once('=')
            .ok_or_else(|| ClientError::Other(format!("--option {option}: expected KEY=VALUE")))?;
        let value = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_owned()));
        map.insert(key.to_owned(), value);
    }
    Ok(map)
}

async fn fault(client: &Client, args: &FaultArgs) -> Result<(), ClientError> {
    let active = matches!(args.state, OnOff::On);
    let mut body = json!({ "active": active });
    let options = fault_options(&args.options)?;
    if !options.is_empty() {
        body["options"] = Value::Object(options);
    }
    let path = format!(
        "{}/faults/{}",
        device_path(&args.device),
        percent_encode(&args.fault)
    );
    client.json(Method::PUT, &path, Some(&body)).await?;
    let state = if active { "on" } else { "off" };
    println!("{} is {state} for {}.", args.fault, args.device);
    Ok(())
}

async fn receipts(client: &Client, args: &ReceiptsArgs) -> Result<(), ClientError> {
    let device = device_path(&args.device);
    let renders = matches!(args.format, Format::Html | Format::Svg | Format::Png);
    if args.format == Format::Png && args.out.is_none() {
        return Err(ClientError::Other("PNG output needs --out FILE".into()));
    }
    let jobs: Vec<Value> = if args.last || renders {
        let (status, bytes) = client
            .send(Method::GET, &format!("{device}/jobs/last"), None)
            .await?;
        if status == 404 {
            // An unknown device is an error; a printer with no receipts says so below.
            client.call(Method::GET, &device, None).await?;
            Vec::new()
        } else if (200..300).contains(&status) {
            let job: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            if job.is_null() {
                Vec::new()
            } else {
                vec![job]
            }
        } else {
            client
                .call(Method::GET, &format!("{device}/jobs/last"), None)
                .await?;
            Vec::new()
        }
    } else {
        let list = client
            .json(
                Method::GET,
                &format!("{device}/jobs?limit={}", args.limit),
                None,
            )
            .await?;
        list.as_array().cloned().unwrap_or_default()
    };
    let Some(latest) = jobs.first() else {
        return Err(ClientError::Other(format!(
            "{} has no receipts yet",
            args.device
        )));
    };

    let output: Vec<u8> = match args.format {
        Format::Text => jobs
            .iter()
            .map(|job| {
                let header = if args.last {
                    String::new()
                } else {
                    format!("--- receipt {} ---\n", job["id"])
                };
                format!("{header}{}", job["text"].as_str().unwrap_or(""))
            })
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes(),
        Format::Json => {
            let value = if args.last {
                latest.clone()
            } else {
                Value::Array(jobs.clone())
            };
            let mut text = serde_json::to_string_pretty(&value)
                .map_err(|e| ClientError::Other(e.to_string()))?;
            text.push('\n');
            text.into_bytes()
        }
        Format::Html | Format::Svg | Format::Png => {
            let format = match args.format {
                Format::Html => "html",
                Format::Svg => "svg",
                _ => "png",
            };
            let width = args.width.map_or(String::new(), |w| format!("&width={w}"));
            let path = format!(
                "{device}/jobs/{}/render?format={format}{width}",
                latest["id"]
            );
            client.call(Method::GET, &path, None).await?.to_vec()
        }
    };
    match &args.out {
        Some(path) => {
            std::fs::write(path, &output)
                .map_err(|e| ClientError::Other(format!("cannot write {}: {e}", path.display())))?;
            println!("Saved receipt {} to {}.", latest["id"], path.display());
        }
        None => {
            use std::io::Write as _;
            let mut stdout = std::io::stdout();
            let _ = stdout.write_all(&output);
            if !output.ends_with(b"\n") {
                let _ = stdout.write_all(b"\n");
            }
        }
    }
    Ok(())
}

/// Third-party notices (NFR-14), generated by `cargo xtask licenses` and embedded so they
/// travel with the binary.
const THIRD_PARTY_LICENSES: &str = include_str!("../THIRD_PARTY_LICENSES.txt");

fn licenses() -> ExitCode {
    use std::io::Write as _;
    // A closed pipe (e.g. `| head`) is not an error worth reporting.
    let _ = std::io::stdout()
        .lock()
        .write_all(THIRD_PARTY_LICENSES.as_bytes());
    ExitCode::SUCCESS
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fault_options_read_json_values() {
        let options =
            fault_options(&["afterBytes=32".into(), "rate=0.5".into(), "name=x".into()]).unwrap();
        assert_eq!(
            Value::Object(options),
            json!({ "afterBytes": 32, "rate": 0.5, "name": "x" })
        );
        assert!(fault_options(&["nope".into()]).is_err());
    }

    #[test]
    fn device_ids_are_encoded_in_paths() {
        assert_eq!(device_path("local_printer"), "/devices/local_printer");
        assert_eq!(device_path("a/b"), "/devices/a%2Fb");
    }

    #[test]
    fn cli_parses() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
